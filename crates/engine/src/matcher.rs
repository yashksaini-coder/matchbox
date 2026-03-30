use std::time::{SystemTime, UNIX_EPOCH};

use crate::book::OrderBook;
use crate::models::{Fill, Order, Side};

/// Process an incoming order against the book.
/// Returns the list of fills generated.
/// The book is mutated in place.
/// If the order is not fully consumed, the remainder is added as a resting order.
pub fn match_order(mut incoming: Order, book: &mut OrderBook) -> Vec<Fill> {
    let mut fills = Vec::new();

    match incoming.side {
        Side::Buy => match_buy(&mut incoming, book, &mut fills),
        Side::Sell => match_sell(&mut incoming, book, &mut fills),
    }

    // If any qty remains unfilled, rest it on the book
    if incoming.qty > 0 {
        book.add_resting_order(incoming);
    }

    book.sequence += 1;
    fills
}

/// A buy order matches against asks (sellers).
/// Match the lowest asks first (price priority).
fn match_buy(incoming: &mut Order, book: &mut OrderBook, fills: &mut Vec<Fill>) {
    let now = now_nanos();

    while incoming.qty > 0 {
        let best_ask_price = match book.best_ask() {
            Some(p) => p,
            None => break,
        };

        if incoming.price < best_ask_price {
            break;
        }

        let mut filled_ids = Vec::new();
        let level_empty;

        {
            let level = book.asks_mut().get_mut(&best_ask_price).unwrap();

            while incoming.qty > 0 && !level.is_empty() {
                let maker = level.front_mut().unwrap();
                let fill_qty = incoming.qty.min(maker.qty);

                fills.push(Fill {
                    maker_order_id: maker.id,
                    taker_order_id: incoming.id,
                    price: best_ask_price,
                    qty: fill_qty,
                    timestamp: now,
                });

                maker.qty -= fill_qty;
                incoming.qty -= fill_qty;

                if maker.qty == 0 {
                    let filled = level.pop_front().unwrap();
                    filled_ids.push(filled.id);
                }
            }

            level_empty = level.is_empty();
        }

        for id in filled_ids {
            book.remove_from_index(id);
        }
        if level_empty {
            book.asks_mut().remove(&best_ask_price);
        }
    }
}

/// A sell order matches against bids (buyers).
/// Match the highest bids first (price priority).
fn match_sell(incoming: &mut Order, book: &mut OrderBook, fills: &mut Vec<Fill>) {
    let now = now_nanos();

    while incoming.qty > 0 {
        let best_bid_price = match book.best_bid() {
            Some(p) => p,
            None => break,
        };

        if incoming.price > best_bid_price {
            break;
        }

        let mut filled_ids = Vec::new();
        let level_empty;

        {
            let level = book.bids_mut().get_mut(&best_bid_price).unwrap();

            while incoming.qty > 0 && !level.is_empty() {
                let maker = level.front_mut().unwrap();
                let fill_qty = incoming.qty.min(maker.qty);

                fills.push(Fill {
                    maker_order_id: maker.id,
                    taker_order_id: incoming.id,
                    price: best_bid_price,
                    qty: fill_qty,
                    timestamp: now,
                });

                maker.qty -= fill_qty;
                incoming.qty -= fill_qty;

                if maker.qty == 0 {
                    let filled = level.pop_front().unwrap();
                    filled_ids.push(filled.id);
                }
            }

            level_empty = level.is_empty();
        }

        for id in filled_ids {
            book.remove_from_index(id);
        }
        if level_empty {
            book.bids_mut().remove(&best_bid_price);
        }
    }
}

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Side;

    fn make_order(id: u64, side: Side, price: u64, qty: u64) -> Order {
        Order {
            id,
            side,
            price,
            qty,
            timestamp: id * 1000,
        }
    }

    #[test]
    fn test_no_match_rests_on_book() {
        let mut book = OrderBook::new();
        let sell = make_order(1, Side::Sell, 100, 10);
        let fills = match_order(sell, &mut book);
        assert!(fills.is_empty(), "No match possible — no bids");
        assert_eq!(book.best_ask(), Some(100));
    }

    #[test]
    fn test_full_match() {
        let mut book = OrderBook::new();
        match_order(make_order(1, Side::Sell, 100, 10), &mut book);
        let fills = match_order(make_order(2, Side::Buy, 100, 10), &mut book);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].qty, 10);
        assert_eq!(fills[0].price, 100);
        assert_eq!(fills[0].maker_order_id, 1);
        assert_eq!(fills[0].taker_order_id, 2);
        assert!(book.best_ask().is_none());
        assert!(book.best_bid().is_none());
    }

    #[test]
    fn test_partial_fill_incoming_larger() {
        let mut book = OrderBook::new();
        match_order(make_order(1, Side::Sell, 100, 30), &mut book);
        let fills = match_order(make_order(2, Side::Buy, 100, 50), &mut book);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].qty, 30); // Only 30 available
                                      // Remaining 20 from buy order rests on bids
        assert_eq!(book.best_bid(), Some(100));
        assert!(book.best_ask().is_none());
    }

    #[test]
    fn test_partial_fill_resting_larger() {
        let mut book = OrderBook::new();
        match_order(make_order(1, Side::Sell, 100, 50), &mut book);
        let fills = match_order(make_order(2, Side::Buy, 100, 20), &mut book);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].qty, 20);
        // Resting sell still has 30 remaining
        assert_eq!(book.best_ask(), Some(100));
        assert!(book.best_bid().is_none());
    }

    #[test]
    fn test_price_priority() {
        let mut book = OrderBook::new();
        // Three sells at different prices
        match_order(make_order(1, Side::Sell, 110, 10), &mut book);
        match_order(make_order(2, Side::Sell, 100, 10), &mut book);
        match_order(make_order(3, Side::Sell, 105, 10), &mut book);

        // Buy at 110 — should match 100 first, then 105
        let fills = match_order(make_order(4, Side::Buy, 110, 20), &mut book);
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].price, 100); // Price priority: lowest ask first
        assert_eq!(fills[1].price, 105);
    }

    #[test]
    fn test_time_priority() {
        let mut book = OrderBook::new();
        // Two sells at same price — order 1 first
        match_order(make_order(1, Side::Sell, 100, 10), &mut book);
        match_order(make_order(2, Side::Sell, 100, 10), &mut book);

        let fills = match_order(make_order(3, Side::Buy, 100, 10), &mut book);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].maker_order_id, 1); // Time priority: earlier order filled first
    }

    #[test]
    fn test_fills_sum_to_matched_qty() {
        let mut book = OrderBook::new();
        // Three sells: 30 + 40 + 50 = 120 total
        match_order(make_order(1, Side::Sell, 490, 10), &mut book);
        match_order(make_order(2, Side::Sell, 500, 10), &mut book);
        match_order(make_order(3, Side::Sell, 510, 10), &mut book);

        // Buy 25 at price 510
        let fills = match_order(make_order(4, Side::Buy, 510, 25), &mut book);
        let total_filled: u64 = fills.iter().map(|f| f.qty).sum();
        assert_eq!(total_filled, 25);
        assert_eq!(fills.len(), 3);
        assert_eq!(fills[0].qty, 10); // 490 level fully consumed
        assert_eq!(fills[1].qty, 10); // 500 level fully consumed
        assert_eq!(fills[2].qty, 5); // 510 level partially consumed
    }

    #[test]
    fn test_no_match_price_too_low() {
        let mut book = OrderBook::new();
        match_order(make_order(1, Side::Sell, 100, 10), &mut book);
        // Buy at 90 — below the ask of 100
        let fills = match_order(make_order(2, Side::Buy, 90, 10), &mut book);
        assert!(fills.is_empty());
        // Both orders should rest
        assert_eq!(book.best_ask(), Some(100));
        assert_eq!(book.best_bid(), Some(90));
    }

    #[test]
    fn test_sell_matches_highest_bid_first() {
        let mut book = OrderBook::new();
        match_order(make_order(1, Side::Buy, 90, 10), &mut book);
        match_order(make_order(2, Side::Buy, 100, 10), &mut book);
        match_order(make_order(3, Side::Buy, 95, 10), &mut book);

        // Sell at 90 — should match 100 first (highest bid), then 95
        let fills = match_order(make_order(4, Side::Sell, 90, 20), &mut book);
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].price, 100); // Highest bid first
        assert_eq!(fills[1].price, 95);
    }
}
