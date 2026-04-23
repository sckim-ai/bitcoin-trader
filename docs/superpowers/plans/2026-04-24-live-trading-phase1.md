# Live Trading Phase 1 Implementation Plan (세션 엔진 + 프리셋)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 다수 전략 페이퍼 세션을 DB에 영속화하고, 1시간 경계마다 `Strategy::run_simulation` 리플레이로 시그널·체결·에쿼티를 갱신하는 세션 엔진 및 최소 UI를 구축한다.

**Architecture:** 기존 `core/engine.rs`의 백테스트 엔진(`Strategy::run_simulation`)을 라이브 모드에서 그대로 재사용한다. 세션 엔진은 세션 시작 시점부터 현재까지의 봉을 가져와 매 사이클 전체를 리플레이하고, DB의 `live_trades`와 diff를 내서 신규 체결만 삽입한다. 단일 스케줄러 태스크가 `status='running'` 세션들을 순회한다.

**Tech Stack:** Rust (tokio, rusqlite, chrono, serde), TypeScript/React (Zustand, Tauri IPC / Axum HTTP).

**Scope (Phase 1):**
- 마이그레이션: `presets`, `live_sessions`, `live_trades`, `live_equity`
- 백엔드: `services/session_engine.rs`, `services/live_scheduler.rs`, `AppState` 확장
- Tauri 커맨드: 프리셋/세션 CRUD + start/stop
- 프론트: 임시 표 UI (세션 목록 + P/L 컬럼 + 생성/정지/삭제). 차트·WebSocket은 Phase 2~3로 연기.

**Out-of-scope (Phase 2 이후):** WebSocket 틱 브로커, 2-패널 차트, 실전 승격 게이트, `real_started_at` 로직.

---

## 파일 구조 (Phase 1에서 신규/수정될 파일)

**신규:**
- `src-tauri/migrations/006_live_trading.sql` — 4개 테이블
- `src-tauri/src/models/live.rs` — Preset, LiveSession, LiveTrade, LiveEquity 도메인 타입
- `src-tauri/src/db/live_repo.rs` — 4개 테이블 CRUD
- `src-tauri/src/services/session_engine.rs` — 리플레이 사이클 + diff 삽입
- `src-tauri/src/services/live_scheduler.rs` — 시간 경계 스케줄러
- `src-tauri/src/commands/live_trading.rs` — Tauri 커맨드
- `src-tauri/tests/live_session_test.rs` — 엔진 통합 테스트
- `src/lib/live.ts` — 프론트 API 클라이언트
- `src/stores/liveTradingStore.ts` — Zustand 스토어 (Phase 2에서 확장)
- `src/pages/LiveTradingPage.tsx` — Phase 1에서는 최소 UI로 축소. 기존 파일 대체.
- `src/components/live/SessionTable.tsx`
- `src/components/live/NewSessionDialog.tsx`

**수정:**
- `src-tauri/src/db/schema.rs` — 마이그레이션 006 로딩 추가
- `src-tauri/src/state.rs` — `paper_sessions` 필드 추가
- `src-tauri/src/lib.rs` — 스케줄러 스레드 실행, 커맨드 등록, AppState 초기화
- `src-tauri/src/services/mod.rs` — 신규 모듈 exports
- `src-tauri/src/commands/mod.rs` — 신규 모듈 exports
- `src-tauri/src/models/mod.rs` — `live` 모듈 exports
- `src-tauri/src/db/mod.rs` — `live_repo` 모듈 exports
- `src/types/index.ts` — 신규 타입 (Preset, LiveSession, LiveTrade)

---

## Task 1: 마이그레이션 파일 작성

**Files:**
- Create: `src-tauri/migrations/006_live_trading.sql`
- Modify: `src-tauri/src/db/schema.rs` (마이그레이션 로딩 부)

- [ ] **Step 1: 마이그레이션 SQL 작성**

Create `src-tauri/migrations/006_live_trading.sql`:

```sql
-- Phase 1: Live trading multi-session schema.
-- Presets, sessions, trades, equity snapshots.

CREATE TABLE IF NOT EXISTS presets (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id       INTEGER NOT NULL DEFAULT 1,
  name          TEXT NOT NULL,
  strategy_key  TEXT NOT NULL,
  params_json   TEXT NOT NULL,
  source        TEXT NOT NULL DEFAULT 'manual',
  source_run_id INTEGER,
  created_at    TEXT NOT NULL,
  UNIQUE(user_id, name)
);

CREATE TABLE IF NOT EXISTS live_sessions (
  id                  INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id             INTEGER NOT NULL DEFAULT 1,
  label               TEXT NOT NULL,
  preset_id           INTEGER NOT NULL REFERENCES presets(id),
  market              TEXT NOT NULL,
  mode                TEXT NOT NULL DEFAULT 'paper',
  status              TEXT NOT NULL DEFAULT 'stopped',
  initial_capital     REAL NOT NULL,
  start_ts            TEXT NOT NULL,
  real_started_at     TEXT,
  last_cycle_ts       TEXT,
  last_signal         TEXT,
  current_position    TEXT NOT NULL DEFAULT 'idle',
  current_buy_price   REAL,
  current_buy_volume  REAL,
  current_equity      REAL,
  created_at          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_live_sessions_status ON live_sessions(status);

CREATE TABLE IF NOT EXISTS live_trades (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id   INTEGER NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  ts           TEXT NOT NULL,
  side         TEXT NOT NULL,
  price        REAL NOT NULL,
  volume       REAL NOT NULL,
  fee          REAL NOT NULL,
  signal       TEXT NOT NULL,
  pnl          REAL,
  pnl_pct      REAL,
  is_real      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_live_trades_session ON live_trades(session_id, ts);

CREATE TABLE IF NOT EXISTS live_equity (
  session_id  INTEGER NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  ts          TEXT NOT NULL,
  equity      REAL NOT NULL,
  position    TEXT NOT NULL,
  PRIMARY KEY (session_id, ts)
);
```

- [ ] **Step 2: `schema.rs`에 마이그레이션 006 로딩 추가**

Modify `src-tauri/src/db/schema.rs` — `005` 블록 아래에 추가:

```rust
    let schema_v6 = include_str!("../../migrations/006_live_trading.sql");
    if let Err(e) = conn.execute_batch(schema_v6) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
```

위치는 `005` 마이그레이션 블록 직후, `optimization_runs` backfill(`UPDATE optimization_runs ...`) 실행 전.

- [ ] **Step 3: 빌드 확인**

Run: `cd src-tauri && cargo build --features tauri-app`
Expected: 경고 없이 컴파일 성공. 런타임에 테이블이 생성되는지는 Task 2 이후 테스트로 검증.

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/migrations/006_live_trading.sql src-tauri/src/db/schema.rs
git commit -m "feat(live): add migration 006 for presets/sessions/trades/equity"
```

---

## Task 2: 도메인 타입 정의 (`models/live.rs`)

**Files:**
- Create: `src-tauri/src/models/live.rs`
- Modify: `src-tauri/src/models/mod.rs`

- [ ] **Step 1: 도메인 타입 파일 작성**

Create `src-tauri/src/models/live.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub strategy_key: String,
    pub params_json: String,
    pub source: String,
    pub source_run_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionMode {
    #[serde(rename = "paper")]
    Paper,
    #[serde(rename = "real")]
    Real,
}

impl SessionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionMode::Paper => "paper",
            SessionMode::Real => "real",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "paper" => Some(SessionMode::Paper),
            "real" => Some(SessionMode::Real),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "stopped")]
    Stopped,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Running => "running",
            SessionStatus::Stopped => "stopped",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "running" => Some(SessionStatus::Running),
            "stopped" => Some(SessionStatus::Stopped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSession {
    pub id: i64,
    pub user_id: i64,
    pub label: String,
    pub preset_id: i64,
    pub market: String,
    pub mode: String,              // 'paper' | 'real'
    pub status: String,            // 'running' | 'stopped'
    pub initial_capital: f64,
    pub start_ts: String,
    pub real_started_at: Option<String>,
    pub last_cycle_ts: Option<String>,
    pub last_signal: Option<String>,
    pub current_position: String,  // 'idle' | 'holding'
    pub current_buy_price: Option<f64>,
    pub current_buy_volume: Option<f64>,
    pub current_equity: Option<f64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveTrade {
    pub id: i64,
    pub session_id: i64,
    pub ts: String,
    pub side: String,              // 'buy' | 'sell'
    pub price: f64,
    pub volume: f64,
    pub fee: f64,
    pub signal: String,
    pub pnl: Option<f64>,
    pub pnl_pct: Option<f64>,
    pub is_real: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveEquityPoint {
    pub session_id: i64,
    pub ts: String,
    pub equity: f64,
    pub position: String,
}
```

- [ ] **Step 2: 모듈 export 추가**

Modify `src-tauri/src/models/mod.rs` — 기존 pub mod 선언 아래에 추가:

```rust
pub mod live;
```

(기존 `pub mod config;`, `pub mod market;`, `pub mod trading;`의 근처)

- [ ] **Step 3: 빌드 확인**

Run: `cd src-tauri && cargo build --features tauri-app`
Expected: 경고 없이 빌드 성공.

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/src/models/live.rs src-tauri/src/models/mod.rs
git commit -m "feat(live): add domain types for presets/sessions/trades"
```

---

## Task 3: Preset 리포지토리

**Files:**
- Create: `src-tauri/src/db/live_repo.rs`
- Modify: `src-tauri/src/db/mod.rs`

- [ ] **Step 1: 실패하는 테스트 먼저 작성**

Create `src-tauri/src/db/live_repo.rs`:

```rust
use crate::models::live::{LiveSession, LiveTrade, LiveEquityPoint, Preset};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result};

// ─── Presets ───

pub fn insert_preset(
    conn: &Connection,
    user_id: i64,
    name: &str,
    strategy_key: &str,
    params_json: &str,
    source: &str,
    source_run_id: Option<i64>,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO presets (user_id, name, strategy_key, params_json, source, source_run_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![user_id, name, strategy_key, params_json, source, source_run_id, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_preset(conn: &Connection, id: i64) -> Result<Option<Preset>> {
    conn.query_row(
        "SELECT id, user_id, name, strategy_key, params_json, source, source_run_id, created_at
         FROM presets WHERE id = ?1",
        [id],
        |row| Ok(Preset {
            id: row.get(0)?,
            user_id: row.get(1)?,
            name: row.get(2)?,
            strategy_key: row.get(3)?,
            params_json: row.get(4)?,
            source: row.get(5)?,
            source_run_id: row.get(6)?,
            created_at: row.get(7)?,
        }),
    )
    .optional()
}

pub fn list_presets(conn: &Connection, user_id: i64) -> Result<Vec<Preset>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, name, strategy_key, params_json, source, source_run_id, created_at
         FROM presets WHERE user_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([user_id], |row| Ok(Preset {
        id: row.get(0)?,
        user_id: row.get(1)?,
        name: row.get(2)?,
        strategy_key: row.get(3)?,
        params_json: row.get(4)?,
        source: row.get(5)?,
        source_run_id: row.get(6)?,
        created_at: row.get(7)?,
    }))?;
    rows.collect()
}

pub fn delete_preset(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM presets WHERE id = ?1", [id])
}

// TODO: Sessions, Trades, Equity — Task 4~6

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let s1 = include_str!("../../migrations/001_initial.sql");
        conn.execute_batch(s1).unwrap();
        let s2 = include_str!("../../migrations/002_users.sql");
        conn.execute_batch(s2).unwrap();
        let s6 = include_str!("../../migrations/006_live_trading.sql");
        conn.execute_batch(s6).unwrap();
        conn
    }

    #[test]
    fn test_preset_insert_and_get() {
        let conn = setup_db();
        let id = insert_preset(&conn, 1, "V3-A", "V3", r#"{"foo":1}"#, "manual", None).unwrap();
        let preset = get_preset(&conn, id).unwrap().expect("preset should exist");
        assert_eq!(preset.name, "V3-A");
        assert_eq!(preset.strategy_key, "V3");
        assert_eq!(preset.source, "manual");
    }

    #[test]
    fn test_preset_list_ordering() {
        let conn = setup_db();
        insert_preset(&conn, 1, "A", "V3", "{}", "manual", None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        insert_preset(&conn, 1, "B", "V3", "{}", "manual", None).unwrap();
        let list = list_presets(&conn, 1).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "B"); // 최신이 먼저
    }

    #[test]
    fn test_preset_delete() {
        let conn = setup_db();
        let id = insert_preset(&conn, 1, "X", "V3", "{}", "manual", None).unwrap();
        let n = delete_preset(&conn, id).unwrap();
        assert_eq!(n, 1);
        assert!(get_preset(&conn, id).unwrap().is_none());
    }

    #[test]
    fn test_preset_unique_name_per_user() {
        let conn = setup_db();
        insert_preset(&conn, 1, "dup", "V3", "{}", "manual", None).unwrap();
        let result = insert_preset(&conn, 1, "dup", "V3", "{}", "manual", None);
        assert!(result.is_err(), "duplicate name should fail");
    }
}
```

- [ ] **Step 2: 모듈 export 추가**

Modify `src-tauri/src/db/mod.rs` — 기존 선언 아래에 추가:

```rust
pub mod live_repo;
```

- [ ] **Step 3: 테스트 실행**

Run: `cd src-tauri && cargo test --lib db::live_repo::tests`
Expected: 4개 테스트 모두 PASS.

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/src/db/live_repo.rs src-tauri/src/db/mod.rs
git commit -m "feat(live): preset CRUD with unit tests"
```

---

## Task 4: LiveSession 리포지토리

**Files:**
- Modify: `src-tauri/src/db/live_repo.rs`

- [ ] **Step 1: 세션 CRUD 함수 + 테스트 추가**

`TODO: Sessions, Trades, Equity` 주석을 다음으로 교체:

```rust
// ─── Live Sessions ───

pub fn insert_session(
    conn: &Connection,
    user_id: i64,
    label: &str,
    preset_id: i64,
    market: &str,
    mode: &str,
    initial_capital: f64,
    start_ts: &str,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO live_sessions
            (user_id, label, preset_id, market, mode, status, initial_capital,
             start_ts, current_position, current_equity, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'stopped', ?6, ?7, 'idle', ?6, ?8)",
        params![user_id, label, preset_id, market, mode, initial_capital, start_ts, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_session(conn: &Connection, id: i64) -> Result<Option<LiveSession>> {
    conn.query_row(
        "SELECT id, user_id, label, preset_id, market, mode, status, initial_capital,
                start_ts, real_started_at, last_cycle_ts, last_signal,
                current_position, current_buy_price, current_buy_volume, current_equity,
                created_at
         FROM live_sessions WHERE id = ?1",
        [id],
        row_to_session,
    )
    .optional()
}

pub fn list_sessions(conn: &Connection, user_id: i64) -> Result<Vec<LiveSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, label, preset_id, market, mode, status, initial_capital,
                start_ts, real_started_at, last_cycle_ts, last_signal,
                current_position, current_buy_price, current_buy_volume, current_equity,
                created_at
         FROM live_sessions WHERE user_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([user_id], row_to_session)?;
    rows.collect()
}

pub fn list_running_sessions(conn: &Connection) -> Result<Vec<LiveSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, label, preset_id, market, mode, status, initial_capital,
                start_ts, real_started_at, last_cycle_ts, last_signal,
                current_position, current_buy_price, current_buy_volume, current_equity,
                created_at
         FROM live_sessions WHERE status = 'running'",
    )?;
    let rows = stmt.query_map([], row_to_session)?;
    rows.collect()
}

pub fn set_session_status(conn: &Connection, id: i64, status: &str) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET status = ?1 WHERE id = ?2",
        params![status, id],
    )
}

pub fn update_session_cycle(
    conn: &Connection,
    id: i64,
    last_cycle_ts: &str,
    last_signal: &str,
    current_position: &str,
    current_buy_price: Option<f64>,
    current_buy_volume: Option<f64>,
    current_equity: f64,
) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET
            last_cycle_ts = ?1,
            last_signal = ?2,
            current_position = ?3,
            current_buy_price = ?4,
            current_buy_volume = ?5,
            current_equity = ?6
         WHERE id = ?7",
        params![last_cycle_ts, last_signal, current_position,
                current_buy_price, current_buy_volume, current_equity, id],
    )
}

pub fn delete_session(conn: &Connection, id: i64) -> Result<usize> {
    // CASCADE가 live_trades / live_equity에 걸려 있음
    conn.execute("DELETE FROM live_sessions WHERE id = ?1", [id])
}

fn row_to_session(row: &rusqlite::Row) -> Result<LiveSession> {
    Ok(LiveSession {
        id: row.get(0)?,
        user_id: row.get(1)?,
        label: row.get(2)?,
        preset_id: row.get(3)?,
        market: row.get(4)?,
        mode: row.get(5)?,
        status: row.get(6)?,
        initial_capital: row.get(7)?,
        start_ts: row.get(8)?,
        real_started_at: row.get(9)?,
        last_cycle_ts: row.get(10)?,
        last_signal: row.get(11)?,
        current_position: row.get(12)?,
        current_buy_price: row.get(13)?,
        current_buy_volume: row.get(14)?,
        current_equity: row.get(15)?,
        created_at: row.get(16)?,
    })
}

// TODO: Trades, Equity — Task 5~6
```

테스트 모듈(`mod tests`) 하단에 추가:

```rust
    #[test]
    fn test_session_insert_and_defaults() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();
        let s = get_session(&conn, sid).unwrap().expect("session exists");
        assert_eq!(s.label, "S1");
        assert_eq!(s.status, "stopped");
        assert_eq!(s.current_position, "idle");
        assert_eq!(s.mode, "paper");
        assert!((s.current_equity.unwrap() - 1_000_000.0).abs() < 0.01);
    }

    #[test]
    fn test_session_status_transitions() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();

        assert_eq!(list_running_sessions(&conn).unwrap().len(), 0);
        set_session_status(&conn, sid, "running").unwrap();
        assert_eq!(list_running_sessions(&conn).unwrap().len(), 1);
        set_session_status(&conn, sid, "stopped").unwrap();
        assert_eq!(list_running_sessions(&conn).unwrap().len(), 0);
    }

    #[test]
    fn test_session_cycle_update() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();

        update_session_cycle(&conn, sid, "2026-04-24T05:00:00Z", "buy", "holding",
            Some(3_200_000.0), Some(0.312), 1_100_000.0).unwrap();

        let s = get_session(&conn, sid).unwrap().unwrap();
        assert_eq!(s.last_signal.as_deref(), Some("buy"));
        assert_eq!(s.current_position, "holding");
        assert!((s.current_buy_price.unwrap() - 3_200_000.0).abs() < 0.01);
        assert!((s.current_equity.unwrap() - 1_100_000.0).abs() < 0.01);
    }
```

- [ ] **Step 2: 테스트 실행**

Run: `cd src-tauri && cargo test --lib db::live_repo::tests`
Expected: Preset 4개 + Session 3개 = 총 7개 테스트 PASS.

- [ ] **Step 3: 커밋**

```bash
git add src-tauri/src/db/live_repo.rs
git commit -m "feat(live): live_sessions CRUD + status/cycle update"
```

---

## Task 5: LiveTrade 리포지토리 (diff 삽입 포함)

**Files:**
- Modify: `src-tauri/src/db/live_repo.rs`

- [ ] **Step 1: trade 관련 함수 + 테스트 추가**

`TODO: Trades, Equity` 주석을 다음으로 교체:

```rust
// ─── Live Trades ───

pub fn insert_trade(
    conn: &Connection,
    session_id: i64,
    ts: &str,
    side: &str,
    price: f64,
    volume: f64,
    fee: f64,
    signal: &str,
    pnl: Option<f64>,
    pnl_pct: Option<f64>,
    is_real: bool,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO live_trades
            (session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct, is_real)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct,
                if is_real { 1 } else { 0 }],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_trades(conn: &Connection, session_id: i64) -> Result<Vec<LiveTrade>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct, is_real
         FROM live_trades WHERE session_id = ?1 ORDER BY ts ASC, id ASC",
    )?;
    let rows = stmt.query_map([session_id], |row| Ok(LiveTrade {
        id: row.get(0)?,
        session_id: row.get(1)?,
        ts: row.get(2)?,
        side: row.get(3)?,
        price: row.get(4)?,
        volume: row.get(5)?,
        fee: row.get(6)?,
        signal: row.get(7)?,
        pnl: row.get(8)?,
        pnl_pct: row.get(9)?,
        is_real: row.get::<_, i64>(10)? != 0,
    }))?;
    rows.collect()
}

/// Count completed trades = number of sell rows.
/// Phase 1 inserts only completed pairs (buy + sell both), so sell count =
/// completed-trade count.
pub fn count_completed_trades(conn: &Connection, session_id: i64) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM live_trades WHERE session_id = ?1 AND side = 'sell'",
        [session_id],
        |r| r.get(0),
    )?;
    Ok(n as usize)
}

// ─── Live Equity ───

pub fn upsert_equity(conn: &Connection, session_id: i64, ts: &str, equity: f64, position: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO live_equity (session_id, ts, equity, position) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(session_id, ts) DO UPDATE SET equity = ?3, position = ?4",
        params![session_id, ts, equity, position],
    )?;
    Ok(())
}

pub fn list_equity(conn: &Connection, session_id: i64) -> Result<Vec<LiveEquityPoint>> {
    let mut stmt = conn.prepare(
        "SELECT session_id, ts, equity, position FROM live_equity
         WHERE session_id = ?1 ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map([session_id], |row| Ok(LiveEquityPoint {
        session_id: row.get(0)?,
        ts: row.get(1)?,
        equity: row.get(2)?,
        position: row.get(3)?,
    }))?;
    rows.collect()
}
```

테스트 모듈 하단에 추가:

```rust
    #[test]
    fn test_trade_insert_and_list() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy", 3_000_000.0, 0.3, 450.0, "buy", None, None, false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T03:00:00Z", "sell", 3_100_000.0, 0.3, 465.0, "sell", Some(30_000.0), Some(3.3), false).unwrap();

        let trades = list_trades(&conn, sid).unwrap();
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].side, "buy");
        assert_eq!(trades[1].side, "sell");
        assert_eq!(trades[1].pnl, Some(30_000.0));
    }

    #[test]
    fn test_count_completed_trades() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 0);
        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy", 3e6, 0.3, 450.0, "buy", None, None, false).unwrap();
        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 0); // 아직 sell 없음
        insert_trade(&conn, sid, "2026-04-24T03:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(30e3), Some(3.3), false).unwrap();
        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 1);
    }

    #[test]
    fn test_equity_upsert() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        upsert_equity(&conn, sid, "2026-04-24T01:00:00Z", 1_000_000.0, "idle").unwrap();
        upsert_equity(&conn, sid, "2026-04-24T01:00:00Z", 1_050_000.0, "holding").unwrap(); // 덮어쓰기
        upsert_equity(&conn, sid, "2026-04-24T02:00:00Z", 1_080_000.0, "holding").unwrap();

        let points = list_equity(&conn, sid).unwrap();
        assert_eq!(points.len(), 2);
        assert!((points[0].equity - 1_050_000.0).abs() < 0.01);
        assert!((points[1].equity - 1_080_000.0).abs() < 0.01);
    }

    #[test]
    fn test_delete_session_cascades() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();
        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy", 3e6, 0.3, 450.0, "buy", None, None, false).unwrap();
        upsert_equity(&conn, sid, "2026-04-24T01:00:00Z", 1e6, "idle").unwrap();

        delete_session(&conn, sid).unwrap();

        assert_eq!(list_trades(&conn, sid).unwrap().len(), 0);
        assert_eq!(list_equity(&conn, sid).unwrap().len(), 0);
    }
```

- [ ] **Step 2: 테스트 실행**

Run: `cd src-tauri && cargo test --lib db::live_repo::tests`
Expected: 11개 테스트 PASS (Preset 4 + Session 3 + Trade/Equity 4).

- [ ] **Step 3: 커밋**

```bash
git add src-tauri/src/db/live_repo.rs
git commit -m "feat(live): live_trades + live_equity repo with CASCADE delete"
```

---

## Task 6: 세션 엔진 — 리플레이 사이클

**Files:**
- Create: `src-tauri/src/services/session_engine.rs`
- Modify: `src-tauri/src/services/mod.rs`

`★ Implementation note:` Phase 1에서는 완료된 buy-sell 쌍만 live_trades에 기록. 진행 중 포지션(last_position=1)은 `live_sessions.current_position/current_buy_price/current_buy_volume`으로만 표현. 이 분리가 Phase 4의 실전 로직을 단순하게 만든다.

- [ ] **Step 1: 엔진 파일 작성 + 유닛 테스트**

Create `src-tauri/src/services/session_engine.rs`:

```rust
use crate::api::upbit::UpbitClient;
use crate::db::live_repo;
use crate::models::live::{LiveSession, Preset};
use crate::models::trading::TradingParameters;
use crate::services::auto_trader::fetch_and_prepare_data;
use crate::strategies::StrategyRegistry;
use chrono::Utc;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Output of a single session cycle — what to log/emit.
#[derive(Debug, Clone)]
pub struct SessionCycleOutput {
    pub session_id: i64,
    pub new_completed_trades: usize,
    pub latest_signal: String,
    pub current_position: String,
    pub current_equity: f64,
}

/// Run one cycle for a single session. Idempotent — re-running on the same
/// data set produces no new DB rows (diff-based insert).
pub async fn run_session_cycle(
    db: &Arc<Mutex<Connection>>,
    client: &UpbitClient,
    session: &LiveSession,
    preset: &Preset,
    registry: &StrategyRegistry,
) -> Result<SessionCycleOutput, BoxErr> {
    // 1. Fetch candles covering session.start_ts ~ now.
    //    Phase 1 shortcut: pull last 500 hourly bars from Upbit + DB-cached
    //    day-psy; trim to session.start_ts at the strategy call site.
    //    (세션이 500시간보다 오래되면 보강 로직은 Phase 2~에서 확장)
    let data = fetch_and_prepare_data(client, db, &session.market, 500).await?;
    let session_start = chrono::DateTime::parse_from_rfc3339(&session.start_ts)
        .map_err(|e| -> BoxErr { e.to_string().into() })?
        .with_timezone(&Utc);
    let data: Vec<_> = data.into_iter()
        .filter(|md| md.candle.timestamp >= session_start)
        .collect();

    if data.len() < 15 {
        return Ok(SessionCycleOutput {
            session_id: session.id,
            new_completed_trades: 0,
            latest_signal: "insufficient_data".into(),
            current_position: session.current_position.clone(),
            current_equity: session.current_equity.unwrap_or(session.initial_capital),
        });
    }

    // 2. Run full simulation from session start to latest candle.
    let strategy = registry.get(&preset.strategy_key)
        .ok_or_else(|| -> BoxErr { format!("strategy not found: {}", preset.strategy_key).into() })?;
    let params: TradingParameters = serde_json::from_str(&preset.params_json)
        .map_err(|e| -> BoxErr { e.to_string().into() })?;
    let result = strategy.run_simulation(&data, &params);

    // 3. Diff: only new completed trades.
    let new_count = {
        let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
        let existing = live_repo::count_completed_trades(&conn, session.id)
            .map_err(|e| -> BoxErr { e.to_string().into() })?;
        result.trades.len().saturating_sub(existing)
    };

    // TradeRecord는 볼륨을 노출하지 않는다. Phase 1 live_trades는 표시 용도로만
    // 쓰이므로, 세션 초기 자본을 체결가로 나눈 러프한 값을 저장한다.
    // (Phase 4 실전 주문 시에는 실제 체결량이 별도 경로로 기록된다.)
    let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
    if new_count > 0 {
        let offset = result.trades.len() - new_count;
        let fee_rate = params.v3_fee_rate;
        for t in &result.trades[offset..] {
            let rough_volume = if t.buy_price > 0.0 {
                session.initial_capital / t.buy_price
            } else {
                0.0
            };
            // Insert buy row
            live_repo::insert_trade(
                &conn, session.id, &t.buy_timestamp, "buy",
                t.buy_price, rough_volume,
                t.buy_price * rough_volume * fee_rate,
                &t.buy_signal, None, None, false,
            ).map_err(|e| -> BoxErr { e.to_string().into() })?;
            // Insert sell row — pnl/pnl_pct가 TradeRecord에는 pct만 있음
            live_repo::insert_trade(
                &conn, session.id, &t.sell_timestamp, "sell",
                t.sell_price, rough_volume,
                t.sell_price * rough_volume * fee_rate,
                &t.sell_signal,
                Some((t.sell_price - t.buy_price) * rough_volume),
                Some(t.pnl_pct),
                false,
            ).map_err(|e| -> BoxErr { e.to_string().into() })?;
        }
    }

    // 4. Current position snapshot from result.last_*.
    let current_position = if result.last_position == 1 { "holding" } else { "idle" };
    let (cbp, cbv) = if result.last_position == 1 {
        (Some(result.last_buy_price), Some(result.last_set_volume))
    } else {
        (None, None)
    };
    let equity = session.initial_capital
        * (1.0 + result.fee_adjusted_return / 100.0);

    // 5. Update session + equity snapshot.
    let last_candle_ts = data.last()
        .map(|md| md.candle.timestamp.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    live_repo::update_session_cycle(
        &conn, session.id, &last_candle_ts, &result.last_signal_type,
        current_position, cbp, cbv, equity,
    ).map_err(|e| -> BoxErr { e.to_string().into() })?;
    live_repo::upsert_equity(&conn, session.id, &last_candle_ts, equity, current_position)
        .map_err(|e| -> BoxErr { e.to_string().into() })?;

    Ok(SessionCycleOutput {
        session_id: session.id,
        new_completed_trades: new_count,
        latest_signal: result.last_signal_type,
        current_position: current_position.into(),
        current_equity: equity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::{Candle, MarketData};
    use crate::core::indicators;
    use chrono::{TimeZone, Utc};

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/001_initial.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/002_users.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/006_live_trading.sql")).unwrap();
        conn
    }

    /// Unit-level test: diff behaviour with a mocked SimulationResult.
    /// (실제 run_session_cycle은 Upbit API + 전략이 얽혀 있어 통합 테스트로 분리)
    #[test]
    fn test_diff_inserts_only_new_trades() {
        use crate::db::live_repo::*;
        let conn = setup_conn();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        // 시나리오: 첫 사이클에 2개 완료, 두번째 사이클에 1개 추가 → 총 3개
        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy",  3e6, 0.3, 450.0, "buy",  None, None, false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T02:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(3.0), Some(3.0), false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T03:00:00Z", "buy",  3.2e6, 0.3, 480.0, "buy",  None, None, false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T04:00:00Z", "sell", 3.3e6, 0.3, 495.0, "sell", Some(2.5), Some(2.5), false).unwrap();

        let existing = count_completed_trades(&conn, sid).unwrap();
        assert_eq!(existing, 2);

        // run_session_cycle 자체는 통합 테스트에서 실데이터와 함께 검증 (Task 10)
    }
}
```

- [ ] **Step 2: 모듈 export 추가**

Modify `src-tauri/src/services/mod.rs`:

```rust
pub mod session_engine;
```

(기존 `pub mod auto_trader;`, `pub mod market_updater;`의 근처)

- [ ] **Step 3: 빌드 및 유닛 테스트**

Run: `cd src-tauri && cargo build --features tauri-app`
Expected: 컴파일 성공.

Run: `cd src-tauri && cargo test --lib services::session_engine`
Expected: 1개 테스트 PASS (diff 로직 확인).

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/src/services/session_engine.rs src-tauri/src/services/mod.rs
git commit -m "feat(live): session engine with replay-based diff insert"
```

---

## Task 7: AppState 확장 + 스케줄러

**Files:**
- Modify: `src-tauri/src/state.rs`
- Create: `src-tauri/src/services/live_scheduler.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: AppState에 paper_sessions 추가**

Modify `src-tauri/src/state.rs` — `AppState` 구조체:

```rust
use crate::core::optimizer::Individual;
use crate::strategies::StrategyRegistry;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct AutoTradingHandle {
    pub cancel_token: Arc<AtomicBool>,
    pub market: String,
    pub strategy_key: String,
}

pub struct OptimizationHandle {
    pub cancel_token: Arc<AtomicBool>,
    pub run_id: Option<i64>,
    pub last_population: Arc<Mutex<Option<Vec<Individual>>>>,
    pub last_generation: Arc<Mutex<usize>>,
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: StrategyRegistry,
    pub auto_trading: Mutex<Option<AutoTradingHandle>>,
    pub optimization: Mutex<Option<OptimizationHandle>>,
    /// Phase 1: 실행 중인 페이퍼 세션 id 집합. 스케줄러가 DB에서 직접
    /// 로드하지만, UI의 즉시성을 위한 캐시.
    pub paper_session_ids: Mutex<HashMap<i64, ()>>,
}

impl AppState {
    pub fn empty() -> Self {
        Self {
            db: Mutex::new(Connection::open_in_memory().expect("in-memory DB")),
            registry: StrategyRegistry::new(),
            auto_trading: Mutex::new(None),
            optimization: Mutex::new(None),
            paper_session_ids: Mutex::new(HashMap::new()),
        }
    }
}
```

- [ ] **Step 2: 스케줄러 파일 작성**

Create `src-tauri/src/services/live_scheduler.rs`:

```rust
use crate::api::upbit::UpbitClient;
use crate::db::live_repo;
use crate::services::session_engine;
use crate::strategies::StrategyRegistry;
use chrono::{Timelike, Utc};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn seconds_until_next_hour() -> u64 {
    let now = Utc::now();
    let secs_into_hour = now.minute() * 60 + now.second();
    let remaining = 3600u64.saturating_sub(secs_into_hour as u64);
    if remaining < 10 { remaining + 3600 } else { remaining }
}

fn create_public_client() -> UpbitClient {
    let ak = std::env::var("UPBIT_ACCESS_KEY").unwrap_or_default();
    let sk = std::env::var("UPBIT_SECRET_KEY").unwrap_or_default();
    UpbitClient::new(ak, sk)
}

/// Run the live scheduler forever. Wakes at every hour boundary, iterates all
/// `running` sessions, and executes one cycle per session.
#[cfg(feature = "tauri-app")]
pub async fn run_loop(
    app_handle: tauri::AppHandle,
    db: Arc<Mutex<Connection>>,
    cancel: Arc<AtomicBool>,
) {
    use tauri::Emitter;

    let registry = StrategyRegistry::new();
    let client = create_public_client();

    loop {
        if cancel.load(Ordering::Relaxed) { break; }
        let wait = seconds_until_next_hour();
        for _ in 0..wait {
            if cancel.load(Ordering::Relaxed) { return; }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // 1. Snapshot running sessions.
        let sessions = {
            let conn = match db.lock() {
                Ok(c) => c,
                Err(e) => { eprintln!("live-scheduler db lock: {e}"); continue; }
            };
            match live_repo::list_running_sessions(&conn) {
                Ok(v) => v,
                Err(e) => { eprintln!("list_running_sessions: {e}"); continue; }
            }
        };

        for session in sessions {
            // Load preset
            let preset = {
                let conn = db.lock().unwrap();
                match live_repo::get_preset(&conn, session.preset_id) {
                    Ok(Some(p)) => p,
                    _ => {
                        eprintln!("preset {} not found for session {}", session.preset_id, session.id);
                        continue;
                    }
                }
            };

            // Run the cycle.
            match session_engine::run_session_cycle(&db, &client, &session, &preset, &registry).await {
                Ok(out) => {
                    let _ = app_handle.emit("session:update", &out);
                }
                Err(e) => {
                    eprintln!("session {} cycle error: {e}", session.id);
                    let _ = app_handle.emit("session:log", serde_json::json!({
                        "session_id": session.id,
                        "level": "ERROR",
                        "message": format!("cycle error: {e}"),
                    }));
                }
            }
        }
    }
}
```

Modify `src-tauri/src/services/mod.rs`:

```rust
pub mod live_scheduler;
```

- [ ] **Step 3: `seconds_until_next_hour` 유닛 테스트 (스케줄러 파일 하단)**

`live_scheduler.rs` 끝에 추가:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seconds_until_next_hour_range() {
        let s = seconds_until_next_hour();
        assert!(s >= 10);
        assert!(s <= 7200);
    }
}
```

- [ ] **Step 4: 빌드 + 테스트**

Run: `cd src-tauri && cargo build --features tauri-app`
Expected: 경고만 있을 수 있음, 에러 없음.

Run: `cd src-tauri && cargo test --lib services::live_scheduler`
Expected: 1 PASS.

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/src/state.rs src-tauri/src/services/live_scheduler.rs src-tauri/src/services/mod.rs
git commit -m "feat(live): scheduler loop + AppState paper_session_ids cache"
```

---

## Task 8: Tauri 커맨드

**Files:**
- Create: `src-tauri/src/commands/live_trading.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 커맨드 파일 작성**

Create `src-tauri/src/commands/live_trading.rs`:

```rust
use crate::db::live_repo;
use crate::models::live::{LiveSession, LiveTrade, Preset};
use crate::state::AppState;
use chrono::Utc;
use serde::Deserialize;
use tauri::State;

// ─── Presets ───

#[derive(Deserialize)]
pub struct CreatePresetArgs {
    pub name: String,
    pub strategy_key: String,
    pub params_json: String,
    pub source: Option<String>,
    pub source_run_id: Option<i64>,
}

#[tauri::command]
pub fn create_preset(
    args: CreatePresetArgs,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::insert_preset(
        &conn,
        1,
        &args.name,
        &args.strategy_key,
        &args.params_json,
        args.source.as_deref().unwrap_or("manual"),
        args.source_run_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_presets(state: State<'_, AppState>) -> Result<Vec<Preset>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::list_presets(&conn, 1).map_err(|e| e.to_string())
}

/// Phase 1 편의 커맨드: `TradingParameters::default_for_market`을 직렬화해
/// 기본 프리셋을 한 번에 만들어준다. 프론트에서 프리셋 JSON을 손으로
/// 구성하지 않아도 되도록 하는 디버그/시드 경로.
#[tauri::command]
pub fn create_default_preset(
    name: String,
    strategy_key: String,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    use crate::models::trading::TradingParameters;
    let params = TradingParameters::default_for_market("ETH");
    let json = serde_json::to_string(&params).map_err(|e| e.to_string())?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::insert_preset(&conn, 1, &name, &strategy_key, &json, "manual", None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_preset(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::delete_preset(&conn, id).map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Sessions ───

#[derive(Deserialize)]
pub struct CreateSessionArgs {
    pub label: String,
    pub preset_id: i64,
    pub market: String,
    pub initial_capital: f64,
    pub start_offset_days: Option<u32>,
}

#[tauri::command]
pub fn create_session(
    args: CreateSessionArgs,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // preset 존재 확인
    let _preset = live_repo::get_preset(&conn, args.preset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("preset {} not found", args.preset_id))?;

    let start_ts = match args.start_offset_days {
        Some(days) => (Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339(),
        None => Utc::now().to_rfc3339(),
    };

    live_repo::insert_session(
        &conn,
        1,
        &args.label,
        args.preset_id,
        &args.market,
        "paper",  // Phase 1: 항상 paper로 생성
        args.initial_capital,
        &start_ts,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> Result<Vec<LiveSession>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::list_sessions(&conn, 1).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_session(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::set_session_status(&conn, id, "running").map_err(|e| e.to_string())?;
    drop(conn);
    state.paper_session_ids.lock().map_err(|e| e.to_string())?.insert(id, ());
    Ok(())
}

#[tauri::command]
pub fn stop_session(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::set_session_status(&conn, id, "stopped").map_err(|e| e.to_string())?;
    drop(conn);
    state.paper_session_ids.lock().map_err(|e| e.to_string())?.remove(&id);
    Ok(())
}

#[tauri::command]
pub fn delete_session(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::delete_session(&conn, id).map_err(|e| e.to_string())?;
    drop(conn);
    state.paper_session_ids.lock().map_err(|e| e.to_string())?.remove(&id);
    Ok(())
}

#[tauri::command]
pub fn list_session_trades(session_id: i64, state: State<'_, AppState>) -> Result<Vec<LiveTrade>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::list_trades(&conn, session_id).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: 커맨드 모듈 export**

Modify `src-tauri/src/commands/mod.rs` — 기존 선언 아래에 추가:

```rust
pub mod live_trading;
```

- [ ] **Step 3: `lib.rs`에서 AppState 초기화 + 커맨드 등록 + 스케줄러 실행**

Modify `src-tauri/src/lib.rs`:

(a) `use` 블록에 추가:

```rust
use crate::commands::{auth, data, simulation, optimization, trading, migration, notification, live_trading};
```

(b) `AppState { ... }` 두 곳 (데스크톱 + 서버) 모두에 `paper_session_ids: Mutex::new(std::collections::HashMap::new()),` 추가.

(c) market_updater 스레드 옆에 라이브 스케줄러 스레드 추가:

```rust
        // Live scheduler (separate DB connection + runtime).
        let scheduler_db = Arc::new(Mutex::new(
            schema::initialize(&db_path).expect("Failed to initialize scheduler database"),
        ));
        let scheduler_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        // AppHandle은 Builder setup에서만 얻을 수 있으므로 아래 Builder에서 setup 핸들러로 이동.
```

(d) `tauri::Builder::default()` 섹션에서 `.setup()` 추가:

```rust
        tauri::Builder::default()
            .manage(app_state)
            .setup(move |app| {
                let app_handle = app.handle().clone();
                let db_clone = scheduler_db.clone();
                let cancel_clone = scheduler_cancel.clone();
                std::thread::spawn(move || {
                    tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on(async move {
                            crate::services::live_scheduler::run_loop(app_handle, db_clone, cancel_clone).await;
                        });
                });
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                // ... 기존 커맨드들 ...
                live_trading::create_preset,
                live_trading::create_default_preset,
                live_trading::list_presets,
                live_trading::delete_preset,
                live_trading::create_session,
                live_trading::list_sessions,
                live_trading::start_session,
                live_trading::stop_session,
                live_trading::delete_session,
                live_trading::list_session_trades,
            ])
```

(`// ... 기존 커맨드들 ...` 자리에는 **삭제하지 말고** 기존 `data::load_csv_data` 등 모든 커맨드를 유지한 상태에서 `live_trading::*`를 추가한다.)

- [ ] **Step 4: 빌드**

Run: `cd src-tauri && cargo build --features tauri-app`
Expected: 경고만 있을 수 있음, 에러 없음.

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/src/commands/live_trading.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(live): tauri commands for sessions/presets + scheduler spawn"
```

---

## Task 9: 통합 테스트 — 전체 사이클

**Files:**
- Create: `src-tauri/tests/live_session_test.rs`

- [ ] **Step 1: 통합 테스트 작성**

이 테스트는 실제 Upbit API를 부르지 않는다. `run_session_cycle`은 API를 필요로 하므로, Phase 1의 통합 테스트는 **diff 삽입 멱등성**과 **리포지토리 orchestration**에 집중한다.

Create `src-tauri/tests/live_session_test.rs`:

```rust
//! Phase 1 integration: verify the DB-layer orchestration (presets, sessions,
//! trades, equity) composes correctly. Upbit API / strategy calls are covered
//! in unit tests; this file focuses on persistence boundary.

use bitcoin_trader_lib::db::live_repo;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    conn.execute_batch(include_str!("../migrations/001_initial.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/002_users.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/006_live_trading.sql")).unwrap();
    conn
}

#[test]
fn full_session_lifecycle() {
    let conn = setup_db();

    // 1) preset 생성
    let pid = live_repo::insert_preset(&conn, 1, "V3-OptA", "V3", "{}", "manual", None).unwrap();

    // 2) 세션 3개 생성
    let s1 = live_repo::insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();
    let s2 = live_repo::insert_session(&conn, 1, "S2", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();
    let s3 = live_repo::insert_session(&conn, 1, "S3", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();

    // 3) 각각 running으로 전환
    for sid in [s1, s2, s3] {
        live_repo::set_session_status(&conn, sid, "running").unwrap();
    }
    assert_eq!(live_repo::list_running_sessions(&conn).unwrap().len(), 3);

    // 4) 사이클 시뮬레이션 — 세션마다 다른 trade 삽입
    live_repo::insert_trade(&conn, s1, "2026-04-24T01:00:00Z", "buy",  3e6,   0.3, 450.0, "buy",  None, None, false).unwrap();
    live_repo::insert_trade(&conn, s1, "2026-04-24T02:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(3.0), Some(3.0), false).unwrap();

    live_repo::insert_trade(&conn, s2, "2026-04-24T03:00:00Z", "buy",  3.2e6, 0.3, 480.0, "buy",  None, None, false).unwrap();
    live_repo::insert_trade(&conn, s2, "2026-04-24T04:00:00Z", "sell", 3.25e6,0.3, 487.5, "sell", Some(1.5), Some(1.5), false).unwrap();

    // 5) 각 세션의 equity 스냅샷
    live_repo::upsert_equity(&conn, s1, "2026-04-24T02:00:00Z", 1_030_000.0, "idle").unwrap();
    live_repo::upsert_equity(&conn, s2, "2026-04-24T04:00:00Z", 1_015_000.0, "idle").unwrap();
    live_repo::upsert_equity(&conn, s3, "2026-04-24T01:00:00Z", 1_000_000.0, "idle").unwrap();

    // 6) 세션별 완료 수 검증
    assert_eq!(live_repo::count_completed_trades(&conn, s1).unwrap(), 1);
    assert_eq!(live_repo::count_completed_trades(&conn, s2).unwrap(), 1);
    assert_eq!(live_repo::count_completed_trades(&conn, s3).unwrap(), 0);

    // 7) 세션 1개 정지 → running 2개
    live_repo::set_session_status(&conn, s3, "stopped").unwrap();
    assert_eq!(live_repo::list_running_sessions(&conn).unwrap().len(), 2);

    // 8) 세션 1 삭제 → trades / equity CASCADE
    live_repo::delete_session(&conn, s1).unwrap();
    assert_eq!(live_repo::list_trades(&conn, s1).unwrap().len(), 0);
    assert_eq!(live_repo::list_equity(&conn, s1).unwrap().len(), 0);
    // s2는 유지
    assert_eq!(live_repo::list_trades(&conn, s2).unwrap().len(), 2);
}

#[test]
fn diff_insert_idempotency() {
    // 동일한 trade 시퀀스를 중복 삽입하면 중복되지만, count_completed_trades를 활용해
    // "이미 처리된 것까지는 건너뛰기"를 호출자가 보장해야 한다는 계약을 확인.
    let conn = setup_db();
    let pid = live_repo::insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
    let sid = live_repo::insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

    // 1차 사이클: trade 2개 완료
    let simulated_trades = vec![
        ("2026-04-24T01:00:00Z", "buy",  3e6,   0.3, 450.0, "buy",  None),
        ("2026-04-24T02:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(3.0)),
    ];

    let existing_before = live_repo::count_completed_trades(&conn, sid).unwrap();
    assert_eq!(existing_before, 0);

    // offset = trades.len() - new_count 계산으로 새 것만 삽입
    for (ts, side, price, vol, fee, sig, pnl) in &simulated_trades {
        live_repo::insert_trade(&conn, sid, ts, side, *price, *vol, *fee, sig, *pnl, *pnl, false).unwrap();
    }

    assert_eq!(live_repo::count_completed_trades(&conn, sid).unwrap(), 1);

    // 2차 사이클 재실행: 완료된 trade 수가 동일 → 삽입 skip
    let existing = live_repo::count_completed_trades(&conn, sid).unwrap();
    let new_to_insert = simulated_trades.len().saturating_sub(existing * 2); // buy+sell 쌍
    assert_eq!(new_to_insert, 0, "재실행 시 신규 없음");
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cd src-tauri && cargo test --test live_session_test`
Expected: 2 PASS.

- [ ] **Step 3: 전체 테스트도 통과 확인**

Run: `cd src-tauri && cargo test`
Expected: 기존 테스트도 모두 PASS, 신규 테스트 포함.

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/tests/live_session_test.rs
git commit -m "test(live): integration test for session lifecycle and diff"
```

---

## Task 10: 프론트 타입 + API 클라이언트

**Files:**
- Modify: `src/types/index.ts`
- Create: `src/lib/live.ts`

- [ ] **Step 1: 프론트 타입 추가**

Modify `src/types/index.ts` — 파일 끝에 추가:

```typescript
export interface Preset {
  id: number;
  user_id: number;
  name: string;
  strategy_key: string;
  params_json: string;
  source: string;
  source_run_id: number | null;
  created_at: string;
}

export interface LiveSession {
  id: number;
  user_id: number;
  label: string;
  preset_id: number;
  market: string;
  mode: "paper" | "real";
  status: "running" | "stopped";
  initial_capital: number;
  start_ts: string;
  real_started_at: string | null;
  last_cycle_ts: string | null;
  last_signal: string | null;
  current_position: "idle" | "holding";
  current_buy_price: number | null;
  current_buy_volume: number | null;
  current_equity: number | null;
  created_at: string;
}

export interface LiveTrade {
  id: number;
  session_id: number;
  ts: string;
  side: "buy" | "sell";
  price: number;
  volume: number;
  fee: number;
  signal: string;
  pnl: number | null;
  pnl_pct: number | null;
  is_real: boolean;
}

export interface CreatePresetArgs {
  name: string;
  strategy_key: string;
  params_json: string;
  source?: string;
  source_run_id?: number | null;
}

export interface CreateSessionArgs {
  label: string;
  preset_id: number;
  market: string;
  initial_capital: number;
  start_offset_days?: number;
}

export interface SessionCycleOutput {
  session_id: number;
  new_completed_trades: number;
  latest_signal: string;
  current_position: string;
  current_equity: number;
}
```

- [ ] **Step 2: API 클라이언트 작성**

기존 `src/lib/api.ts`의 Tauri/HTTP 분기 패턴을 확인하고, 같은 패턴으로 새 파일 생성. 먼저 api.ts의 구조를 확인.

Read `src/lib/api.ts` 상단 30줄을 보고 Tauri detection 패턴 파악. 본 플랜에서는 **동일 패턴을 따른다**.

Create `src/lib/live.ts`:

```typescript
import type {
  Preset,
  LiveSession,
  LiveTrade,
  CreatePresetArgs,
  CreateSessionArgs,
} from "../types";

const isTauri = "__TAURI__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke: tInvoke } = await import("@tauri-apps/api/core");
    return tInvoke<T>(cmd, args);
  }
  // PWA HTTP 경로 — Phase 1에서는 데스크톱 전용으로 구현.
  // Phase 2 이후 Axum 라우트를 추가하면 여기서 fetch로 분기.
  throw new Error(`${cmd} is desktop-only in Phase 1`);
}

export const listPresets = (): Promise<Preset[]> => invoke("list_presets");
export const createPreset = (args: CreatePresetArgs): Promise<number> =>
  invoke("create_preset", { args });
export const createDefaultPreset = (name: string, strategyKey: string): Promise<number> =>
  invoke("create_default_preset", { name, strategyKey });
export const deletePreset = (id: number): Promise<void> =>
  invoke("delete_preset", { id });

export const listSessions = (): Promise<LiveSession[]> => invoke("list_sessions");
export const createSession = (args: CreateSessionArgs): Promise<number> =>
  invoke("create_session", { args });
export const startSession = (id: number): Promise<void> =>
  invoke("start_session", { id });
export const stopSession = (id: number): Promise<void> =>
  invoke("stop_session", { id });
export const deleteSession = (id: number): Promise<void> =>
  invoke("delete_session", { id });
export const listSessionTrades = (sessionId: number): Promise<LiveTrade[]> =>
  invoke("list_session_trades", { sessionId });
```

- [ ] **Step 3: 타입 빌드 확인**

Run: `npm run vite:build`
Expected: `tsc`가 타입 오류 없이 통과.

- [ ] **Step 4: 커밋**

```bash
git add src/types/index.ts src/lib/live.ts
git commit -m "feat(live): frontend types + API client (desktop-only in Phase 1)"
```

---

## Task 11: Zustand 스토어

**Files:**
- Create: `src/stores/liveTradingStore.ts`

- [ ] **Step 1: 스토어 작성**

Create `src/stores/liveTradingStore.ts`:

```typescript
import { create } from "zustand";
import type { LiveSession, LiveTrade, Preset } from "../types";
import {
  listPresets,
  listSessions,
  listSessionTrades,
  createSession as apiCreateSession,
  startSession as apiStart,
  stopSession as apiStop,
  deleteSession as apiDelete,
  createPreset as apiCreatePreset,
} from "../lib/live";

type ListenFn = () => Promise<() => void>;

interface LiveTradingState {
  sessions: LiveSession[];
  presets: Preset[];
  tradesBySession: Record<number, LiveTrade[]>;
  loading: boolean;

  refreshAll: () => Promise<void>;
  refreshSessions: () => Promise<void>;
  refreshPresets: () => Promise<void>;
  refreshTrades: (sessionId: number) => Promise<void>;

  createSession: (args: Parameters<typeof apiCreateSession>[0]) => Promise<void>;
  createPreset: (args: Parameters<typeof apiCreatePreset>[0]) => Promise<void>;
  startSession: (id: number) => Promise<void>;
  stopSession: (id: number) => Promise<void>;
  deleteSession: (id: number) => Promise<void>;

  subscribeEvents: () => Promise<() => void>;
}

export const useLiveTradingStore = create<LiveTradingState>((set, get) => ({
  sessions: [],
  presets: [],
  tradesBySession: {},
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

  createPreset: async (args) => {
    await apiCreatePreset(args);
    await get().refreshPresets();
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

  subscribeEvents: async () => {
    if (!("__TAURI__" in window)) return () => {};
    const { listen } = await import("@tauri-apps/api/event");

    const u1 = await listen("session:update", () => {
      get().refreshSessions();
    });
    const u2 = await listen<{ session_id: number }>("session:log", (e) => {
      // Phase 2에서 로그 패널 연동. 일단 콘솔에만.
      console.debug("session:log", e.payload);
    });

    return () => { u1(); u2(); };
  },
}));
```

- [ ] **Step 2: 타입 빌드**

Run: `npm run vite:build`
Expected: 통과.

- [ ] **Step 3: 커밋**

```bash
git add src/stores/liveTradingStore.ts
git commit -m "feat(live): zustand store for sessions/presets"
```

---

## Task 12: 최소 UI — 세션 테이블 + 생성 다이얼로그

**Files:**
- Create: `src/components/live/SessionTable.tsx`
- Create: `src/components/live/NewSessionDialog.tsx`
- Modify: `src/pages/LiveTradingPage.tsx` (최소 버전으로 대체)

- [ ] **Step 1: SessionTable 작성**

Create `src/components/live/SessionTable.tsx`:

```tsx
import { Button } from "../ui/Button";
import { Badge } from "../ui/Badge";
import type { LiveSession } from "../../types";

interface Props {
  sessions: LiveSession[];
  onStart: (id: number) => void;
  onStop: (id: number) => void;
  onDelete: (id: number) => void;
}

export default function SessionTable({ sessions, onStart, onStop, onDelete }: Props) {
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
          <th className="text-right">Equity</th>
          <th className="text-right">P/L %</th>
          <th className="text-left">Signal</th>
          <th className="text-right">Last Cycle</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {sessions.map((s) => {
          const pnlPct = s.current_equity != null
            ? ((s.current_equity / s.initial_capital - 1) * 100)
            : 0;
          const pnlColor = pnlPct > 0 ? "text-emerald-400" : pnlPct < 0 ? "text-rose-400" : "text-zinc-400";
          return (
            <tr key={s.id} className="border-b border-zinc-900 hover:bg-zinc-900/40">
              <td className="py-2 font-medium text-zinc-200">{s.label}</td>
              <td className="text-zinc-400">{s.market}</td>
              <td>
                <Badge variant={s.status === "running" ? "green" : "gray"}>
                  {s.status}
                </Badge>
              </td>
              <td className="text-right font-data text-zinc-200">
                {s.current_equity != null ? s.current_equity.toLocaleString() : "--"}
              </td>
              <td className={`text-right font-data ${pnlColor}`}>
                {pnlPct.toFixed(2)}%
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

- [ ] **Step 2: NewSessionDialog 작성**

Create `src/components/live/NewSessionDialog.tsx`:

```tsx
import { useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import type { Preset } from "../../types";

interface Props {
  presets: Preset[];
  onClose: () => void;
  onSubmit: (args: {
    label: string;
    preset_id: number;
    market: string;
    initial_capital: number;
    start_offset_days?: number;
  }) => void;
}

export default function NewSessionDialog({ presets, onClose, onSubmit }: Props) {
  const [label, setLabel] = useState("");
  const [presetId, setPresetId] = useState<number | null>(null);
  const [capital, setCapital] = useState(1_000_000);
  const [offsetDays, setOffsetDays] = useState(0);

  useEffect(() => {
    if (presets.length > 0 && presetId == null) setPresetId(presets[0].id);
  }, [presets, presetId]);

  const canSubmit = label.trim().length > 0 && presetId != null && capital > 0;

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 w-96 space-y-4">
        <h3 className="text-lg font-semibold text-zinc-100">New Paper Session</h3>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Label</label>
          <Input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="V3-OptA" />
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Market</label>
          <p className="text-sm text-zinc-300">KRW-ETH <span className="text-zinc-600">(fixed)</span></p>
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Preset</label>
          <select
            value={presetId ?? ""}
            onChange={(e) => setPresetId(Number(e.target.value))}
            className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-200"
          >
            {presets.length === 0 ? (
              <option value="">No presets available — create one first</option>
            ) : (
              presets.map((p) => (
                <option key={p.id} value={p.id}>{p.strategy_key}: {p.name}</option>
              ))
            )}
          </select>
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Initial Capital (KRW)</label>
          <Input type="number" value={capital} onChange={(e) => setCapital(Number(e.target.value))} />
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Start From</label>
          <select
            value={offsetDays}
            onChange={(e) => setOffsetDays(Number(e.target.value))}
            className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-200"
          >
            <option value={0}>Now</option>
            <option value={1}>1 day ago</option>
            <option value={7}>7 days ago</option>
            <option value={30}>30 days ago</option>
          </select>
        </div>

        <div className="flex justify-end gap-2 pt-2">
          <Button variant="secondary" onClick={onClose}>Cancel</Button>
          <Button
            disabled={!canSubmit}
            onClick={() => {
              onSubmit({
                label: label.trim(),
                preset_id: presetId!,
                market: "KRW-ETH",
                initial_capital: capital,
                start_offset_days: offsetDays > 0 ? offsetDays : undefined,
              });
            }}
          >
            Create
          </Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: LiveTradingPage 대체**

**WARN:** 기존 `LiveTradingPage.tsx`는 Phase 2~3에서 점진 복구할 기능(차트, 수동 매매, 모니터링)을 잃는다. 이는 설계서 Section 9 의 "Phase 4에서 레거시 제거" 계획에 맞춘 조치다.

Replace `src/pages/LiveTradingPage.tsx` entirely:

```tsx
import { useEffect, useState } from "react";
import { Button } from "../components/ui/Button";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Plus } from "lucide-react";
import SessionTable from "../components/live/SessionTable";
import NewSessionDialog from "../components/live/NewSessionDialog";
import { useLiveTradingStore } from "../stores/liveTradingStore";

export default function LiveTradingPage() {
  const {
    sessions, presets,
    refreshAll, createSession,
    startSession, stopSession, deleteSession,
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
      <Card>
        <CardHeader className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-zinc-300">Live Trading — Paper Sessions</h3>
          <Button size="sm" onClick={() => setShowNew(true)}>
            <Plus size={14} /> New Session
          </Button>
        </CardHeader>
        <CardContent>
          <SessionTable
            sessions={sessions}
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

      {presets.length === 0 && (
        <p className="text-xs text-zinc-500 px-2">
          No presets yet. Phase 2/3에서 Optimization 페이지와 연동됩니다.
          당장 테스트하려면 개발자 콘솔에서{" "}
          <code className="text-zinc-400">createPreset</code>을 호출하세요.
        </p>
      )}

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
Expected: 타입 + Vite 빌드 모두 통과.

- [ ] **Step 5: 커밋**

```bash
git add src/components/live/SessionTable.tsx src/components/live/NewSessionDialog.tsx src/pages/LiveTradingPage.tsx
git commit -m "feat(live): minimal Phase 1 UI — session table + create dialog"
```

---

## Task 13: 수동 smoke test (통합 확인)

**Files:** (변경 없음 — 수동 검증)

- [ ] **Step 1: 개발 서버 실행 (사용자)**

**중요:** 본 프로젝트 규칙상 Claude는 서버를 직접 시작하지 않는다. 사용자에게 아래 명령 실행을 요청:

```bash
# 한 터미널에서
cargo build --features tauri-app --manifest-path src-tauri/Cargo.toml

# 다른 터미널
npm run dev
```

- [ ] **Step 2: 프리셋 시드 (사용자 개발 콘솔에서)**

앱이 로드되면 브라우저 DevTools 콘솔에서 `create_default_preset`을 호출해 `TradingParameters::default_for_market("ETH")`가 직렬화된 프리셋을 한 번에 생성한다:

```javascript
const { invoke } = await import("@tauri-apps/api/core");
await invoke("create_default_preset", { name: "V3-default", strategyKey: "V3" });
await invoke("list_presets");
```

**Expected:** `list_presets` 결과에 `V3-default` 프리셋 1개가 나오고, `params_json`은 모든 V3/V5/V3.1 필드가 채워진 긴 JSON 문자열.

Phase 2 이후 Optimization 결과를 프리셋으로 export하는 플로우가 추가되면 이 편의 커맨드는 UI 버튼으로 대체된다.

- [ ] **Step 3: 세션 생성 + 시작 → 관찰**

UI에서:
1. "New Session" 클릭 → label `T1` + preset 선택 + 기본 자본 → Create
2. 테이블에 세션이 나타남 (status: stopped)
3. "Start" 클릭 → status: running
4. 다음 정시 경계 + 수 초 내에 `session:update` 이벤트로 equity/signal이 갱신돼야 함.
   - 정시까지 기다리기 어렵다면 단기 검증으로 DB 직접 확인: `sqlite3 %LOCALAPPDATA%\bitcoin-trader\bitcoin_trader.db "SELECT id, status, last_cycle_ts, last_signal, current_equity FROM live_sessions;"`

- [ ] **Step 4: 정지 + 삭제**

1. "Stop" → status: stopped
2. "Del" → 확인 후 테이블에서 사라짐
3. DB 확인: `SELECT * FROM live_trades WHERE session_id = <id>;` → 0 rows (CASCADE)

- [ ] **Step 5: 최종 커밋 (변경 없으면 skip)**

수동 테스트 중 발견된 버그 수정이 있으면 그 파일들을 커밋. 없으면 이 단계는 skip.

---

## Definition of Done (Phase 1)

- [ ] `cargo build --features tauri-app` 경고만 있고 에러 없음
- [ ] `cargo test` 전체 통과 (기존 + 신규 테스트)
- [ ] `npm run vite:build` 통과
- [ ] 수동 smoke test (Task 13) 통과
- [ ] 사용자가 UI에서 페이퍼 세션 여러 개를 생성·시작·정지·삭제할 수 있음
- [ ] 1시간 경계마다 세션별 equity/signal이 DB에 갱신됨
- [ ] 세션 삭제 시 trades/equity 모두 CASCADE로 제거

## Phase 2 Preview (본 플랜 범위 밖)

다음 플랜 `2026-04-??-live-trading-phase2.md`에서:
- `services/tick_broker.rs` (tokio-tungstenite + Upbit WS)
- `market:tick` Tauri 이벤트 + Axum SSE 라우트
- `liveTradingStore`에 틱 구독 + 미실현 P/L 합성
- 세션 테이블에 실시간 P/L 컬럼 표시
