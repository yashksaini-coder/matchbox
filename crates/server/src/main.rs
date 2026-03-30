mod engine_worker;
mod errors;
mod redis_sub;
mod routes;
mod state;

use std::env;

use fred::prelude::*;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let port = env::var("PORT").unwrap_or_else(|_| "8080".into());

    // Connect to Redis
    let config = Config::from_url(&redis_url)?;
    let redis = Builder::from_config(config).build()?;
    redis.init().await?;
    tracing::info!("Connected to Redis at {redis_url}");

    // Broadcast channel for fill events (Redis Pub/Sub → WebSocket clients)
    let (fills_tx, _) = broadcast::channel::<String>(1024);

    let instance_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("Instance ID: {instance_id}");

    let state = AppState {
        redis,
        fills_tx,
        instance_id,
    };

    // Spawn the engine worker (leader election + BRPOP matching loop)
    tokio::spawn(engine_worker::engine_worker_loop(state.clone()));

    // Spawn Redis subscriber (receives fills from Pub/Sub, fans out to WS clients)
    tokio::spawn(redis_sub::redis_subscriber(
        redis_url,
        state.fills_tx.clone(),
    ));

    let app = routes::app_router().with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
