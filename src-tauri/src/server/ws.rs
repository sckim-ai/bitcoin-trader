use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use crate::state::AppState;
use std::sync::Arc;

pub fn ws_routes() -> Router<Arc<AppState>> {
    Router::new().route("/ws/live", get(ws_handler))
}

async fn ws_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    // Stub: echo messages back. Real-time price streaming will be connected
    // when the monitoring loop is implemented.
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                let send_result = socket.send(Message::Text(text)).await;
                if send_result.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
