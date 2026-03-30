use std::time::{Duration, Instant};

use fred::prelude::*;

use engine::book::OrderBook;
use engine::matcher::match_order;
use engine::models::{Fill, Order};

use crate::state::AppState;

const LEADER_KEY: &str = "engine:leader";
const LEADER_TTL_SECS: i64 = 30;
const LEADER_REFRESH_SECS: u64 = 10;

/// Try to become the engine leader.
/// Uses SET NX EX — only set if not exists, with TTL.
/// Returns true if this instance acquired the lock.
async fn try_become_leader(redis: &Client, instance_id: &str) -> bool {
    redis
        .set::<(), _, _>(
            LEADER_KEY,
            instance_id,
            Some(Expiration::EX(LEADER_TTL_SECS)),
            Some(SetOptions::NX),
            false,
        )
        .await
        .is_ok()
}

/// Refresh the leader lock (must be called before TTL expires).
/// Only refreshes if the current value matches our instance_id (compare-and-refresh via Lua).
async fn refresh_leader_lock(redis: &Client, instance_id: &str) -> bool {
    let script = r#"
        if redis.call('get', KEYS[1]) == ARGV[1] then
            return redis.call('expire', KEYS[1], ARGV[2])
        else
            return 0
        end
    "#;

    redis
        .eval::<i64, _, _, _>(
            script,
            vec![LEADER_KEY],
            vec![instance_id, &LEADER_TTL_SECS.to_string()],
        )
        .await
        .map(|r| r == 1)
        .unwrap_or(false)
}

/// The main engine loop. Only runs on the leader instance.
pub async fn engine_worker_loop(state: AppState) {
    loop {
        // Try to become leader
        if !try_become_leader(&state.redis, &state.instance_id).await {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        tracing::info!("Became engine leader: {}", state.instance_id);

        let mut book = OrderBook::new();
        let mut last_refresh = Instant::now();

        // Leader loop
        loop {
            // Refresh leadership lock periodically
            if last_refresh.elapsed().as_secs() >= LEADER_REFRESH_SECS {
                if !refresh_leader_lock(&state.redis, &state.instance_id).await {
                    tracing::warn!("Lost engine leadership! Another instance took over.");
                    break;
                }
                last_refresh = Instant::now();
            }

            // Non-blocking pop from the queue.
            // Fred's connection pool doesn't support true blocking commands well,
            // so we poll with LPOP and sleep briefly when the queue is empty.
            let raw_order: Option<String> = state
                .redis
                .lpop("engine:order_queue", None)
                .await
                .unwrap_or(None);

            let raw_order = match raw_order {
                Some(v) => v,
                None => {
                    // Queue empty — sleep briefly then loop
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            };

            // Deserialize order
            let order: Order = match serde_json::from_str(&raw_order) {
                Ok(o) => o,
                Err(e) => {
                    tracing::error!("Failed to deserialize order: {e}");
                    continue;
                }
            };

            // MATCH — pure in-memory, very fast
            let fills = match_order(order, &mut book);

            // Publish fills + update snapshot
            publish_fills_and_update_snapshot(&state, &book, &fills).await;
        }
    }
}

async fn publish_fills_and_update_snapshot(state: &AppState, book: &OrderBook, fills: &[Fill]) {
    // Publish each fill to the fan-out channel
    for fill in fills {
        if let Ok(json) = serde_json::to_string(fill) {
            if let Err(e) = state.redis.publish::<(), _, _>("fills:events", &json).await {
                tracing::error!("Failed to publish fill: {e}");
            }
        }
    }

    // Update the orderbook snapshot
    let snapshot = book.snapshot(20); // Top 20 levels
    if let Ok(json) = serde_json::to_string(&snapshot) {
        let _: () = state
            .redis
            .set("orderbook:snapshot", &json, None, None, false)
            .await
            .unwrap_or(());
    }
}
