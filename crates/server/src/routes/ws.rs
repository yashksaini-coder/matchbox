use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;

use crate::state::AppState;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    // Subscribe BEFORE upgrade to avoid missing any fills
    let rx = state.fills_tx.subscribe();
    ws.on_upgrade(|socket| handle_socket(socket, rx))
}

async fn handle_socket(socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    let (mut sender, mut receiver) = socket.split();

    // Task 1: Forward fills from broadcast channel to WebSocket client
    let mut send_task = tokio::spawn(async move {
        while let Ok(fill_json) = rx.recv().await {
            if sender.send(Message::Text(fill_json.into())).await.is_err() {
                // Client disconnected
                break;
            }
        }
    });

    // Task 2: Read from WebSocket (just drain; detects disconnection)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Close(_) = msg {
                break;
            }
        }
    });

    // When either task finishes, abort the other
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}
