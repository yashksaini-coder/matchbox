use fred::prelude::*;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub redis: Client,
    pub fills_tx: broadcast::Sender<String>,
    pub instance_id: String,
}
