# Live Trading Phase 2 Implementation Plan (WebSocket 틱 + 실시간 P/L)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upbit WebSocket으로 KRW-ETH 틱을 받아 백엔드에서 재방송하고, 프론트가 틱 + 세션 스냅샷으로 미실현 P/L을 실시간 합성해 세션 테이블과 KPI 바에 표시한다.

**Architecture:** 백엔드에 **단일 WebSocket 연결**을 두고 `tokio::sync::broadcast` 채널로 다중 소비자(Tauri Emitter + Axum SSE)에 재방송한다. 브로커는 주기적 Ping으로 keepalive, 연결 실패 시 지수 백오프 재접속. 프론트는 최신 틱을 store에 담고, 세션별 미실현 P/L은 `(tick.price - session.current_buy_price) × current_buy_volume`으로 **파생값으로 계산** — 백엔드는 세션별 실시간 계산을 하지 않는다.

**Tech Stack:** Rust (`tokio-tungstenite`, `futures-util`, `tokio::sync::broadcast`), Axum SSE (already-bundled `axum-extra`), React + Zustand.

**Scope (Phase 2):**
- 의존성: `tokio-tungstenite`, `futures-util` 추가
- 백엔드: `services/tick_broker.rs`, Tauri 이벤트 `market:tick`, Axum `/sse/market`
- 프론트: `TickData` 타입, `liveTradingStore`에 틱 구독 + 파생 P/L 셀렉터, 세션 테이블 갱신, 상단 KPI 바

**Out-of-scope (Phase 3):** 캔들 차트, 시그널 레인 차트, running candle 틱 갱신. Phase 2에서는 "현재가 + 세션 P/L" 텍스트만 실시간 갱신.

---

## 파일 구조 (Phase 2)

**신규:**
- `src-tauri/src/services/tick_broker.rs` — Upbit WS 클라이언트 + broadcast 채널
- `src-tauri/src/server/sse.rs` — Axum SSE 엔드포인트
- `src/components/live/LiveKpiBar.tsx` — 상단 가격/잔고 표시 바

**수정:**
- `src-tauri/Cargo.toml` — 의존성 추가
- `src-tauri/src/services/mod.rs` — `tick_broker` export
- `src-tauri/src/server/mod.rs` — SSE 라우트 merge
- `src-tauri/src/state.rs` — `TickBrokerHandle` 필드 추가
- `src-tauri/src/lib.rs` — 브로커 시작 + AppState에 주입
- `src/types/index.ts` — `TickData` 타입
- `src/lib/live.ts` — 틱 구독 helper
- `src/stores/liveTradingStore.ts` — `ticks` state + 파생 P/L 셀렉터
- `src/components/live/SessionTable.tsx` — 미실현 P/L 컬럼 실시간화
- `src/pages/LiveTradingPage.tsx` — LiveKpiBar 배치

---

## Task 1: 의존성 추가 (tokio-tungstenite, futures-util)

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Cargo.toml에 의존성 추가**

`[dependencies]` 섹션 끝에 추가:

```toml
tokio-tungstenite = { version = "0.23", features = ["native-tls"] }
futures-util = "0.3"
async-stream = "0.3"
```

`async-stream`은 Axum SSE 응답에서 틱 broadcast를 Stream으로 변환할 때 필요.

- [ ] **Step 2: 빌드**

Run: `cargo build --manifest-path src-tauri/Cargo.toml --features tauri-app`
Expected: 의존성 다운로드 + 컴파일 성공.

- [ ] **Step 3: 커밋**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore(live): add tokio-tungstenite + futures-util + async-stream"
```

---

## Task 2: TickData 타입 + parse 유닛 테스트 먼저 작성

**Files:**
- Create: `src-tauri/src/services/tick_broker.rs`
- Modify: `src-tauri/src/services/mod.rs`

`★ Philosophy:` 전체 WS 클라이언트를 한 번에 쓰지 않는다. 먼저 **파싱 로직**만 만들고 단위 테스트로 고정시킨 뒤 WS 루프를 그 위에 쌓는다. 파싱은 네트워크 없이 테스트 가능하고, 형태가 굳으면 그 위의 레이어가 안정적으로 쌓인다.

- [ ] **Step 1: 실패하는 테스트를 먼저 작성**

Create `src-tauri/src/services/tick_broker.rs`:

```rust
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
}
```

- [ ] **Step 2: 모듈 export 추가**

Modify `src-tauri/src/services/mod.rs` — 기존 선언 아래:

```rust
pub mod tick_broker;
```

- [ ] **Step 3: 테스트 실행**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib services::tick_broker`
Expected: 5 tests PASS.

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/src/services/tick_broker.rs src-tauri/src/services/mod.rs
git commit -m "feat(live): TickData + Upbit ticker parser with 5 unit tests"
```

---

## Task 3: WebSocket 클라이언트 + Broadcast 채널

**Files:**
- Modify: `src-tauri/src/services/tick_broker.rs`

`★ Design:` 브로커는 **spawn 시점에 소유권을 양도**하고, 외부에 노출되는 것은 `TickBrokerHandle` — `broadcast::Sender`의 clone + `cancel` flag. 소비자는 `handle.subscribe()`로 `Receiver`를 얻는다. 이 API가 테스트 가능하고 여러 소비자(Tauri / SSE)를 동시에 지원한다.

- [ ] **Step 1: 브로커 구조체 + 헬퍼 시그니처 추가**

`tick_broker.rs` 파일 끝의 `#[cfg(test)]` **앞에** 다음 코드 추가:

```rust
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

/// Start the broker as a background task and return a handle.
/// Broadcasts capacity = 256 ticks; lagging consumers get `RecvError::Lagged`
/// which they should treat as "drop and resubscribe to latest" (not fatal).
pub fn start(markets: Vec<String>) -> TickBrokerHandle {
    let (tx, _) = broadcast::channel::<TickData>(256);
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = TickBrokerHandle {
        sender: tx.clone(),
        cancel: cancel.clone(),
    };

    tokio::spawn(run_loop(markets, tx, cancel));
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
```

**Why SIMPLE format:** Upbit's default "SIMPLE" compact format keeps the same field names we already parse. The format option is forward-compatible with Upbit's newer "DEFAULT" (full) format if we ever need it.

- [ ] **Step 2: broadcast 구독 동작을 확인하는 유닛 테스트 추가**

`#[cfg(test)] mod tests { ... }` 안에 추가 (기존 테스트 뒤):

```rust
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
```

- [ ] **Step 3: 테스트 실행**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib services::tick_broker`
Expected: 7 tests PASS (5 parse + 2 broadcast).

`★ Note on integration:` 실제 `connect_and_stream` 테스트는 네트워크가 필요하므로 유닛 테스트에서 제외. 수동 smoke test(Task 8)에서 검증.

- [ ] **Step 4: 빌드 (feature gate 포함)**

Run: `cargo build --manifest-path src-tauri/Cargo.toml --features tauri-app`
Expected: 성공.

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/src/services/tick_broker.rs
git commit -m "feat(live): tick broker with Upbit WS + broadcast + reconnect"
```

---

## Task 4: AppState에 broker 핸들 주입 + lib.rs에서 시작

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: AppState에 필드 추가**

Modify `src-tauri/src/state.rs`:

```rust
use crate::core::optimizer::Individual;
use crate::services::tick_broker::TickBrokerHandle;
use crate::strategies::StrategyRegistry;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
```

그리고 `AppState` 구조체:

```rust
pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: StrategyRegistry,
    pub auto_trading: Mutex<Option<AutoTradingHandle>>,
    pub optimization: Mutex<Option<OptimizationHandle>>,
    pub paper_session_ids: Mutex<HashMap<i64, ()>>,
    /// Phase 2: Upbit ticker stream broker. `None` only in test contexts
    /// (`AppState::empty()`); real runtime always has `Some`.
    pub tick_broker: Option<TickBrokerHandle>,
}
```

`empty()` 초기화:

```rust
impl AppState {
    pub fn empty() -> Self {
        Self {
            db: Mutex::new(Connection::open_in_memory().expect("in-memory DB")),
            registry: StrategyRegistry::new(),
            auto_trading: Mutex::new(None),
            optimization: Mutex::new(None),
            paper_session_ids: Mutex::new(HashMap::new()),
            tick_broker: None,
        }
    }
}
```

- [ ] **Step 2: lib.rs에서 브로커 시작 + AppState에 주입 + Tauri 이벤트 재방송**

Modify `src-tauri/src/lib.rs` — `mod app` 내부의 `pub fn run()`을 수정.

`let app_state = AppState { ... }` 블록 위에 추가:

```rust
        // Phase 2: start the Upbit ticker broker and share its handle.
        // Tauri Emitter and Axum SSE both subscribe to it.
        let broker = crate::services::tick_broker::start(vec!["KRW-ETH".to_string()]);
```

`AppState { ... }` 두 곳의 초기화에 필드 추가 (app_state + server_state):

```rust
            // app_state
            tick_broker: Some(broker.clone()),
```

```rust
            // server_state (Arc::new 안쪽)
            tick_broker: Some(broker.clone()),
```

그리고 `.setup(move |app| { ... })` 내부 — 기존 live_scheduler 스레드 바로 뒤에, 틱 재방송 태스크 추가:

```rust
                // Phase 2: re-broadcast ticker events to the frontend via Tauri IPC.
                let tick_handle = app.handle().clone();
                let mut tick_rx = broker.subscribe();
                tauri::async_runtime::spawn(async move {
                    use tauri::Emitter;
                    loop {
                        match tick_rx.recv().await {
                            Ok(t) => { let _ = tick_handle.emit("market:tick", &t); }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                // fell behind — next recv will yield the latest
                                continue;
                            }
                            Err(_) => break,  // channel closed
                        }
                    }
                });
```

`★ Where to add:` 기존 live_scheduler 스폰 블록 뒤, `Ok(())` 앞.

- [ ] **Step 3: 빌드**

Run: `cargo build --manifest-path src-tauri/Cargo.toml --features tauri-app`
Expected: 성공.

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/src/state.rs src-tauri/src/lib.rs
git commit -m "feat(live): start tick broker in app + re-emit ticks via Tauri IPC"
```

---

## Task 5: Axum SSE 엔드포인트 (PWA용)

**Files:**
- Create: `src-tauri/src/server/sse.rs`
- Modify: `src-tauri/src/server/mod.rs`

- [ ] **Step 1: SSE 라우트 파일 작성**

Create `src-tauri/src/server/sse.rs`:

```rust
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
```

- [ ] **Step 2: server/mod.rs에 SSE 라우트 merge**

Modify `src-tauri/src/server/mod.rs`:

```rust
pub mod routes;
pub mod ws;
pub mod middleware;
pub mod sse;

use crate::state::AppState;
use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

pub async fn start(state: Arc<AppState>, port: u16) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(routes::api_routes())
        .merge(ws::ws_routes())
        .merge(sse::sse_routes())
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind server address");
    println!("Axum server listening on {addr}");
    axum::serve(listener, app).await.expect("Axum server error");
}
```

- [ ] **Step 3: 빌드**

Run: `cargo build --manifest-path src-tauri/Cargo.toml --features tauri-app`
Expected: 성공.

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/src/server/sse.rs src-tauri/src/server/mod.rs
git commit -m "feat(live): Axum /sse/market endpoint for PWA tick stream"
```

---

## Task 6: 프론트 타입 + 구독 헬퍼

**Files:**
- Modify: `src/types/index.ts`
- Modify: `src/lib/live.ts`

- [ ] **Step 1: `TickData` 타입 추가**

Modify `src/types/index.ts` — 파일 끝에 추가:

```typescript
export interface TickData {
  market: string;
  price: number;
  change_pct: number;
  volume_24h: number;
  ts_ms: number;
}
```

- [ ] **Step 2: 플랫폼별 구독 함수 추가**

Modify `src/lib/live.ts` — 파일 끝에 추가:

```typescript
import type { TickData } from "../types";

/// Subscribe to market ticks. Returns an unsubscribe function.
/// Tauri: uses the "market:tick" event emitted by the backend.
/// PWA: opens an EventSource to `/sse/market`.
export async function subscribeTicks(onTick: (t: TickData) => void): Promise<() => void> {
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen<TickData>("market:tick", (e) => onTick(e.payload));
    return () => { unlisten(); };
  }
  // PWA fallback
  const es = new EventSource("/sse/market");
  es.addEventListener("tick", (ev) => {
    try { onTick(JSON.parse((ev as MessageEvent).data) as TickData); } catch {}
  });
  return () => { es.close(); };
}
```

- [ ] **Step 3: 프론트 빌드**

Run: `npm run vite:build`
Expected: 타입 + Vite 빌드 통과.

- [ ] **Step 4: 커밋**

```bash
git add src/types/index.ts src/lib/live.ts
git commit -m "feat(live): frontend TickData type + subscribeTicks helper"
```

---

## Task 7: Store에 틱 상태 + 파생 P/L 셀렉터

**Files:**
- Modify: `src/stores/liveTradingStore.ts`

- [ ] **Step 1: 틱 상태 + 구독 훅 + 파생 셀렉터 추가**

Modify `src/stores/liveTradingStore.ts`:

```typescript
import { create } from "zustand";
import type { LiveSession, LiveTrade, Preset, TickData } from "../types";
import {
  listPresets,
  listSessions,
  listSessionTrades,
  createSession as apiCreateSession,
  startSession as apiStart,
  stopSession as apiStop,
  deleteSession as apiDelete,
  deletePreset as apiDeletePreset,
  subscribeTicks,
} from "../lib/live";

interface LiveTradingState {
  sessions: LiveSession[];
  presets: Preset[];
  tradesBySession: Record<number, LiveTrade[]>;
  /// Market-keyed most-recent tick snapshot. Updated by the tick subscription.
  ticks: Record<string, TickData>;
  loading: boolean;

  refreshAll: () => Promise<void>;
  refreshSessions: () => Promise<void>;
  refreshPresets: () => Promise<void>;
  refreshTrades: (sessionId: number) => Promise<void>;

  createSession: (args: Parameters<typeof apiCreateSession>[0]) => Promise<void>;
  startSession: (id: number) => Promise<void>;
  stopSession: (id: number) => Promise<void>;
  deleteSession: (id: number) => Promise<void>;
  deletePreset: (id: number) => Promise<void>;

  subscribeEvents: () => Promise<() => void>;
}

export const useLiveTradingStore = create<LiveTradingState>((set, get) => ({
  sessions: [],
  presets: [],
  tradesBySession: {},
  ticks: {},
  loading: false,

  refreshAll: async () => {
    set({ loading: true });
    try {
      await Promise.all([get().refreshSessions(), get().refreshPresets()]);
    } finally {
      set({ loading: false });
    }
  },

  refreshSessions: async () => {
    const sessions = await listSessions();
    set({ sessions });
  },

  refreshPresets: async () => {
    const presets = await listPresets();
    set({ presets });
  },

  refreshTrades: async (sessionId) => {
    const trades = await listSessionTrades(sessionId);
    set((s) => ({ tradesBySession: { ...s.tradesBySession, [sessionId]: trades } }));
  },

  createSession: async (args) => {
    await apiCreateSession(args);
    await get().refreshSessions();
  },

  startSession: async (id) => {
    await apiStart(id);
    await get().refreshSessions();
  },

  stopSession: async (id) => {
    await apiStop(id);
    await get().refreshSessions();
  },

  deleteSession: async (id) => {
    await apiDelete(id);
    await get().refreshSessions();
  },

  deletePreset: async (id) => {
    await apiDeletePreset(id);
    await get().refreshPresets();
  },

  subscribeEvents: async () => {
    const unsubs: Array<() => void> = [];

    // Tauri session events (same as before)
    if ("__TAURI_INTERNALS__" in window) {
      const { listen } = await import("@tauri-apps/api/event");
      const u1 = await listen("session:update", () => { get().refreshSessions(); });
      const u2 = await listen<{ session_id: number }>("session:log", (e) => {
        console.debug("session:log", e.payload);
      });
      unsubs.push(u1 as () => void, u2 as () => void);
    }

    // Market ticks (Tauri event OR SSE fallback)
    const unsubTicks = await subscribeTicks((tick) => {
      set((s) => ({ ticks: { ...s.ticks, [tick.market]: tick } }));
    });
    unsubs.push(unsubTicks);

    return () => { unsubs.forEach((fn) => fn()); };
  },
}));

/// Derive current equity and unrealized P/L for a session using the latest tick.
/// Falls back to the DB-persisted `current_equity` if no tick is available yet.
export function deriveSessionPnl(session: LiveSession, tick: TickData | undefined) {
  const baseEquity = session.current_equity ?? session.initial_capital;
  if (session.current_position !== "holding" || tick == null
      || session.current_buy_price == null || session.current_buy_volume == null) {
    return {
      currentEquity: baseEquity,
      unrealizedPnl: 0,
      unrealizedPnlPct: 0,
      pnlPctSinceStart: baseEquity / session.initial_capital * 100 - 100,
    };
  }
  const unrealized = (tick.price - session.current_buy_price) * session.current_buy_volume;
  // DB equity snapshot is taken at last cycle's candle close. For real-time
  // display we overlay the gap between that close and the current tick price.
  // Since DB equity already includes realized P/L + last candle's mark, we
  // recompute the holding leg from the buy anchor to tick.
  const realizedPortion = baseEquity - (session.current_buy_price * session.current_buy_volume);
  const currentEquity = realizedPortion + tick.price * session.current_buy_volume;
  return {
    currentEquity,
    unrealizedPnl: unrealized,
    unrealizedPnlPct: (tick.price / session.current_buy_price - 1) * 100,
    pnlPctSinceStart: currentEquity / session.initial_capital * 100 - 100,
  };
}
```

`★ Why two P/L concepts:` "unrealized P/L" = 현재 보유분의 매수가 대비 수익, "pnlPctSinceStart" = 세션 시작 대비 총 수익률 (실현 + 미실현). UI는 둘 다 의미 있음.

- [ ] **Step 2: 프론트 빌드**

Run: `npm run vite:build`
Expected: 통과.

- [ ] **Step 3: 커밋**

```bash
git add src/stores/liveTradingStore.ts
git commit -m "feat(live): ticks state + deriveSessionPnl selector in store"
```

---

## Task 8: SessionTable에 실시간 P/L 반영 + LiveKpiBar

**Files:**
- Modify: `src/components/live/SessionTable.tsx`
- Create: `src/components/live/LiveKpiBar.tsx`
- Modify: `src/pages/LiveTradingPage.tsx`

- [ ] **Step 1: SessionTable이 ticks를 사용하도록 수정**

Modify `src/components/live/SessionTable.tsx` — 전체 교체:

```tsx
import { Button } from "../ui/Button";
import { Badge } from "../ui/Badge";
import type { LiveSession, TickData } from "../../types";
import { deriveSessionPnl } from "../../stores/liveTradingStore";

interface Props {
  sessions: LiveSession[];
  ticks: Record<string, TickData>;
  onStart: (id: number) => void;
  onStop: (id: number) => void;
  onDelete: (id: number) => void;
}

export default function SessionTable({ sessions, ticks, onStart, onStop, onDelete }: Props) {
  if (sessions.length === 0) {
    return <p className="text-zinc-500 text-sm">No sessions yet. Create one to start.</p>;
  }
  return (
    <table className="w-full text-sm">
      <thead className="text-zinc-400 text-xs">
        <tr className="border-b border-zinc-800">
          <th className="text-left py-2">Label</th>
          <th className="text-left">Market</th>
          <th className="text-left">Status</th>
          <th className="text-left">Position</th>
          <th className="text-right">Equity</th>
          <th className="text-right">Total P/L</th>
          <th className="text-right">Unrealized</th>
          <th className="text-left">Signal</th>
          <th className="text-right">Last Cycle</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {sessions.map((s) => {
          const tick = ticks[s.market];
          const derived = deriveSessionPnl(s, tick);
          const totalColor = derived.pnlPctSinceStart > 0 ? "text-emerald-400"
            : derived.pnlPctSinceStart < 0 ? "text-rose-400" : "text-zinc-400";
          const unrColor = derived.unrealizedPnlPct > 0 ? "text-emerald-400"
            : derived.unrealizedPnlPct < 0 ? "text-rose-400" : "text-zinc-500";
          return (
            <tr key={s.id} className="border-b border-zinc-900 hover:bg-zinc-900/40">
              <td className="py-2 font-medium text-zinc-200">{s.label}</td>
              <td className="text-zinc-400">{s.market}</td>
              <td>
                <Badge variant={s.status === "running" ? "green" : "default"}>
                  {s.status}
                </Badge>
              </td>
              <td>
                <Badge variant={s.current_position === "holding" ? "amber" : "default"}>
                  {s.current_position}
                  {s.current_position === "holding" && s.current_buy_price != null && (
                    <span className="ml-1 text-[10px] text-zinc-400 font-data">
                      @ {s.current_buy_price.toLocaleString()}
                    </span>
                  )}
                </Badge>
              </td>
              <td className="text-right font-data text-zinc-200">
                {Math.round(derived.currentEquity).toLocaleString()}
              </td>
              <td className={`text-right font-data ${totalColor}`}>
                {derived.pnlPctSinceStart.toFixed(2)}%
              </td>
              <td className={`text-right font-data ${unrColor}`}>
                {s.current_position === "holding"
                  ? `${derived.unrealizedPnlPct.toFixed(2)}%`
                  : "--"}
              </td>
              <td className="text-zinc-400">{s.last_signal ?? "--"}</td>
              <td className="text-right text-zinc-500 text-xs">
                {s.last_cycle_ts?.slice(11, 16) ?? "--"}
              </td>
              <td className="text-right">
                <div className="flex gap-1 justify-end">
                  {s.status === "stopped" ? (
                    <Button size="sm" variant="success" onClick={() => onStart(s.id)}>Start</Button>
                  ) : (
                    <Button size="sm" variant="secondary" onClick={() => onStop(s.id)}>Stop</Button>
                  )}
                  <Button size="sm" variant="danger" onClick={() => onDelete(s.id)}>Del</Button>
                </div>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
```

- [ ] **Step 2: LiveKpiBar 컴포넌트 작성**

Create `src/components/live/LiveKpiBar.tsx`:

```tsx
import { Card, CardContent } from "../ui/Card";
import type { TickData } from "../../types";

interface Props {
  tick: TickData | undefined;
  market?: string;
}

export default function LiveKpiBar({ tick, market = "KRW-ETH" }: Props) {
  const price = tick?.price;
  const change = tick?.change_pct ?? 0;
  const changeColor = change > 0 ? "bg-emerald-500/15 text-emerald-400"
    : change < 0 ? "bg-rose-500/15 text-rose-400" : "bg-zinc-700/30 text-zinc-400";
  const lagMs = tick ? Date.now() - tick.ts_ms : Infinity;
  const stale = lagMs > 10_000;

  return (
    <Card>
      <CardContent className="flex items-center justify-between">
        <div>
          <p className="text-xs font-medium text-zinc-500 mb-1">{market}</p>
          <p className="text-[32px] font-bold text-zinc-100 font-data leading-tight">
            {price != null ? price.toLocaleString() : "--"}
            <span className="text-sm ml-2 text-zinc-600 font-sans font-normal">KRW</span>
          </p>
        </div>
        <div className="flex items-center gap-2">
          {stale && price != null && (
            <span className="text-[10px] text-zinc-600">stale</span>
          )}
          {price != null && (
            <span className={`inline-flex items-center px-3 py-1.5 rounded-lg text-sm font-semibold font-data ${changeColor}`}>
              {change >= 0 ? "+" : ""}{change.toFixed(2)}%
            </span>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 3: LiveTradingPage 통합**

Modify `src/pages/LiveTradingPage.tsx`:

```tsx
import { useEffect, useState } from "react";
import { Button } from "../components/ui/Button";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Plus, Trash2 } from "lucide-react";
import SessionTable from "../components/live/SessionTable";
import NewSessionDialog from "../components/live/NewSessionDialog";
import LiveKpiBar from "../components/live/LiveKpiBar";
import { useLiveTradingStore } from "../stores/liveTradingStore";

export default function LiveTradingPage() {
  const {
    sessions, presets, ticks,
    refreshAll, createSession,
    startSession, stopSession, deleteSession, deletePreset,
    subscribeEvents,
  } = useLiveTradingStore();
  const [showNew, setShowNew] = useState(false);

  useEffect(() => {
    refreshAll();
    let unlisten: (() => void) | null = null;
    subscribeEvents().then((fn) => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
  }, []);

  return (
    <div className="space-y-4 animate-fade-in">
      <LiveKpiBar tick={ticks["KRW-ETH"]} />

      <Card>
        <CardHeader>
          <h3 className="text-sm font-semibold text-zinc-300">
            Presets ({presets.length})
          </h3>
        </CardHeader>
        <CardContent>
          {presets.length === 0 ? (
            <p className="text-xs text-zinc-500">
              프리셋이 없습니다. <span className="text-zinc-300 font-medium">Simulation 페이지</span>에서
              파라미터를 조정한 뒤 "Save as preset"으로 저장하세요.
            </p>
          ) : (
            <table className="w-full text-xs">
              <thead className="text-zinc-500">
                <tr className="border-b border-zinc-800">
                  <th className="text-left py-1.5">Name</th>
                  <th className="text-left">Strategy</th>
                  <th className="text-left">Market</th>
                  <th className="text-left">Timeframe</th>
                  <th className="text-left">Window</th>
                  <th className="text-left">Source</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {presets.map((p) => (
                  <tr key={p.id} className="border-b border-zinc-900 hover:bg-zinc-900/40">
                    <td className="py-1.5 text-zinc-200 font-medium">{p.name}</td>
                    <td className="text-zinc-400">{p.strategy_key}</td>
                    <td className="text-zinc-400">{p.market ?? "--"}</td>
                    <td className="text-zinc-400">{p.timeframe ?? "--"}</td>
                    <td className="text-zinc-500">
                      {p.since_ts && p.until_ts ? `${p.since_ts} ~ ${p.until_ts}` : "--"}
                    </td>
                    <td className="text-zinc-500">{p.source}</td>
                    <td className="text-right">
                      <Button
                        size="sm"
                        variant="danger"
                        onClick={() => {
                          if (window.confirm(`Delete preset "${p.name}"?`)) {
                            deletePreset(p.id);
                          }
                        }}
                      >
                        <Trash2 size={12} />
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-zinc-300">Live Trading — Paper Sessions</h3>
          <Button size="sm" onClick={() => setShowNew(true)} disabled={presets.length === 0}>
            <Plus size={14} /> New Session
          </Button>
        </CardHeader>
        <CardContent>
          <SessionTable
            sessions={sessions}
            ticks={ticks}
            onStart={startSession}
            onStop={stopSession}
            onDelete={(id) => {
              if (window.confirm("Delete this session? All trades and equity history will be removed.")) {
                deleteSession(id);
              }
            }}
          />
        </CardContent>
      </Card>

      {showNew && (
        <NewSessionDialog
          presets={presets}
          onClose={() => setShowNew(false)}
          onSubmit={async (args) => {
            await createSession(args);
            setShowNew(false);
          }}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 4: 프론트 빌드**

Run: `npm run vite:build`
Expected: 통과.

- [ ] **Step 5: 커밋**

```bash
git add src/components/live/SessionTable.tsx src/components/live/LiveKpiBar.tsx src/pages/LiveTradingPage.tsx
git commit -m "feat(live): real-time P/L in SessionTable + LiveKpiBar"
```

---

## Task 9: 수동 smoke test (실제 WS 동작 확인)

**Files:** (변경 없음 — 수동 검증)

- [ ] **Step 1: 전체 회귀 테스트**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`
Expected: Phase 1 테스트 + Phase 2 신규 7개 모두 PASS.

- [ ] **Step 2: 앱 실행 안내**

사용자에게 다음을 요청 (프로젝트 규칙: Claude는 앱을 직접 시작하지 않음):

```bash
npm run dev
```

- [ ] **Step 3: UI 동작 검증 체크리스트**

Live 페이지에서 다음 확인:
- [ ] 상단 KPI 바에 ETH 가격이 실시간으로 변함 (초 단위)
- [ ] 가격 변화률 %의 부호·색상이 맞음 (+ 녹색, − 적색)
- [ ] Running 세션 중 holding 포지션이 있다면, "Unrealized" 컬럼이 tick마다 움직임
- [ ] 모든 Stopped 세션의 "Unrealized"는 `--`로 표시
- [ ] 개발자 콘솔에 `market:tick` 로그가 주기적으로 찍힘 (브라우저 DevTools)
- [ ] 앱 시작 후 30초 내 첫 tick 도착 (WS 연결 지연 + 첫 이벤트)
- [ ] 네트워크 연결을 잠깐 끊었다가 복구해도, 60초 이내에 자동 재연결 후 tick 재개

- [ ] **Step 4: 문제가 없으면 종료. 버그 발견 시 해당 커밋 수정.**

---

## Definition of Done (Phase 2)

- [ ] `cargo build --features tauri-app` 성공
- [ ] `cargo test --no-default-features` 전체 통과 (Phase 1 + 신규 7개)
- [ ] `npm run vite:build` 통과
- [ ] Smoke test (Task 9) 체크리스트 전부 OK
- [ ] 앱 실행 중 Upbit WS가 연결되고 브라우저 UI에 가격 + 세션 P/L이 실시간 반영됨

## Phase 3 Preview (본 플랜 범위 밖)

다음 플랜 `2026-04-??-live-trading-phase3.md`에서:
- `lightweight-charts` 의존 추가
- `CandleChart` + `SignalLaneChart` + `useChartSync`
- 세션별 마커/색 팔레트
- Running candle 틱 갱신 (Phase 2의 tick 스트림 소비)
- 로그 패널 세션 필터
