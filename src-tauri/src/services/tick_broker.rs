use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TickData {
    pub market: String,
    pub price: f64,
    pub change_pct: f64,   // e.g., +1.24 => 1.24
    pub volume_24h: f64,
    pub ts_ms: i64,
}

/// Parse an Upbit ticker WebSocket message into TickData.
/// Returns None if the payload is not a ticker event or is malformed.
pub fn parse_ticker(bytes: &[u8]) -> Option<TickData> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    // Upbit ticker identifies itself with `"type":"ticker"` + market code in `code`.
    if v.get("type").and_then(|t| t.as_str()) != Some("ticker") {
        return None;
    }
    let market = v.get("code")?.as_str()?.to_string();
    let price = v.get("trade_price")?.as_f64()?;
    // Upbit: `signed_change_rate` is fractional (0.0124 == +1.24%).
    let change_rate = v.get("signed_change_rate").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let volume_24h = v.get("acc_trade_volume_24h").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let ts_ms = v.get("timestamp").and_then(|x| x.as_i64()).unwrap_or(0);

    Some(TickData {
        market,
        price,
        change_pct: change_rate * 100.0,
        volume_24h,
        ts_ms,
    })
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Handle passed around the app to subscribe to the broker's tick stream
/// and to request shutdown.
#[derive(Clone)]
pub struct TickBrokerHandle {
    sender: broadcast::Sender<TickData>,
    cancel: Arc<AtomicBool>,
}

impl TickBrokerHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<TickData> {
        self.sender.subscribe()
    }
    pub fn shutdown(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Upbit WS endpoint.
const UPBIT_WS_URL: &str = "wss://api.upbit.com/websocket/v1";
/// Keepalive ping interval — Upbit disconnects after ~3 minutes of silence.
const PING_INTERVAL: Duration = Duration::from_secs(120);

/// Start the broker on its own dedicated Tokio runtime thread and return a
/// handle. Having a self-owned runtime lets `start()` be called from any sync
/// context (e.g. app bootstrap before Tauri's runtime is available).
/// Broadcasts capacity = 256 ticks; lagging consumers get `RecvError::Lagged`
/// which they should treat as "drop and resubscribe to latest" (not fatal).
pub fn start(markets: Vec<String>) -> TickBrokerHandle {
    let (tx, _) = broadcast::channel::<TickData>(256);
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = TickBrokerHandle {
        sender: tx.clone(),
        cancel: cancel.clone(),
    };

    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .expect("tick-broker tokio runtime")
            .block_on(run_loop(markets, tx, cancel));
    });
    handle
}

async fn run_loop(markets: Vec<String>, tx: broadcast::Sender<TickData>, cancel: Arc<AtomicBool>) {
    let mut backoff_secs: u64 = 5;
    loop {
        if cancel.load(Ordering::Relaxed) {
            eprintln!("[tick-broker] cancelled");
            break;
        }
        match connect_and_stream(&markets, &tx, &cancel).await {
            Ok(_) => { backoff_secs = 5; }  // clean exit (cancelled)
            Err(e) => {
                eprintln!("[tick-broker] error: {e}; reconnect in {backoff_secs}s");
                for _ in 0..backoff_secs {
                    if cancel.load(Ordering::Relaxed) { return; }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }
    }
}

async fn connect_and_stream(
    markets: &[String],
    tx: &broadcast::Sender<TickData>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (ws_stream, _) = tokio_tungstenite::connect_async(UPBIT_WS_URL).await?;
    eprintln!("[tick-broker] connected to Upbit WS");
    let (mut write, mut read) = ws_stream.split();

    // Subscribe payload. Upbit expects an array of objects.
    let codes: Vec<&str> = markets.iter().map(|s| s.as_str()).collect();
    let sub = serde_json::json!([
        {"ticket": "bt-live"},
        {"type": "ticker", "codes": codes},
        {"format": "SIMPLE"}
    ]);
    write.send(Message::Text(sub.to_string())).await?;

    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    ping_interval.tick().await;  // consume immediate tick

    loop {
        if cancel.load(Ordering::Relaxed) { return Ok(()); }
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        if let Some(tick) = parse_ticker(&bytes) {
                            let _ = tx.send(tick);
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Some(tick) = parse_ticker(text.as_bytes()) {
                            let _ = tx.send(tick);
                        }
                    }
                    Some(Ok(Message::Ping(data))) => { write.send(Message::Pong(data)).await?; }
                    Some(Ok(Message::Close(_))) | None => {
                        return Err("ws closed by peer".into());
                    }
                    Some(Err(e)) => return Err(e.into()),
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                write.send(Message::Ping(vec![])).await?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_eth_ticker_event() {
        let msg = br#"{
            "type": "ticker",
            "code": "KRW-ETH",
            "trade_price": 3245000.0,
            "signed_change_rate": 0.0124,
            "acc_trade_volume_24h": 15234.5,
            "timestamp": 1714000000000
        }"#;
        let t = parse_ticker(msg).expect("should parse");
        assert_eq!(t.market, "KRW-ETH");
        assert!((t.price - 3_245_000.0).abs() < 0.01);
        assert!((t.change_pct - 1.24).abs() < 1e-6);
        assert!((t.volume_24h - 15234.5).abs() < 0.01);
        assert_eq!(t.ts_ms, 1_714_000_000_000);
    }

    #[test]
    fn rejects_non_ticker_event() {
        // Upbit also emits `"type":"trade"` and others.
        let msg = br#"{"type":"trade","code":"KRW-ETH"}"#;
        assert!(parse_ticker(msg).is_none());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_ticker(b"not json").is_none());
    }

    #[test]
    fn handles_negative_change_rate() {
        let msg = br#"{
            "type": "ticker",
            "code": "KRW-ETH",
            "trade_price": 3000000.0,
            "signed_change_rate": -0.02,
            "acc_trade_volume_24h": 5000.0,
            "timestamp": 1714000000000
        }"#;
        let t = parse_ticker(msg).expect("should parse");
        assert!((t.change_pct - (-2.0)).abs() < 1e-6);
    }

    #[test]
    fn missing_optional_fields_default_to_zero() {
        let msg = br#"{
            "type": "ticker",
            "code": "KRW-ETH",
            "trade_price": 3000000.0
        }"#;
        let t = parse_ticker(msg).expect("should parse");
        assert_eq!(t.change_pct, 0.0);
        assert_eq!(t.volume_24h, 0.0);
        assert_eq!(t.ts_ms, 0);
    }

    #[tokio::test]
    async fn broadcast_handle_delivers_ticks_to_multiple_subscribers() {
        let (tx, _) = broadcast::channel::<TickData>(8);
        let handle = TickBrokerHandle {
            sender: tx.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
        };

        let mut r1 = handle.subscribe();
        let mut r2 = handle.subscribe();

        tx.send(TickData {
            market: "KRW-ETH".into(), price: 100.0,
            change_pct: 0.5, volume_24h: 1.0, ts_ms: 1,
        }).unwrap();

        let t1 = r1.recv().await.unwrap();
        let t2 = r2.recv().await.unwrap();
        assert_eq!(t1, t2);
        assert!((t1.price - 100.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn shutdown_sets_cancel_flag() {
        let (tx, _) = broadcast::channel::<TickData>(8);
        let cancel = Arc::new(AtomicBool::new(false));
        let handle = TickBrokerHandle { sender: tx, cancel: cancel.clone() };
        assert!(!cancel.load(Ordering::Relaxed));
        handle.shutdown();
        assert!(cancel.load(Ordering::Relaxed));
    }
}
