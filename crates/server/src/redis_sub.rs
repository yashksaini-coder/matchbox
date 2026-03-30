use fred::prelude::*;
use tokio::sync::broadcast;

/// Subscribes to Redis fills:events channel and fans out to local WS clients.
/// One instance of this task runs per API server instance.
pub async fn redis_subscriber(redis_url: String, fill_tx: broadcast::Sender<String>) {
    // PubSub requires a dedicated connection — it cannot share
    // a connection pool with regular commands.
    let config = match Config::from_url(&redis_url) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Invalid Redis URL for subscriber: {e}");
            return;
        }
    };

    let subscriber = match Builder::from_config(config).build_subscriber_client() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to build subscriber client: {e}");
            return;
        }
    };

    if let Err(e) = subscriber.init().await {
        tracing::error!("Failed to init subscriber client: {e}");
        return;
    }

    // Get the message stream BEFORE subscribing
    let mut message_rx = subscriber.message_rx();

    if let Err(e) = subscriber.subscribe("fills:events").await {
        tracing::error!("Failed to subscribe to fills:events: {e}");
        return;
    }

    tracing::info!("Redis subscriber listening on fills:events");

    while let Ok(message) = message_rx.recv().await {
        if let Ok(payload) = message.value.convert::<String>() {
            // Send to all local WebSocket clients via broadcast.
            // Ignore errors — no receivers connected is OK.
            let _ = fill_tx.send(payload);
        }
    }
}
