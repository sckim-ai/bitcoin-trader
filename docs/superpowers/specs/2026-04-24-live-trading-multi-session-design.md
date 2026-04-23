# Live Trading: Multi-Session Paper + Real Auto-Trading

**Date**: 2026-04-24
**Status**: Design (awaiting implementation plan)
**Scope**: LiveTradingPage rework — 실시간 차트, 다수 전략 페이퍼 비교, 단일 실전 승격 자동매매
**Trading target**: `KRW-ETH` (BTC는 참고 데이터만)

---

## 1. 목적과 동기

현재 `LiveTradingPage`는 단일 시장·단일 전략 자동매매만 지원하고, 차트가 없으며, 전략 파라미터 커스터마이징 경로도 없다. 사용자 요구는 세 가지:

1. **실시간 차트** — 캔들 + 시그널 시각화
2. **다수 전략 동시 비교** — 전략·프리셋 조합별로 실시간 성과 추적
3. **선택한 설정으로 오토 트레이딩** — 페이퍼에서 검증 후 실전 승격

이 설계는 위 요구를 만족하면서 기존 백테스트 엔진(`Strategy::run_simulation`)을 재사용해 **백테스트 = 페이퍼 = 실전 시그널 일치**를 보장한다.

## 2. 핵심 결정사항

| 항목 | 결정 | 근거 |
|------|------|------|
| 멀티 실행 의미 | 실전 1개 + 페이퍼 N개 | Upbit 단일 계좌 제약, 사용자 멘탈 모델 일치 |
| 전략 설정 | 프리셋 선택 기반 | Optimization 결과(NSGA-II 파레토 프런트) 재활용 경로 |
| 차트 구성 | 2-패널: 캔들 + 시그널 레인 | 5종 시그널 × N세션을 한 차트에 표시하면 노이즈 과다 |
| 차트 라이브러리 | `lightweight-charts` (TradingView) | 멀티 페인 시간축 동기화 표준 지원, 캔버스 성능 |
| 영속성 | DB 저장 + 재시작 후 수동 재개 | 갭 자동 보정의 애매함 회피, 사용자 의도 명시성 |
| 실시간 소스 | Upbit WebSocket (백엔드 단일 연결) | 페이퍼 세션이 많아도 연결 1개, 서버가 틱을 재방송 |
| 체결 모델 | 기존 `Strategy::run_simulation` 재사용 | 백테스트/페이퍼/실전 로직 일원화 |
| 실전 승격 규칙 | 시장당 실전 최대 1, 확인 다이얼로그 1회 | 과도한 가드는 첫 버전에 부담, 킬스위치는 기존 stop 커맨드로 대체 |
| **안전 규칙** | `real_started_at` 이후 새 시그널에만 실주문 | 승격 시점의 과거 `holding` 상태가 실주문을 트리거하지 않도록 보호 |
| 거래 시장 | `KRW-ETH` 고정 | 프로젝트 현재 타겟 |

## 3. 아키텍처

### 3.1 전체 구조

```
┌──────────── Frontend (React) ────────────┐
│  LiveTradingPage                          │
│  ├─ LiveKpiBar      (가격/잔고 + 실전표시)  │
│  ├─ SessionList     (좌측: 세션 카드 N개)   │
│  ├─ CandleChart     (우측 상: 캔들 + 마커)  │
│  ├─ SignalLaneChart (우측 하: 시그널 레인)   │
│  └─ LiveLogPanel    (하단: 로그 + 수동매매)  │
│                                           │
│  Zustand: liveTradingStore                │
│   - sessions[], ticks{}, logs[]           │
│   - 이벤트 구독: session:*, market:tick    │
└──────────────────┬───────────────────────┘
                   │ Tauri IPC / HTTP(+SSE)
┌──────────────────▼───────────────────────┐
│           Backend (Rust)                  │
│                                           │
│  commands/live_trading.rs (신규)          │
│   - 세션/프리셋 CRUD, 승격·강등            │
│                                           │
│  services/                                │
│   - session_engine.rs  (신규)             │
│       풀 리플레이 기반 사이클 실행           │
│   - tick_broker.rs     (신규)             │
│       Upbit WS 단일 연결 → 재방송          │
│   - scheduler.rs       (신규, 얇은 층)     │
│       1시간 경계에 모든 running 세션 처리   │
│   - auto_trader.rs     (축소/재배선)       │
│       순수 로직은 session_engine이 재사용   │
│                                           │
│  AppState (수정)                          │
│   - paper_sessions: HashMap<Id, Handle>   │
│   - real_session:   Option<SessionId>     │
│   - tick_broker:    Arc<TickBroker>       │
└──────────────────────────────────────────┘
```

### 3.2 두 개의 독립 루프

| 루프 | 주기 | 역할 |
|------|------|------|
| 세션 엔진 | 1시간 경계 | 모든 `running` 세션에 대해 `run_simulation(data[start..now])` → trade diff 저장 → 이벤트 발행 → (실전이면) Upbit 주문 |
| 틱 브로커 | 실시간 | Upbit WS → `market:tick` 이벤트 재방송 (백엔드는 세션 계산을 하지 않음, 프론트가 틱 + 세션 스냅샷으로 미실현 P/L 합성) |

두 루프는 서로를 모른다. 프론트가 둘의 결과를 합성해 UI를 그린다.

## 4. 데이터 모델

### 4.1 신규 테이블 (마이그레이션)

```sql
-- 전략 파라미터 프리셋 (Optimization 결과 또는 수동)
CREATE TABLE presets (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id       INTEGER NOT NULL DEFAULT 1,
  name          TEXT NOT NULL,
  strategy_key  TEXT NOT NULL,           -- 'V3', 'V3.1', 'V5'
  params_json   TEXT NOT NULL,
  source        TEXT NOT NULL,           -- 'manual' | 'optimization'
  source_run_id INTEGER,
  created_at    TEXT NOT NULL,
  UNIQUE(user_id, name)
);

-- 라이브 세션 (페이퍼 + 실전 공용)
CREATE TABLE live_sessions (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id          INTEGER NOT NULL DEFAULT 1,
  label            TEXT NOT NULL,
  preset_id        INTEGER NOT NULL REFERENCES presets(id),
  market           TEXT NOT NULL,        -- 'KRW-ETH'
  mode             TEXT NOT NULL,        -- 'paper' | 'real'
  status           TEXT NOT NULL,        -- 'running' | 'stopped'
  initial_capital  REAL NOT NULL,
  start_ts         TEXT NOT NULL,        -- UTC
  real_started_at  TEXT,                 -- NULL = 실전 경험 없음
  last_cycle_ts    TEXT,
  last_signal      TEXT,
  created_at       TEXT NOT NULL
);
CREATE INDEX idx_live_sessions_status ON live_sessions(status);

-- 세션 트레이드 이력 (페이퍼/실전 공용)
CREATE TABLE live_trades (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id   INTEGER NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  ts           TEXT NOT NULL,
  side         TEXT NOT NULL,            -- 'buy' | 'sell'
  price        REAL NOT NULL,
  volume       REAL NOT NULL,
  fee          REAL NOT NULL,
  signal       TEXT NOT NULL,
  pnl          REAL,
  pnl_pct      REAL,
  is_real      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_live_trades_session ON live_trades(session_id, ts);

-- 세션 에쿼티 스냅샷 (차트 P/L 곡선용)
CREATE TABLE live_equity (
  session_id  INTEGER NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  ts          TEXT NOT NULL,
  equity      REAL NOT NULL,
  position    TEXT NOT NULL,              -- 'idle' | 'holding'
  PRIMARY KEY (session_id, ts)
);
```

### 4.2 기존 테이블과의 관계

- `positions`, `trades`: 기존 단일 실전용. Phase 4에서 제거 또는 `live_trades`에 마이그레이션 (최종 판단은 구현 시점에).
- `optimization_runs` (있다면): `presets.source_run_id`가 옵션으로 참조.

## 5. 백엔드 상세

### 5.1 `services/session_engine.rs`

```rust
pub struct SessionCycleOutput {
    pub new_trades: Vec<LiveTrade>,
    pub final_equity: f64,
    pub final_position: PositionState,
    pub latest_signal: String,
    pub latest_signal_bar_ts: DateTime<Utc>,
}

pub async fn run_session_cycle(
    db: &Arc<Mutex<Connection>>,
    client: &UpbitClient,
    session: &LiveSession,
    preset: &Preset,
    registry: &StrategyRegistry,
) -> Result<SessionCycleOutput, Error>
```

**알고리즘:**
1. `session.start_ts` ~ 최신까지 1시간 봉 확보 (DB 우선, 부족분만 Upbit에서)
2. `strategy.run_simulation(&data, &params)` 실행
3. DB의 `live_trades`에서 `session_id`별 최근 `ts` 조회
4. `result.trades` 중 해당 `ts`보다 큰 것만 신규로 판정해 `live_trades`에 삽입
5. `live_equity`에 최종 스냅샷 upsert, `live_sessions.last_cycle_ts` / `last_signal` 갱신
6. 출력 구조체 반환

**멱등성:** 동일 입력에서 `run_session_cycle`을 여러 번 호출해도 DB 결과 동일.

### 5.2 실전 주문 게이트

세션 엔진 결과를 받은 scheduler 층에서 **두 겹의 게이트**를 통과해야 실주문:

**게이트 1 — 타임스탬프**: 시그널이 찍힌 봉의 `ts`가 `real_started_at` 이후일 것. 승격 이전의 과거 시그널이 초기 조건으로서만 의미를 갖게 함.

**게이트 2 — 실전 라이프사이클 포지션**: 세션의 `is_real=1` trade 이력으로 파생되는 "실전 포지션" 상태. 마지막 실전 trade가 없거나 `sell`이면 `Idle`, `buy`면 `Holding`. **실전 매수 이전에는 sell 시그널을 무시**해서, 승격 이전 paper 포지션을 실전이 뒤쫓아 청산하는 일을 막는다.

```rust
#[derive(Debug, PartialEq)]
enum RealPosition { Idle, Holding }

fn derive_real_position(db: &Connection, session_id: i64) -> RealPosition {
    // live_trades 중 is_real=1 만 대상으로 가장 최근 side 조회
    let last_real_side: Option<String> = db.query_row(
        "SELECT side FROM live_trades
         WHERE session_id=?1 AND is_real=1
         ORDER BY ts DESC LIMIT 1",
        [session_id], |r| r.get(0),
    ).optional()?;

    match last_real_side.as_deref() {
        Some("buy") => RealPosition::Holding,
        _           => RealPosition::Idle,  // None 또는 sell
    }
}

async fn execute_real_order_gate(
    session: &LiveSession,
    output: &SessionCycleOutput,
    client: &UpbitClient,
    db: &Arc<Mutex<Connection>>,
) -> Result<(), Error> {
    // 방금 처음 승격됐으면 real_started_at만 기록하고 종료
    let Some(real_started_at) = session.real_started_at else {
        set_real_started_at(db, session.id, now_utc())?;
        return Ok(());
    };

    // 게이트 1: 승격 이전 시그널이면 무시
    if output.latest_signal_bar_ts <= real_started_at {
        log_info("pre-promotion signal — skip");
        return Ok(());
    }

    let real_pos = derive_real_position(&db.lock().unwrap(), session.id);

    match (output.latest_signal.as_str(), real_pos) {
        // 신규 매수: 실전 라이프사이클이 Idle이고 Upbit도 idle
        ("buy", RealPosition::Idle) if upbit_is_idle(client, &session.market).await? => {
            try_real_buy(client, db, session, output.current_price).await?;
        }
        // 청산: 실전으로 매수한 포지션이 있을 때만
        ("sell", RealPosition::Holding) if upbit_is_holding(client, &session.market).await? => {
            try_real_sell(client, db, session).await?;
        }
        ("buy",  RealPosition::Holding) => log_info("buy signal but already holding — skip"),
        ("sell", RealPosition::Idle)    => log_info("sell signal but no real position — skip"),
        _ => {}  // hold / ready 계열
    }
    Ok(())
}
```

**규칙 해석:**
- `real_started_at`은 **첫 승격 시에만** 기록, 강등↔승격 반복해도 유지.
- "실전 라이프사이클 포지션"은 세션이 **실전 모드에서 직접 실행한 매수**만 인정. 사용자가 다른 경로(수동 매수, 기존 보유분)로 갖고 있는 ETH는 이 세션의 실전 매도 대상이 아니다.
- 세션 삭제 후 재생성은 `real_started_at=NULL` + 실전 trade 없음 → 자연 초기화.
- Upbit 실잔고 체크는 **안전 보조장치**. 라이프사이클이 `Holding`인데 실잔고가 비어 있으면 → 불일치 로그 + 주문 미시도 (추후 reconcile 플로우로 처리).

### 5.3 `services/tick_broker.rs`

```rust
pub struct TickBroker {
    last_tick: Arc<RwLock<HashMap<String, TickData>>>,
    markets: Vec<String>,
    cancel: Arc<AtomicBool>,
}

impl TickBroker {
    pub fn start(app: tauri::AppHandle, markets: Vec<String>) -> Arc<Self>;
    pub fn get_last(&self, market: &str) -> Option<TickData>;
}
```

**구현 요점:**
- 의존성: `tokio-tungstenite`
- Upbit WS 엔드포인트: `wss://api.upbit.com/websocket/v1`
- 구독 메시지: `[{"ticket":"bt-live"}, {"type":"ticker","codes":["KRW-ETH"]}]`
- **Keepalive**: 120초 주기 `Ping` 프레임 필수 (Upbit는 3분 idle 시 연결 종료)
- 재연결: 지수 백오프 5s → 10s → 30s → 60s 상한
- 수신 시: `last_tick` 캐시 갱신 + `app.emit("market:tick", &tick)` 재방송

**PWA (Axum) 연동:**
- `/sse/market` 엔드포인트 신설 → 브로커의 `tokio::sync::broadcast` 채널 구독해 재송신 (SSE 선택 이유: Axum 구현이 단순하고 프론트 EventSource API만으로 충분)
- 프론트 `api.ts`는 Tauri 환경이면 `listen("market:tick")`, 아니면 EventSource로 분기

### 5.4 스케줄러

단일 Tokio 태스크:
```
loop {
    wait = seconds_until_next_hour();
    tokio::time::sleep(wait).await;
    let sessions = db.query("SELECT * FROM live_sessions WHERE status='running'");
    for s in sessions {
        let preset = db.query("SELECT * FROM presets WHERE id=?", s.preset_id);
        let output = session_engine::run_session_cycle(...).await;
        emit_session_events(&app, &s, &output);
        if s.mode == "real" { execute_real_order_gate(&s, &output).await; }
    }
}
```

세션 순회는 순차(세션 하나당 수백 ms 예상). 병렬화는 불필요 — Upbit REST 레이트리밋 여유 확보 목적이 더 큼.

### 5.5 AppState 변경

```rust
pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: StrategyRegistry,
    pub paper_sessions: Mutex<HashMap<i64, SessionHandle>>,  // ← 변경
    pub real_session: Mutex<Option<i64>>,                    // ← 신규 (session_id)
    pub tick_broker: Arc<TickBroker>,                        // ← 신규
    pub optimization: Mutex<Option<OptimizationHandle>>,
}
```

기존 `auto_trading: Mutex<Option<AutoTradingHandle>>`는 제거하고 `real_session`으로 대체.

### 5.6 커맨드 목록

```
create_preset(name, strategy_key, params_json, source, source_run_id?)
list_presets() -> Vec<Preset>
delete_preset(id)

create_session(label, preset_id, initial_capital, start_offset_days?)
list_sessions() -> Vec<LiveSession>
start_session(id)      // status: stopped → running
stop_session(id)       // status: running → stopped
delete_session(id)     // CASCADE

promote_to_real(session_id)    // mode: paper → real, 조건 검증
demote_to_paper(session_id)    // mode: real → paper

// 세션 상세 조회
get_session_trades(session_id, limit?) -> Vec<LiveTrade>
get_session_equity_curve(session_id) -> Vec<EquityPoint>
```

## 6. 프론트엔드 상세

### 6.1 페이지 레이아웃 (1440px 기준)

```
┌─────────────────────────────────────────────────────────────┐
│  ETH 3,245,000 KRW  +1.24%   [Real: V3-OptA]               │
│  KRW 842,000   ETH 0.25                                     │
├──────────────┬──────────────────────────────────────────────┤
│  [+] New     │  CandleChart (KRW-ETH, 1h)                   │
│              │   - SMA/BB/Volume 오버레이 토글               │
│ [S1 V3-A]    │   - 세션별 마커 (실전=굵음, 페이퍼=얇음)      │
│ Real ● +2.4% │                                              │
│ [S2 V3-B]    ├──────────────────────────────────────────────┤
│ Paper +0.8%  │  SignalLaneChart (시간축 동기화)              │
│ [S3 V5-X]    │   - 세션당 수평 레인                          │
│ Paper -1.2%  │   - 점 모양: Buy/BuyReady/Sell/SellReady      │
├──────────────┴──────────────────────────────────────────────┤
│  로그 (세션 필터 드롭다운)            [수동 매수][수동 매도] │
└─────────────────────────────────────────────────────────────┘
```

### 6.2 컴포넌트 분해

```
src/pages/LiveTradingPage.tsx
src/components/live/
  ├─ LiveKpiBar.tsx
  ├─ SessionList.tsx
  │   └─ SessionCard.tsx            (P/L, 상태 토글, 승격 버튼)
  ├─ NewSessionDialog.tsx           (프리셋 선택 + 초기자본)
  ├─ PromoteDialog.tsx              (확인 + 위험 고지)
  ├─ charts/
  │   ├─ CandleChart.tsx
  │   ├─ SignalLaneChart.tsx
  │   └─ useChartSync.ts            (timeScale 동기화 훅)
  └─ LiveLogPanel.tsx
src/stores/liveTradingStore.ts      (Zustand)
```

### 6.3 세션 상태 머신

```
Created → Paper-Running ⇄ Stopped
              ↓ promote
         Real-Running ⇄ Stopped
              ↓ demote
         Paper-Running
```

### 6.4 차트 구현 지침

**CandleChart:**
- `lightweight-charts`의 `createChart()` + `addCandlestickSeries()`
- 세션마다 dummy `LineSeries`를 만들고 거기 `setMarkers()` 호출 → 세션 on/off 토글이 `applyOptions({ visible })` 한 줄로 해결
- 오버레이 시리즈: SMA, BB upper/lower, Volume (별도 priceScale)
- Running candle: `market:tick` 수신 시 마지막 봉에 대해 `series.update({ time, open, high: max, low: min, close: tick.price })`

**SignalLaneChart:**
- 별도 Chart 인스턴스, y축 = 세션 인덱스
- 각 세션마다 `LineSeries` 하나 (가시 라인은 투명), `setMarkers()`로 점 찍기
- 점 모양으로 시그널 타입 구분 (`shape: 'circle' | 'square'`), hollow 여부로 Ready 구분

**useChartSync:**
```typescript
function useChartSync(charts: IChartApi[]) {
  useEffect(() => {
    const subs = charts.map((src, i) =>
      src.timeScale().subscribeVisibleLogicalRangeChange(r => {
        if (!r) return;
        charts.forEach((dst, j) => {
          if (i !== j) dst.timeScale().setVisibleLogicalRange(r);
        });
      })
    );
    // (구독 해제)
  }, [charts]);
}
```

### 6.5 세션 생성 다이얼로그

| 필드 | 설명 |
|------|------|
| Label | 세션 구분 이름 |
| Market | `KRW-ETH` 고정 |
| Preset | 드롭다운 (수동/Optimization 결과 혼합 표시) |
| Initial Capital | 기본 1,000,000 KRW |
| Start from | ● 지금부터 ○ 과거 N일부터 (리플레이) |

### 6.6 실전 승격 다이얼로그

- 현재 페이퍼 세션이 `holding` 상태면 경고 문구 노출:
  > "이 세션은 현재 포지션 보유 중입니다. 승격 후에도 기존 시뮬레이션 포지션은 실전에 반영되지 않으며, 새 buy 시그널이 발생한 시점부터 실주문이 시도됩니다."
- 이미 다른 실전 세션이 있으면:
  > "현재 `S1 V3-A`가 실전 중입니다. 해당 세션이 페이퍼로 강등됩니다. 진행하시겠습니까?"
- 확인 체크박스 + "실전으로 승격" 버튼

### 6.7 실시간 P/L 합성 (프론트)

```typescript
const deriveSession = (session: LiveSession, tick: TickData) => {
  if (session.position === "holding") {
    const unrealized = (tick.price - session.buy_price) * session.buy_volume;
    return {
      unrealizedPnl: unrealized,
      unrealizedPnlPct: (tick.price / session.buy_price - 1) * 100,
      currentEquity: session.realized_equity + unrealized,
    };
  }
  return {
    unrealizedPnl: 0,
    unrealizedPnlPct: 0,
    currentEquity: session.realized_equity,
  };
};
```

백엔드는 미실현 P/L을 계산하지 않는다. 프론트에서 틱 × 세션 스냅샷으로 합성.

## 7. 테스트 전략

### 7.1 백엔드 유닛

- `session_engine::run_session_cycle` — 고정 CSV 주입 → 결정적 결과 검증
- `session_engine::diff_trades` — 멱등성 (동일 입력 N번 호출 → 1회 insert)
- `tick_broker::parse_tick` — 대표 WS 메시지 4~5개
- `execute_real_order_gate` 회귀 방지:
  - 승격 직후 첫 사이클 (`real_started_at` 미설정) → `real_started_at`만 설정, 주문 없음
  - 승격 이전 buy가 마지막 시그널 → 주문 없음 (게이트 1)
  - 승격 이전 paper가 `holding`, 승격 후 sell 시그널 발생 → 주문 없음 (게이트 2, 라이프사이클 Idle)
  - 승격 이후 새 buy 시그널 → 주문 시도 (Upbit idle 체크 포함)
  - 실전 buy 후 sell 시그널 → 주문 시도
  - 이미 라이프사이클 Holding인데 buy 시그널 → 주문 없음 (중복 방지)
- `derive_real_position` 파생 로직 (is_real=1 필터링)
- CRUD: `presets`, `live_sessions`, `live_trades`, `live_equity` (인메모리 SQLite)

### 7.2 백엔드 통합 (`scenario_tests`)

신규 `live_session_scenario.rs`:
1. 프리셋 3개 → 세션 3개 → 동일 데이터에서 사이클 반복 → 세션별로 trades 수·equity가 다르게 나옴 확인
2. 세션 승격 → Mock UpbitClient → 승격 이전 포지션은 실주문되지 않음 확인
3. 재시작 시나리오: 세션 저장 → 재개 후 추가된 봉 반영 확인

### 7.3 프론트 유닛

- `deriveSessionPnl` — 틱 + 스냅샷 → 미실현 P/L
- `SessionCard` 상태 머신 전이
- 세션 색 팔레트 할당

### 7.4 수동 QA 체크리스트

- [ ] WS 끊김 시 자동 재접속 (keepalive 포함)
- [ ] 세션 3개 동시 실행, 시간축 동기 스크롤 매끄러움
- [ ] 1시간 경계 직후 새 봉 마커 반영
- [ ] 실전 승격 다이얼로그 취소 → 상태 무변경
- [ ] 세션 삭제 → trades/equity CASCADE
- [ ] PWA 모드 기본 동작 (SSE 틱 포함)

## 8. 단계별 롤아웃

| Phase | 범위 | 산출물 | 예상 |
|-------|------|--------|------|
| 1 | 세션 엔진 + 프리셋 | 마이그레이션, `session_engine.rs`, 커맨드 CRUD, 최소 UI (표 형태) | 2~3일 |
| 2 | WebSocket 틱 + 실시간 P/L | `tick_broker.rs`, `tokio-tungstenite`, Tauri/SSE 이벤트, 프론트 P/L 합성 | 2일 |
| 3 | 2-패널 차트 | `lightweight-charts`, `CandleChart`, `SignalLaneChart`, 동기화 훅, 로그 필터 | 3~4일 |
| 4 | 실전 승격 + 안전장치 | `promote/demote` 커맨드, `real_started_at` 게이트, `PromoteDialog`, 레거시 `auto_trading` 제거 | 2일 |

각 Phase 완료 시점에 독립 사용 가능한 가치 제공.

## 9. 기존 코드와의 관계

**제거 대상 (Phase 4):**
- `AppState.auto_trading: Mutex<Option<AutoTradingHandle>>`
- `commands/trading.rs`의 `start_auto_trading` / `stop_auto_trading` / `get_auto_trading_status`
- `tradingStore`의 auto-trading 상태
- `LiveTradingPage.tsx`의 기존 컨트롤 UI 전체 교체

**유지/재사용:**
- `services/auto_trader.rs`의 순수 함수: `reconcile_position`, `fetch_and_prepare_data`, `update_market_data`, `calculate_split_orders`, `record_trade`, `save_position`
- `commands/trading.rs`의 수동 매수/매도 (`manual_buy`, `manual_sell`, `get_current_price`, `get_balance`)
- `NotificationManager` — 실전 체결 이벤트에서 호출

## 10. 오픈 이슈

- `SimulationResult`가 `last_signal_bar_ts` / `final_position` / `final_equity` 필드를 이미 노출하는지 구현 시점에 확인. 없으면 `Strategy` trait에 얇게 확장 (리플레이 기반 설계의 전제).
- Optimization 결과를 프리셋으로 export하는 UI는 Optimization 페이지 쪽 수정이 필요 (본 설계 범위 밖 또는 Phase 5로 분리).
- `positions` / `trades` 레거시 테이블 마이그레이션 여부 — Phase 4 착수 시 결정 (현재로는 제거 권장).

---

## 부록 A. 핵심 규칙 재명시

1. **리플레이 기반 결정성**: 매 사이클마다 `session.start_ts → now` 풀 시뮬레이션. 백테스트 = 페이퍼 = 실전 시그널 일치.
2. **diff 저장**: `run_simulation`이 생성한 전체 trade 중 `last_recorded_ts`보다 큰 것만 `live_trades`에 insert.
3. **실전 주문 2단 게이트**:
   - (a) `output.latest_signal_bar_ts > session.real_started_at`
   - (b) 세션의 실전 라이프사이클 포지션 (`is_real=1` trade 이력 기반) — 매수는 `Idle`일 때만, 매도는 `Holding`일 때만. 첫 실전 매수 이전에 오는 sell 시그널은 무시.
4. **실전 단일성**: `real_session: Mutex<Option<SessionId>>` — 시장당 최대 1개.
5. **백엔드는 틱 계산 안 함**: 틱은 재방송만. 미실현 P/L은 프론트가 합성.
