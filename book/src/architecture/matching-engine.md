# Matching Engine

The matching engine lives in `crates/engine/` — a pure Rust library with zero I/O dependencies.

## Price-Time Priority

The algorithm used by every major exchange (NASDAQ, CME, Binance, Polymarket):

1. **Price first** — for a buy, match the lowest ask. For a sell, match the highest bid.
2. **Time second** — at the same price, the earlier order matches first.
3. **Maker price** — fills execute at the resting (maker) order's price, not the taker's.

## The Core Function

```rust
pub fn match_order(mut incoming: Order, book: &mut OrderBook) -> Vec<Fill> {
    let mut fills = Vec::new();

    match incoming.side {
        Side::Buy  => match_buy(&mut incoming, book, &mut fills),
        Side::Sell => match_sell(&mut incoming, book, &mut fills),
    }

    // Unfilled remainder rests on the book
    if incoming.qty > 0 {
        book.add_resting_order(incoming);
    }

    book.sequence += 1;
    fills
}
```

Takes an `Order` and a mutable `OrderBook`. Returns fills. The book is mutated in place. No I/O, no async, no Redis — pure computation.

## How match_buy Works

```rust
while incoming.qty > 0 {
    let best_ask_price = match book.best_ask() {
        Some(p) => p,
        None => break,       // No sellers
    };

    if incoming.price < best_ask_price {
        break;               // Price too low
    }

    // Walk the price level FIFO, filling orders
    // ...
}
```

The sell-side (`match_sell`) is symmetric — matches against highest bids first.

## Price Priority in Action

```mermaid
flowchart LR
    subgraph Input
        O["Incoming Order<br/>Buy 25 @ 510"]
    end

    subgraph Book["Order Book — asks side"]
        L1["490: Sell qty=10"]
        L2["500: Sell qty=10"]
        L3["510: Sell qty=10"]
    end

    subgraph Output["Generated Fills"]
        F1["Fill price=490 qty=10"]
        F2["Fill price=500 qty=10"]
        F3["Fill price=510 qty=5"]
    end

    O --> L1
    L1 --> F1
    O --> L2
    L2 --> F2
    O --> L3
    L3 --> F3

    F3 -. "5 remaining" .-> L3

    style Input fill:#1565c0,color:#fff
    style Output fill:#2e7d32,color:#fff
```

## Partial Fills

When an incoming order is larger than available liquidity:

```mermaid
sequenceDiagram
    participant I as Incoming Buy 60@100
    participant B as Book asks at 100
    participant F as Fills

    Note over B: [Sell qty=30, Sell qty=50]

    I->>B: Match first sell (qty=30)
    B->>F: Fill qty=30 (sell fully consumed)
    I->>B: Match second sell (30 of 50)
    B->>F: Fill qty=30 (sell partially consumed)

    Note over I: Fully consumed (30+30=60)
    Note over B: [Sell qty=20] remaining
```

## Test Coverage

```bash
$ cargo test -p engine
running 9 tests
test test_no_match_rests_on_book       ... ok
test test_full_match                   ... ok
test test_partial_fill_incoming_larger ... ok
test test_partial_fill_resting_larger  ... ok
test test_price_priority               ... ok
test test_time_priority                ... ok
test test_fills_sum_to_matched_qty     ... ok
test test_no_match_price_too_low       ... ok
test test_sell_matches_highest_bid_first ... ok
```
