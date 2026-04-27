use crate::state::AppState;
use async_stream::stream;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures_util::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

pub fn sse_routes() -> Router<Arc<AppState>> {
    Router::new().route("/sse/market", get(market_stream))
}

async fn market_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let broker = state.tick_broker.clone();
    let s = stream! {
        let Some(broker) = broker else {
            // AppState::empty (tests). Never reached in runtime.
            yield Ok(Event::default().event("error").data("broker unavailable"));
            return;
        };
        let mut rx = broker.subscribe();
        loop {
            match rx.recv().await {
                Ok(tick) => {
                    let payload = serde_json::to_string(&tick).unwrap_or_default();
                    yield Ok(Event::default().event("tick").data(payload));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };
    Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}
