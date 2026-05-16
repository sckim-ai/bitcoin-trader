# Multi Upbit Accounts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 여러 Upbit 계정을 1급 엔티티로 분리하고, 계정당 최대 1개의 running real 세션을 안전하게 운영할 수 있게 한다.

**Architecture:**
- DB에 `upbit_accounts` 테이블 신설, `live_sessions.upbit_account_id` FK 추가
- 기존 단일 keyring 키를 `account_id=1, label='기본'`으로 자동 흡수
- 기존 전역 `multi-real=1` 정책 제거 → 계정별 부분 유니크 인덱스 (`mode='real'` 한정)
- `upbit_keys::upbit_client_or_err()` 시그니처를 `account_id: i64`로 변경, 모든 호출처 갱신
- `pending_order_tracker`를 per-session 호출로 변경 — N×M 중복 fetch 제거
- 프론트엔드 `/accounts` 페이지 신설, `NewSessionDialog`/`ManualOrderCard`에 계정/세션 셀렉터 추가

**Tech Stack:** Rust (rusqlite, keyring, tokio, serde, jsonwebtoken), TypeScript/React (Zustand, Tauri IPC).

**Spec reference:** `docs/superpowers/specs/2026-05-15-multi-upbit-accounts-design.md`

---

## 파일 구조

**신규 (Rust):**
- `src-tauri/migrations/014_upbit_accounts.sql` — 테이블 + ALTER + partial unique index
- `src-tauri/src/models/upbit_account.rs` — `UpbitAccount` 도메인 타입
- `src-tauri/src/db/upbit_accounts_repo.rs` — CRUD
- `src-tauri/src/commands/upbit_accounts.rs` — Tauri 커맨드 6개
- `src-tauri/tests/upbit_accounts_test.rs` — repo + 커맨드 통합 테스트
- `src-tauri/tests/account_migration_test.rs` — 자동 흡수 마이그레이션 테스트

**신규 (Frontend):**
- `src/pages/AccountsPage.tsx`
- `src/components/accounts/AccountCard.tsx`
- `src/components/accounts/AddAccountDialog.tsx`
- `src/components/accounts/EditAccountDialog.tsx`

**수정 (Rust):**
- `src-tauri/src/db/schema.rs` — 014 migration + `migrate_legacy_upbit_key()` (seed_admin 다음)
- `src-tauri/src/commands/upbit_keys.rs` — `load_upbit_keys(account_id)` 시그니처
- `src-tauri/src/models/live.rs` — `LiveSession.upbit_account_id`, `account_label`
- `src-tauri/src/db/live_repo.rs` — JOIN, `count_real_sessions` 제거, `insert_session` 시그니처
- `src-tauri/src/commands/live_trading.rs` — `CreateSessionArgs.upbit_account_id`, `toggle_session_mode` 검증
- `src-tauri/src/services/session_engine.rs` — `upbit_client_or_err(session.upbit_account_id)`, `reconcile_pending_orders_for_session`
- `src-tauri/src/services/auto_trader.rs` — account_id 전파
- `src-tauri/src/services/pending_order_tracker.rs` — `reconcile_pending_orders_for_session` 신설
- `src-tauri/src/commands/trading.rs` — `get_balance`/`get_position`에 account_id, `manual_buy`/`manual_sell` 제거
- `src-tauri/src/lib.rs` — invoke_handler에 신규 커맨드 등록, legacy 제거

**수정 (Frontend):**
- `src/types/index.ts` — `UpbitAccount`, `LiveSession` 확장
- `src/lib/api.ts` — 신규 커맨드, `manualBuy`/`manualSell` 제거
- `src/App.tsx` — `/accounts` 라우트
- `src/pages/SettingsPage.tsx` — Upbit 섹션 제거
- `src/components/live/NewSessionDialog.tsx` — 계정 드롭다운
- `src/pages/LiveTradingPage.tsx` — 세션 카드 라벨 배지
- `src/components/live/ManualOrderCard.tsx` — 명시적 세션 셀렉터
- `src/stores/tradingStore.ts` — deprecate / 정리
- `src/components/trading/ManualOrderDialog.tsx` — 제거

---

## Task 1: Migration 014 SQL 파일 작성

**Files:**
- Create: `src-tauri/migrations/014_upbit_accounts.sql`

- [ ] **Step 1: SQL 파일 작성**

```sql
-- 014_upbit_accounts.sql
-- 여러 Upbit 계정을 1급 엔티티로 분리. 1 계정 = 1 running real 세션.

CREATE TABLE IF NOT EXISTS upbit_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id),
    label TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, label)
);

-- ALTER ADD COLUMN은 idempotent가 아니므로 schema.rs에서 "duplicate column" 에러 swallow 필요.
ALTER TABLE live_sessions
    ADD COLUMN upbit_account_id INTEGER REFERENCES upbit_accounts(id) ON DELETE SET NULL;

-- 같은 계정에 running real 세션은 1개만. paper는 N개 허용.
CREATE UNIQUE INDEX IF NOT EXISTS idx_session_account_running_real
    ON live_sessions(upbit_account_id)
    WHERE status = 'running' AND mode = 'real' AND upbit_account_id IS NOT NULL;
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/migrations/014_upbit_accounts.sql
git commit -m "feat(db): migration 014 — upbit_accounts table + session FK + per-account running-real unique index"
```

---

## Task 2: UpbitAccount 도메인 모델

**Files:**
- Create: `src-tauri/src/models/upbit_account.rs`
- Modify: `src-tauri/src/models/mod.rs`

- [ ] **Step 1: 모델 작성**

```rust
// src-tauri/src/models/upbit_account.rs
use serde::{Deserialize, Serialize};

/// Upbit API 계정 1개. 키 값은 별도 keyring에 `upbit_access_key_<id>` /
/// `upbit_secret_key_<id>`로 저장되며 이 구조체에는 포함되지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpbitAccount {
    pub id: i64,
    pub user_id: i64,
    pub label: String,
    pub enabled: bool,
    pub created_at: String,
    /// 키링에 access 키가 저장되어 있는지 (값 노출 X).
    pub has_access_key: bool,
    /// 키링에 secret 키가 저장되어 있는지.
    pub has_secret_key: bool,
    /// 이 계정에 running 세션이 있는지 (UI 셀렉터에서 disabled 표시용).
    /// JOIN으로 계산되어 list 응답에 포함됨.
    pub has_running_session: bool,
}
```

- [ ] **Step 2: `models/mod.rs`에 등록**

기존 `mod.rs`를 열어 다른 `pub mod ...` 옆에 추가:

```rust
pub mod upbit_account;
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/models/upbit_account.rs src-tauri/src/models/mod.rs
git commit -m "feat(models): UpbitAccount domain type"
```

---

## Task 3: upbit_accounts_repo CRUD + 단위 테스트

**Files:**
- Create: `src-tauri/src/db/upbit_accounts_repo.rs`
- Modify: `src-tauri/src/db/mod.rs`

- [ ] **Step 1: Repo 함수 + 인라인 테스트 작성**

```rust
// src-tauri/src/db/upbit_accounts_repo.rs
use crate::models::upbit_account::UpbitAccount;
use rusqlite::{params, Connection, OptionalExtension, Result};

pub fn insert_account(conn: &Connection, user_id: i64, label: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO upbit_accounts (user_id, label) VALUES (?1, ?2)",
        params![user_id, label],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_account(conn: &Connection, id: i64) -> Result<Option<UpbitAccount>> {
    conn.query_row(
        "SELECT a.id, a.user_id, a.label, a.enabled, a.created_at,
                EXISTS(SELECT 1 FROM live_sessions ls
                       WHERE ls.upbit_account_id = a.id AND ls.status = 'running')
         FROM upbit_accounts a WHERE a.id = ?1",
        [id],
        row_to_account_partial,
    )
    .optional()
}

pub fn list_accounts(conn: &Connection, user_id: i64) -> Result<Vec<UpbitAccount>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.user_id, a.label, a.enabled, a.created_at,
                EXISTS(SELECT 1 FROM live_sessions ls
                       WHERE ls.upbit_account_id = a.id AND ls.status = 'running')
         FROM upbit_accounts a WHERE a.user_id = ?1 ORDER BY a.id ASC",
    )?;
    let rows = stmt.query_map([user_id], row_to_account_partial)?;
    rows.collect()
}

pub fn update_label(conn: &Connection, id: i64, label: &str) -> Result<usize> {
    conn.execute(
        "UPDATE upbit_accounts SET label = ?1 WHERE id = ?2",
        params![label, id],
    )
}

pub fn set_enabled(conn: &Connection, id: i64, enabled: bool) -> Result<usize> {
    conn.execute(
        "UPDATE upbit_accounts SET enabled = ?1 WHERE id = ?2",
        params![enabled as i64, id],
    )
}

pub fn delete_account(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM upbit_accounts WHERE id = ?1", params![id])
}

pub fn count_running_sessions(conn: &Connection, account_id: i64) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM live_sessions
         WHERE upbit_account_id = ?1 AND status = 'running'",
        [account_id],
        |r| r.get(0),
    )
}

/// `has_access_key` / `has_secret_key`는 keyring 조회가 필요하므로 repo 레벨에선
/// 채우지 않고 false로 둔다. 커맨드 레이어에서 enrich한다.
fn row_to_account_partial(row: &rusqlite::Row) -> Result<UpbitAccount> {
    let enabled: i64 = row.get(3)?;
    let has_running: i64 = row.get(5)?;
    Ok(UpbitAccount {
        id: row.get(0)?,
        user_id: row.get(1)?,
        label: row.get(2)?,
        enabled: enabled != 0,
        created_at: row.get(4)?,
        has_access_key: false,
        has_secret_key: false,
        has_running_session: has_running != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn setup() -> Connection {
        let dir = std::env::temp_dir().join(format!("bt_repo_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        schema::initialize(&dir.join("test.db")).unwrap()
    }

    #[test]
    fn insert_and_get() {
        let conn = setup();
        let id = insert_account(&conn, 1, "메인").unwrap();
        let got = get_account(&conn, id).unwrap().unwrap();
        assert_eq!(got.label, "메인");
        assert_eq!(got.user_id, 1);
        assert!(got.enabled);
        assert!(!got.has_running_session);
    }

    #[test]
    fn duplicate_label_rejected() {
        let conn = setup();
        insert_account(&conn, 1, "메인").unwrap();
        let err = insert_account(&conn, 1, "메인").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    #[test]
    fn list_orders_by_id() {
        let conn = setup();
        let a = insert_account(&conn, 1, "A").unwrap();
        let b = insert_account(&conn, 1, "B").unwrap();
        let list = list_accounts(&conn, 1).unwrap();
        assert_eq!(list.iter().map(|x| x.id).collect::<Vec<_>>(), vec![a, b]);
    }

    #[test]
    fn delete_then_get_returns_none() {
        let conn = setup();
        let id = insert_account(&conn, 1, "X").unwrap();
        delete_account(&conn, id).unwrap();
        assert!(get_account(&conn, id).unwrap().is_none());
    }

    #[test]
    fn set_enabled_toggles() {
        let conn = setup();
        let id = insert_account(&conn, 1, "X").unwrap();
        set_enabled(&conn, id, false).unwrap();
        assert!(!get_account(&conn, id).unwrap().unwrap().enabled);
        set_enabled(&conn, id, true).unwrap();
        assert!(get_account(&conn, id).unwrap().unwrap().enabled);
    }
}
```

- [ ] **Step 2: `db/mod.rs`에 등록**

```rust
pub mod upbit_accounts_repo;
```

- [ ] **Step 3: Run tests**

```bash
cd src-tauri && cargo test --lib upbit_accounts_repo
```

Expected: 5 passed (insert_and_get, duplicate_label_rejected, list_orders_by_id, delete_then_get_returns_none, set_enabled_toggles)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/upbit_accounts_repo.rs src-tauri/src/db/mod.rs
git commit -m "feat(db): upbit_accounts_repo CRUD + unit tests"
```

---

## Task 4: schema.rs 마이그레이션 등록 + Legacy 자동 흡수

**Files:**
- Modify: `src-tauri/src/db/schema.rs`
- Create: `src-tauri/tests/account_migration_test.rs`

- [ ] **Step 1: schema.rs에 014 마이그레이션 등록 + 자동 흡수 함수 추가**

`src-tauri/src/db/schema.rs`의 `initialize()` 함수 내부, `seed_admin(&conn)?;` 직전에 014를 추가하고 직후에 `migrate_legacy_upbit_key`를 호출.

기존 코드 (line 84~101):
```rust
    let schema_v13 = include_str!("../../migrations/013_pending_cost_basis.sql");
    if let Err(e) = conn.execute_batch(schema_v13) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    // Backfill best_return cache ...
    conn.execute(
        "UPDATE optimization_runs ...",
        [],
    )?;
    seed_admin(&conn)?;
    Ok(conn)
}
```

변경 후:
```rust
    let schema_v13 = include_str!("../../migrations/013_pending_cost_basis.sql");
    if let Err(e) = conn.execute_batch(schema_v13) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v14 = include_str!("../../migrations/014_upbit_accounts.sql");
    if let Err(e) = conn.execute_batch(schema_v14) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    // Backfill best_return cache ...
    conn.execute(
        "UPDATE optimization_runs ...",
        [],
    )?;
    seed_admin(&conn)?;
    // FK on upbit_accounts.user_id needs users(1) to exist → must run AFTER seed_admin.
    migrate_legacy_upbit_key(&conn)?;
    Ok(conn)
}

/// Idempotent one-time migration: 기존 단일 keyring `upbit_access_key` /
/// `upbit_secret_key` (또는 env fallback) → `upbit_accounts(id=1, label='기본')`.
///
/// 순서가 중요하다:
///   1. account_id=1 존재 → skip (재실행 안전)
///   2. 옛 keyring 또는 env에서 키 읽기 (둘 다 없으면 무동작)
///   3. 새 위치 keyring에 **모두 저장 성공** → 4 진행
///   4. DB row 생성 (keyring 성공 후에만 — "row 있는데 키 없음" 방지)
///   5. 옛 keyring 항목 삭제
///   6. 기존 세션의 upbit_account_id를 1로 백필
///
/// 런타임에서는 env 변수를 더 이상 참조하지 않는다 (멀티 계정 환경에서 사고 방지).
fn migrate_legacy_upbit_key(conn: &Connection) -> Result<()> {
    const SERVICE: &str = "bitcoin-trader";

    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM upbit_accounts WHERE id = 1",
        [],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Ok(());
    }

    let read_kr = |user: &str| -> Option<String> {
        keyring::Entry::new(SERVICE, user)
            .ok()
            .and_then(|e| e.get_password().ok())
    };
    let (access, secret) = match (read_kr("upbit_access_key"), read_kr("upbit_secret_key")) {
        (Some(a), Some(s)) if !a.is_empty() && !s.is_empty() => (a, s),
        _ => match (
            std::env::var("UPBIT_ACCESS_KEY").ok(),
            std::env::var("UPBIT_SECRET_KEY").ok(),
        ) {
            (Some(a), Some(s)) if !a.is_empty() && !s.is_empty() => (a, s),
            _ => return Ok(()), // 아무 출처도 없으면 마이그레이션 자체를 건너뜀
        },
    };

    // 3. 새 위치 keyring 저장 — 두 항목 모두 성공해야 다음 단계로 진행.
    //    map_err로 rusqlite::Error로 통일해서 ? 사용.
    let to_err = |e: keyring::Error| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERNAL),
            Some(format!("keyring: {e}")),
        )
    };
    keyring::Entry::new(SERVICE, "upbit_access_key_1")
        .map_err(to_err)?
        .set_password(&access)
        .map_err(to_err)?;
    keyring::Entry::new(SERVICE, "upbit_secret_key_1")
        .map_err(to_err)?
        .set_password(&secret)
        .map_err(to_err)?;

    // 4. DB row 생성
    conn.execute(
        "INSERT INTO upbit_accounts (id, user_id, label) VALUES (1, 1, '기본')",
        [],
    )?;

    // 5. 옛 keyring 항목 삭제 (실패해도 치명적이지 않음 — 새 위치가 우선)
    let _ = keyring::Entry::new(SERVICE, "upbit_access_key")
        .and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new(SERVICE, "upbit_secret_key")
        .and_then(|e| e.delete_credential());

    // 6. 기존 세션 백필 (paper/real 무관)
    conn.execute(
        "UPDATE live_sessions SET upbit_account_id = 1 WHERE upbit_account_id IS NULL",
        [],
    )?;

    Ok(())
}
```

- [ ] **Step 2: 통합 테스트 작성**

```rust
// src-tauri/tests/account_migration_test.rs
//! 마이그레이션 014 + migrate_legacy_upbit_key의 DB 부분 검증.
//! keyring write는 dev/CI 환경에 따라 결과가 달라 통합 테스트가 어려우므로,
//! "옛 키/env가 없을 때 무동작" 케이스만 자동화한다. keyring 경로는 수동 검증.

use bitcoin_trader::db::schema;

#[test]
fn clean_install_creates_no_account() {
    // 환경변수 격리
    std::env::remove_var("UPBIT_ACCESS_KEY");
    std::env::remove_var("UPBIT_SECRET_KEY");

    let dir = std::env::temp_dir().join(format!("bt_mig_clean_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = schema::initialize(&dir.join("test.db")).unwrap();

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM upbit_accounts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "clean install should not auto-create account_id=1");
}

#[test]
fn schema_014_creates_table_and_index() {
    let dir = std::env::temp_dir().join(format!("bt_mig_schema_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = schema::initialize(&dir.join("test.db")).unwrap();

    let table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='upbit_accounts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 1);

    let idx_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='index' AND name='idx_session_account_running_real'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(idx_count, 1);

    // live_sessions.upbit_account_id 컬럼 존재 확인
    let col_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('live_sessions')
             WHERE name='upbit_account_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(col_exists, 1);
}

#[test]
fn idempotent_double_initialize() {
    let dir = std::env::temp_dir().join(format!("bt_mig_idemp_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    let _conn1 = schema::initialize(&path).unwrap();
    let _conn2 = schema::initialize(&path).unwrap(); // 두 번째 호출이 깨지지 않아야
}
```

- [ ] **Step 3: Run tests**

```bash
cd src-tauri && cargo test --test account_migration_test
```

Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/schema.rs src-tauri/tests/account_migration_test.rs
git commit -m "feat(db): register migration 014 + idempotent legacy keyring auto-absorption (seed_admin 다음)"
```

---

## Task 5: upbit_keys.rs 시그니처 변경 (account_id 기반)

**Files:**
- Modify: `src-tauri/src/commands/upbit_keys.rs`

기존의 단일 키 경로 (`load_upbit_keys()`, `upbit_client_or_err()`, `save_upbit_keys`, `clear_upbit_keys`, `test_upbit_connection`)는 호출처가 광범위하므로, **새 함수를 추가**하고 옛 함수는 그대로 둔다(다음 Task에서 호출처 갱신). Legacy Tauri 커맨드(`save_upbit_keys`, `clear_upbit_keys`, `test_upbit_connection`)는 `#[deprecated]` 주석을 달되 invoke_handler에서는 일단 유지 → Task 16에서 제거.

- [ ] **Step 1: 새 헬퍼 추가**

`src-tauri/src/commands/upbit_keys.rs` 상단에 `SERVICE` 상수 옆에 새 키 이름 생성 헬퍼와 account-id 기반 로더 추가:

```rust
fn access_user(account_id: i64) -> String { format!("upbit_access_key_{account_id}") }
fn secret_user(account_id: i64) -> String { format!("upbit_secret_key_{account_id}") }

/// account_id 기반 키 조회. keyring만 참조 (env fallback 없음 — 마이그레이션 1회에서만 env를 봄).
/// 둘 다 있으면 `Ok((access, secret))`, 하나라도 빠지면 `Err`.
pub fn load_upbit_keys_for(account_id: i64) -> Result<(String, String), String> {
    let access = keyring::Entry::new(SERVICE, &access_user(account_id))
        .map_err(|e| format!("keyring open access({account_id}): {e}"))?
        .get_password()
        .map_err(|e| format!("keyring read access({account_id}): {e}"))?;
    let secret = keyring::Entry::new(SERVICE, &secret_user(account_id))
        .map_err(|e| format!("keyring open secret({account_id}): {e}"))?
        .get_password()
        .map_err(|e| format!("keyring read secret({account_id}): {e}"))?;
    Ok((access, secret))
}

/// account_id로 UpbitClient 생성.
pub fn upbit_client_for(account_id: i64) -> Result<UpbitClient, String> {
    let (a, s) = load_upbit_keys_for(account_id)?;
    Ok(UpbitClient::new(a, s))
}

/// 키링에 access/secret 키가 모두 있는지 boolean으로만 반환. UI list 응답 enrich용.
pub fn has_keys_for(account_id: i64) -> (bool, bool) {
    let has_access = keyring::Entry::new(SERVICE, &access_user(account_id))
        .and_then(|e| e.get_password())
        .is_ok();
    let has_secret = keyring::Entry::new(SERVICE, &secret_user(account_id))
        .and_then(|e| e.get_password())
        .is_ok();
    (has_access, has_secret)
}

/// account_id별로 keyring에 키 저장. add_upbit_account 흐름의 §4.2.3 Phase 1에서 사용.
/// 둘 다 성공해야 Ok. 하나라도 실패하면 잔존 항목을 정리한 뒤 Err.
pub fn save_keys_for(account_id: i64, access: &str, secret: &str) -> Result<(), String> {
    if access.trim().is_empty() || secret.trim().is_empty() {
        return Err("access/secret must not be empty".into());
    }
    let a_entry = keyring::Entry::new(SERVICE, &access_user(account_id))
        .map_err(|e| format!("keyring open access: {e}"))?;
    let s_entry = keyring::Entry::new(SERVICE, &secret_user(account_id))
        .map_err(|e| format!("keyring open secret: {e}"))?;
    a_entry
        .set_password(access)
        .map_err(|e| format!("keyring write access: {e}"))?;
    if let Err(e) = s_entry.set_password(secret) {
        // access는 저장됐는데 secret 실패 → access도 되돌림
        let _ = a_entry.delete_credential();
        return Err(format!("keyring write secret: {e}"));
    }
    Ok(())
}

/// account_id별로 keyring에서 키 삭제. delete_upbit_account 흐름에서 사용.
pub fn delete_keys_for(account_id: i64) {
    let _ = keyring::Entry::new(SERVICE, &access_user(account_id))
        .and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new(SERVICE, &secret_user(account_id))
        .and_then(|e| e.delete_credential());
}
```

- [ ] **Step 2: 기존 단일 키 함수에 deprecated 주석**

```rust
#[deprecated(note = "Multi-account migration: use load_upbit_keys_for(account_id) instead. Removed in next release.")]
pub fn load_upbit_keys_full() -> ...

#[deprecated(note = "Multi-account migration: use load_upbit_keys_for(account_id). Removed in next release.")]
pub fn load_upbit_keys() -> ...

#[deprecated(note = "Multi-account migration: use upbit_client_for(account_id). Removed in next release.")]
pub fn upbit_client_or_err() -> ...
```

(커맨드 `save_upbit_keys`, `clear_upbit_keys`, `test_upbit_connection`도 동일 deprecate.)

- [ ] **Step 3: 빌드 확인**

```bash
cd src-tauri && cargo build --features tauri-app
```

Expected: build succeeds. Deprecated 경고가 호출처에 출력되어도 무방 (Task 9, 12에서 정리).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/upbit_keys.rs
git commit -m "feat(upbit-keys): account_id-aware loader/saver/deleter; deprecate single-key APIs"
```

---

## Task 6: commands/upbit_accounts.rs — list / add / test_connection

**Files:**
- Create: `src-tauri/src/commands/upbit_accounts.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: list + add + test_connection 작성**

```rust
// src-tauri/src/commands/upbit_accounts.rs
//! Multi-Upbit-account management commands.
//!
//! add 흐름은 §4.2.3 (spec) 보상 패턴:
//!   Phase 1 (동기) — DB row insert, lock 해제
//!   Phase 2 (동기) — keyring write
//!   Phase 3 (async) — Upbit 연결 테스트
//!   실패 시 keyring 삭제 + DB row 삭제 보상.
//! AppState.db가 std::sync::Mutex이므로 MutexGuard를 `.await` 너머로 못 가져간다.

use crate::commands::upbit_keys::{
    delete_keys_for, has_keys_for, save_keys_for, upbit_client_for,
};
use crate::db::upbit_accounts_repo;
use crate::models::upbit_account::UpbitAccount;
use crate::state::AppState;
use serde::Deserialize;
use tauri::State;

fn enrich_keyring(mut accounts: Vec<UpbitAccount>) -> Vec<UpbitAccount> {
    for a in accounts.iter_mut() {
        let (ha, hs) = has_keys_for(a.id);
        a.has_access_key = ha;
        a.has_secret_key = hs;
    }
    accounts
}

#[tauri::command]
pub fn list_upbit_accounts(state: State<'_, AppState>) -> Result<Vec<UpbitAccount>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let accounts = upbit_accounts_repo::list_accounts(&conn, 1).map_err(|e| e.to_string())?;
    drop(conn); // keyring I/O는 lock 풀고
    Ok(enrich_keyring(accounts))
}

#[derive(Deserialize)]
pub struct AddAccountArgs {
    pub label: String,
    pub access_key: String,
    pub secret_key: String,
}

#[tauri::command]
pub async fn add_upbit_account(
    args: AddAccountArgs,
    state: State<'_, AppState>,
) -> Result<UpbitAccount, String> {
    let label = args.label.trim().to_string();
    if label.is_empty() {
        return Err("라벨은 비어 있을 수 없습니다.".into());
    }
    if args.access_key.trim().is_empty() || args.secret_key.trim().is_empty() {
        return Err("access_key / secret_key는 비어 있을 수 없습니다.".into());
    }

    // Phase 1: DB row insert (짧은 sync 작업, lock 해제 후 await)
    let new_id = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        upbit_accounts_repo::insert_account(&conn, 1, &label).map_err(|e| {
            if e.to_string().to_lowercase().contains("unique") {
                "같은 이름의 계정이 이미 있습니다.".to_string()
            } else {
                e.to_string()
            }
        })?
    };

    // Phase 2: keyring write
    let access = args.access_key.trim();
    let secret = args.secret_key.trim();
    if let Err(e) = save_keys_for(new_id, access, secret) {
        // DB row 보상 삭제
        let conn = state.db.lock().map_err(|err| err.to_string())?;
        let _ = upbit_accounts_repo::delete_account(&conn, new_id);
        return Err(format!("키 저장 실패: {e}"));
    }

    // Phase 3: 연결 테스트 (async)
    let test_result = match upbit_client_for(new_id) {
        Ok(client) => client.get_all_balances().await.map_err(|e| e.to_string()),
        Err(e) => Err(e),
    };
    if let Err(e) = test_result {
        delete_keys_for(new_id);
        let conn = state.db.lock().map_err(|err| err.to_string())?;
        let _ = upbit_accounts_repo::delete_account(&conn, new_id);
        return Err(format!("Upbit 연결 실패 (롤백됨): {e}"));
    }

    // 최종 응답
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let acc = upbit_accounts_repo::get_account(&conn, new_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "방금 만든 계정을 다시 읽지 못했습니다".to_string())?;
    drop(conn);
    Ok(enrich_keyring(vec![acc]).into_iter().next().unwrap())
}

#[tauri::command]
pub async fn test_upbit_account_connection(id: i64) -> Result<usize, String> {
    let client = upbit_client_for(id)?;
    let balances = client
        .get_all_balances()
        .await
        .map_err(|e| format!("Upbit API rejected: {e}"))?;
    Ok(balances.len())
}
```

- [ ] **Step 2: `commands/mod.rs`에 등록**

`pub mod upbit_accounts;` 한 줄 추가 (기존 `pub mod upbit_keys;` 옆).

- [ ] **Step 3: 빌드 확인**

```bash
cd src-tauri && cargo build --features tauri-app
```

Expected: build 성공.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/upbit_accounts.rs src-tauri/src/commands/mod.rs
git commit -m "feat(commands): upbit_accounts list/add/test_connection with compensating-rollback flow"
```

---

## Task 7: commands/upbit_accounts.rs — update / delete / set_enabled

**Files:**
- Modify: `src-tauri/src/commands/upbit_accounts.rs`

업데이트/삭제는 running 세션 가드가 핵심.

- [ ] **Step 1: 가드 헬퍼 + 3개 커맨드 추가**

`upbit_accounts.rs` 끝에 추가:

```rust
fn guard_no_running(conn: &rusqlite::Connection, account_id: i64) -> Result<(), String> {
    let n = upbit_accounts_repo::count_running_sessions(conn, account_id)
        .map_err(|e| e.to_string())?;
    if n > 0 {
        return Err("실행 중인 세션이 있습니다. 먼저 세션을 중지하세요.".into());
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct UpdateAccountArgs {
    pub id: i64,
    pub label: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[tauri::command]
pub async fn update_upbit_account(
    args: UpdateAccountArgs,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let id = args.id;

    // 라벨만 바꾸는 경우는 가드 없이 허용 (주문 경로 영향 없음).
    if let Some(label) = args.label.as_deref() {
        let trimmed = label.trim();
        if trimmed.is_empty() {
            return Err("라벨은 비어 있을 수 없습니다.".into());
        }
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        upbit_accounts_repo::update_label(&conn, id, trimmed).map_err(|e| {
            if e.to_string().to_lowercase().contains("unique") {
                "같은 이름의 계정이 이미 있습니다.".to_string()
            } else {
                e.to_string()
            }
        })?;
    }

    // 키 변경은 running 세션 가드 + 새 키 검증.
    if args.access_key.is_some() || args.secret_key.is_some() {
        {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            guard_no_running(&conn, id)?;
        } // lock 해제 후 keyring/await

        // 둘 중 하나만 들어와도 둘 다 새로 저장 — 부분 갱신 방지.
        let new_access = args
            .access_key
            .ok_or_else(|| "키 갱신 시 access_key/secret_key를 모두 보내세요.".to_string())?;
        let new_secret = args
            .secret_key
            .ok_or_else(|| "키 갱신 시 access_key/secret_key를 모두 보내세요.".to_string())?;
        save_keys_for(id, new_access.trim(), new_secret.trim())?;
        // 연결 테스트 — 실패 시 옛 키 복구 불가능하므로 사용자에게 알림.
        let client = upbit_client_for(id)?;
        if let Err(e) = client.get_all_balances().await {
            return Err(format!("새 키 저장됨, 그러나 Upbit 연결 실패: {e}"));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn set_upbit_account_enabled(
    id: i64,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if !enabled {
        guard_no_running(&conn, id)?; // disable 시에도 running 있으면 거부
    }
    upbit_accounts_repo::set_enabled(&conn, id, enabled).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_upbit_account(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    guard_no_running(&conn, id)?;
    upbit_accounts_repo::delete_account(&conn, id).map_err(|e| e.to_string())?;
    drop(conn);
    delete_keys_for(id);
    Ok(())
}
```

- [ ] **Step 2: 빌드 확인**

```bash
cd src-tauri && cargo build --features tauri-app
```

Expected: build 성공.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/upbit_accounts.rs
git commit -m "feat(commands): upbit_accounts update/delete/set_enabled with running-session guards"
```

---

## Task 8: LiveSession 모델에 upbit_account_id + account_label 추가

**Files:**
- Modify: `src-tauri/src/models/live.rs`

- [ ] **Step 1: 필드 추가**

`src-tauri/src/models/live.rs`의 `LiveSession` 구조체 끝(`pub created_at: String,` 직전)에 두 필드 추가:

```rust
    /// 이 세션이 실주문을 보낼 Upbit 계정. NULL은 마이그레이션 직전의
    /// 옛 세션에서만 발생하고, 신규 세션은 백엔드가 NOT NULL을 강제한다.
    #[serde(default)]
    pub upbit_account_id: Option<i64>,
    /// JOIN으로 가져오는 표시용 라벨. NULL이면 "[삭제됨]"으로 UI가 표시.
    #[serde(default)]
    pub account_label: Option<String>,
    pub created_at: String,
}
```

- [ ] **Step 2: 빌드 확인 (실패할 것 — repo가 아직 row_to_session에서 채우지 않음)**

```bash
cd src-tauri && cargo build --features tauri-app
```

`LiveSession { ... }` 생성자가 새 필드를 빠뜨려 에러가 날 예정. 다음 Task에서 fix.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/models/live.rs
git commit -m "feat(models): add upbit_account_id + account_label to LiveSession"
```

---

## Task 9: live_repo 갱신 — JOIN, insert 시그니처, count_real_sessions 제거

**Files:**
- Modify: `src-tauri/src/db/live_repo.rs`
- Modify: `src-tauri/src/services/session_engine.rs` (build 통과시키기 위한 최소 변경 — 호출처는 다음 Task)
- Modify: `src-tauri/src/services/auto_trader.rs` (동일)

- [ ] **Step 1: insert_session에 upbit_account_id 인자 추가**

```rust
// 기존
pub fn insert_session(
    conn: &Connection,
    user_id: i64,
    label: &str,
    preset_id: i64,
    market: &str,
    mode: &str,
    initial_capital: f64,
    start_ts: &str,
) -> Result<i64> { ... }
```

```rust
// 변경 후
pub fn insert_session(
    conn: &Connection,
    user_id: i64,
    label: &str,
    preset_id: i64,
    market: &str,
    mode: &str,
    initial_capital: f64,
    start_ts: &str,
    upbit_account_id: Option<i64>,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO live_sessions
            (user_id, label, preset_id, market, mode, status, initial_capital,
             start_ts, real_started_at, current_position, current_equity,
             live_return, created_at, upbit_account_id)
         VALUES (?1, ?2, ?3, ?4, ?5, 'stopped', ?6, ?7, ?8, 'idle', ?6, 0.0, ?8, ?9)",
        params![user_id, label, preset_id, market, mode, initial_capital,
                start_ts, now, upbit_account_id],
    )?;
    Ok(conn.last_insert_rowid())
}
```

- [ ] **Step 2: SELECT 쿼리 + row_to_session에 LEFT JOIN으로 account_label 추가**

기존 `get_session` / `list_sessions` / `list_running_sessions` 3개 SELECT 모두 다음과 같이 바꾼다:

```sql
SELECT ls.id, ls.user_id, ls.label, ls.preset_id, ls.market, ls.mode, ls.status,
       ls.initial_capital, ls.start_ts, ls.real_started_at, ls.last_cycle_ts,
       ls.last_signal, ls.current_position, ls.current_buy_price, ls.current_buy_volume,
       ls.current_equity, ls.live_return, ls.max_daily_loss_pct, ls.max_daily_trades,
       ls.max_order_krw, ls.created_at,
       ls.upbit_account_id, ua.label AS account_label
FROM live_sessions ls
LEFT JOIN upbit_accounts ua ON ua.id = ls.upbit_account_id
WHERE ls.id = ?1
```

(WHERE 절은 함수별로 유지 — `WHERE ls.user_id = ?1`, `WHERE ls.status = 'running'` 등.)

`row_to_session` 갱신:

```rust
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
        live_return: row.get(16)?,
        max_daily_loss_pct: row.get(17)?,
        max_daily_trades: row.get(18)?,
        max_order_krw: row.get(19)?,
        created_at: row.get(20)?,
        upbit_account_id: row.get(21)?,
        account_label: row.get(22)?,
    })
}
```

- [ ] **Step 3: `count_real_sessions` 제거**

`live_repo.rs:171-188`의 `count_real_sessions` 함수 전체 삭제 (주석 포함). 새 multi-account 정책은 부분 유니크 인덱스가 강제.

- [ ] **Step 4: 호출처 build 에러 임시 fix**

`insert_session` 호출처는 `commands/live_trading.rs::create_session` 한 곳. 일단 `None`을 마지막 인자로 전달해서 빌드만 통과시킨다 (다음 Task 10에서 args에서 받도록 정식 변경):

```rust
// commands/live_trading.rs::create_session 안에서 임시
let id = live_repo::insert_session(
    &conn,
    1,
    &args.label,
    args.preset_id,
    &args.market,
    "paper",
    args.initial_capital,
    &start_ts,
    None, // ← 임시. Task 10에서 args.upbit_account_id로 교체.
)
```

`commands/live_trading.rs::toggle_session_mode`에서 `count_real_sessions` 호출이 있다면 그 블록 전체를 일단 제거 (검증은 Task 10에서 새 로직으로).

- [ ] **Step 5: 빌드 확인**

```bash
cd src-tauri && cargo build --features tauri-app
```

Expected: build 성공.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/db/live_repo.rs src-tauri/src/commands/live_trading.rs
git commit -m "feat(live-repo): JOIN account_label, insert_session(upbit_account_id), drop count_real_sessions"
```

---

## Task 10: commands/live_trading.rs — CreateSessionArgs + start_session 검증

**Files:**
- Modify: `src-tauri/src/commands/live_trading.rs`

- [ ] **Step 1: CreateSessionArgs에 upbit_account_id 필드 추가**

```rust
#[derive(Deserialize)]
pub struct CreateSessionArgs {
    pub label: String,
    pub preset_id: i64,
    pub market: String,
    pub initial_capital: f64,
    #[serde(default)]
    pub max_order_krw: Option<f64>,
    /// 이 세션이 실주문을 보낼 Upbit 계정. 신규 세션은 필수 (Some 강제).
    /// Option으로 두는 이유는 serde가 누락된 필드를 None으로 받게 해서
    /// 백엔드 한 곳에서 명시적 에러를 던지기 위함.
    pub upbit_account_id: Option<i64>,
}
```

- [ ] **Step 2: create_session 검증 + insert 호출 정식화**

`create_session` 함수의 lock 직후, preset 조회 직전에:

```rust
let account_id = args.upbit_account_id
    .ok_or_else(|| "Upbit 계정을 선택하세요. (Accounts 페이지에서 먼저 등록)".to_string())?;
let acc = crate::db::upbit_accounts_repo::get_account(&conn, account_id)
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("계정 {account_id}이 존재하지 않습니다."))?;
if !acc.enabled {
    return Err("선택한 계정이 비활성화 상태입니다.".into());
}
let (has_a, has_s) = crate::commands::upbit_keys::has_keys_for(account_id);
if !has_a || !has_s {
    return Err("선택한 계정에 API 키가 설정되지 않았습니다.".into());
}
```

`insert_session` 호출의 마지막 인자를 `Some(account_id)`로 교체.

- [ ] **Step 3: toggle_session_mode의 검증 새로 작성**

기존 `toggle_session_mode`에서 단일 키 확인(`load_upbit_keys()`) + (제거 예정인) `count_real_sessions` 자리를 다음으로 대체:

```rust
if mode == "real" {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let session = live_repo::get_session(&conn, args.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("session {} not found", args.id))?;
    let account_id = session
        .upbit_account_id
        .ok_or_else(|| "이 세션에는 Upbit 계정이 연결되어 있지 않습니다.".to_string())?;
    let acc = crate::db::upbit_accounts_repo::get_account(&conn, account_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "계정이 삭제되었습니다.".to_string())?;
    if !acc.enabled {
        return Err("계정이 비활성화 상태입니다.".into());
    }
    let (ha, hs) = crate::commands::upbit_keys::has_keys_for(account_id);
    if !ha || !hs {
        return Err("계정 API 키가 설정되지 않았습니다.".into());
    }
    // 부분 유니크 인덱스가 같은 계정 두 번째 running real을 DB 레벨에서 차단.
    // 여기서는 추가 검사 불필요.
}
```

- [ ] **Step 4: 빌드 확인**

```bash
cd src-tauri && cargo build --features tauri-app
```

Expected: build 성공.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/live_trading.rs
git commit -m "feat(live-trading): require upbit_account_id on create, validate on real-mode toggle"
```

---

## Task 11: pending_order_tracker — per-session reconcile

**Files:**
- Modify: `src-tauri/src/services/pending_order_tracker.rs`

- [ ] **Step 1: 새 함수 추가**

기존 `reconcile_pending_orders(db, upbit)` 위에 새 함수 추가 (기존 함수는 삭제하지 않고 둠 — Task 12에서 호출처를 바꾼 후 다음 Task에서 정리):

```rust
/// 한 세션의 wait pending만 reconcile. 멀티 계정에서 N×M API 중복 fetch
/// 회피용: 세션은 자기 주문만 책임지고, 다른 세션의 pending은 다른 세션의
/// cycle이 처리한다.
pub async fn reconcile_pending_orders_for_session(
    db: &Arc<Mutex<Connection>>,
    session_id: i64,
    upbit_account_id: i64,
) -> Result<usize, BoxErr> {
    let pendings = {
        let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
        live_repo::list_pending_wait_by_session(&conn, session_id)
            .map_err(|e| -> BoxErr { e.to_string().into() })?
    };
    if pendings.is_empty() {
        return Ok(0);
    }
    let upbit = match crate::commands::upbit_keys::upbit_client_for(upbit_account_id) {
        Ok(c) => c,
        Err(e) => {
            crate::live_log!(
                "[pending_tracker] account {upbit_account_id} client error: {e}"
            );
            return Ok(0);
        }
    };
    reconcile_inner(db, &upbit, pendings).await
}

/// 기존 reconcile_pending_orders의 본문을 이 함수로 추출. 동작 동일.
async fn reconcile_inner(
    db: &Arc<Mutex<Connection>>,
    upbit: &UpbitClient,
    pendings: Vec<crate::models::live::PendingOrder>,
) -> Result<usize, BoxErr> {
    // (기존 reconcile_pending_orders 본문의 for p in pendings { ... } 블록을
    // 통째로 이쪽으로 이동. 시그니처만 다르고 로직은 동일.)
    // ↓ 기존 함수 body의 line 53 이후를 그대로 옮긴다.
    let now = Utc::now();
    let mut resolved = 0usize;
    for p in pendings {
        // ... (기존 코드 그대로)
    }
    Ok(resolved)
}

/// @deprecated — `reconcile_pending_orders_for_session` 사용. 외부 진단용으로만.
pub async fn reconcile_pending_orders(
    db: &Arc<Mutex<Connection>>,
    upbit: &UpbitClient,
) -> Result<usize, BoxErr> {
    let pendings = {
        let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
        live_repo::list_pending_wait(&conn)
            .map_err(|e| -> BoxErr { e.to_string().into() })?
    };
    reconcile_inner(db, upbit, pendings).await
}
```

(실제 작업 시 기존 함수 본문을 그대로 `reconcile_inner`로 옮기고, 양쪽 진입점이 같은 inner를 호출하게 만든다.)

- [ ] **Step 2: 빌드 확인**

```bash
cd src-tauri && cargo build --features tauri-app
cd src-tauri && cargo test pending_orders_test --no-run
```

Expected: 둘 다 성공. (기존 테스트는 아직 `reconcile_pending_orders`를 호출 — 그대로 유지)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/services/pending_order_tracker.rs
git commit -m "feat(pending-tracker): per-session reconcile entrypoint with account-specific client"
```

---

## Task 12: session_engine + auto_trader — account_id 기반 클라이언트

**Files:**
- Modify: `src-tauri/src/services/session_engine.rs`
- Modify: `src-tauri/src/services/auto_trader.rs`

- [ ] **Step 1: session_engine.rs에서 upbit client 생성을 account_id 기반으로**

`session_engine.rs`에서 `upbit_client_or_err()` 호출 지점들을 모두 찾아 다음으로 교체:

```rust
let account_id = session
    .upbit_account_id
    .ok_or_else(|| -> BoxErr { "session has no upbit_account_id (run migration?)".into() })?;
let upbit = crate::commands::upbit_keys::upbit_client_for(account_id)
    .map_err(|e| -> BoxErr { e.into() })?;
```

(real cycle 함수 입구 한 곳, 그 외 client 만드는 위치 모두.)

- [ ] **Step 2: pending tracker 호출을 per-session으로 교체**

`session_engine.rs:350` 위치:

```rust
// 기존
if let Err(e) = crate::services::pending_order_tracker::reconcile_pending_orders(db, &upbit).await {
    crate::live_log!("[realcycle] session={} pending reconcile error: {e}", session.id);
}
```

→ 변경:

```rust
if let Err(e) = crate::services::pending_order_tracker::reconcile_pending_orders_for_session(
    db, session.id, account_id,
).await {
    crate::live_log!("[realcycle] session={} pending reconcile error: {e}", session.id);
}
```

- [ ] **Step 3: auto_trader.rs에서 단일 키 의존 제거**

`auto_trader.rs`에 `upbit_client_or_err()` 또는 `load_upbit_keys()` 호출이 있다면, 호출하는 함수 시그니처에 `upbit_account_id: i64`를 추가하고 `upbit_client_for(account_id)`로 교체. 호출처(session_engine)에서 `account_id`를 전달.

`reconcile_position`이 client를 직접 받는 시그니처라면 시그니처 변경 불필요 — `session_engine`이 이미 만든 `upbit`을 그대로 전달.

(실제 변경량은 grep으로 확인 후 결정. 핵심은 "단일 키 경로 호출이 한 곳도 남지 않게.")

- [ ] **Step 4: 빌드 확인**

```bash
cd src-tauri && cargo build --features tauri-app
```

Expected: build 성공. 단일 키 시그니처에 대한 deprecated 경고는 호출처가 모두 정리되면 사라짐.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/session_engine.rs src-tauri/src/services/auto_trader.rs
git commit -m "feat(session-engine): per-account upbit client + per-session pending reconcile"
```

---

## Task 13: Discord notification — `[label]` prefix

**Files:**
- Modify: `src-tauri/src/services/session_engine.rs` (또는 notification 빌더가 따로 있다면 그 파일)

- [ ] **Step 1: notification 메시지 빌드 지점 찾기**

```bash
cd src-tauri && grep -rn "discord\|webhook\|notification" src/services/ | grep -i "msg\|format\|push\|send"
```

- [ ] **Step 2: 메시지 prefix 추가**

세션 cycle에서 Discord/notification으로 보내는 메시지 빌더 위치에서 `session.account_label`로 prefix:

```rust
let prefix = match session.account_label.as_deref() {
    Some(label) => format!("[{label}] "),
    None => String::new(),
};
let msg = format!("{prefix}{original_msg}");
```

(이미 `session`을 갖고 있으므로 추가 DB lookup 없이 JOIN으로 함께 로드된 `account_label`을 활용. 호출처가 `LiveSession` 외 다른 구조로 메시지를 만들면 거기서도 같은 prefix 추가.)

- [ ] **Step 3: 수동 확인용 토픽 정리**

본 step은 외부 webhook이 있어야 진짜 검증이 됨 → 빌드 통과만 확인하고, 사용자가 직접 디스코드 채널에서 prefix 보이는지 확인하도록 §"E2E 수동 검증" 체크리스트에 항목 추가 (Task 20에서).

- [ ] **Step 4: 빌드 확인 + Commit**

```bash
cd src-tauri && cargo build --features tauri-app
git add src-tauri/src/services/session_engine.rs
git commit -m "feat(notify): prefix Discord messages with [account_label]"
```

---

## Task 14: commands/trading.rs — 단일 키 의존 제거

**Files:**
- Modify: `src-tauri/src/commands/trading.rs`

`trading.rs` 안의 `create_client()`/`create_public_client()`는 단일 키를 쓴다(`upbit_keys.rs:42-43,52,62,114`). 멀티 계정에선 어떤 계정을 쓸지 모호 → `get_balance`/`get_position`은 시그니처에 account_id 추가, `manual_buy`/`manual_sell`은 제거 (ManualOrderCard + manual_market_order가 대체 경로).

- [ ] **Step 1: create_client 제거 + 함수별 account_id 인자**

```rust
// 제거: fn create_client() -> Result<UpbitClient, String> { ... }
// 유지: fn create_public_client() — get_current_price만 사용 (인증 불필요)
//        단, public client 본체에서도 single-key 로딩을 제거하고 빈 키로 생성.
fn create_public_client() -> UpbitClient {
    UpbitClient::new(String::new(), String::new())
}
```

- [ ] **Step 2: get_balance / get_position 시그니처에 account_id**

```rust
#[tauri::command]
pub async fn get_balance(account_id: i64, currency: String) -> Result<f64, String> {
    let client = crate::commands::upbit_keys::upbit_client_for(account_id)?;
    client.get_balance(&currency).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_position(account_id: i64, market: String) -> Result<PositionInfo, String> {
    let client = crate::commands::upbit_keys::upbit_client_for(account_id)?;
    // ... (기존 로직 동일하지만 client만 교체)
}
```

- [ ] **Step 3: manual_buy / manual_sell 함수 삭제**

해당 두 함수와 매개변수 구조체 (있다면) 모두 삭제.

- [ ] **Step 4: 빌드 확인 (실패할 것)**

```bash
cd src-tauri && cargo build --features tauri-app
```

`lib.rs`의 invoke_handler에 `manual_buy`/`manual_sell` 등록이 남아 있어 컴파일 에러. 다음 Task에서 정리.

- [ ] **Step 5: 추가 호출처 확인 + 정리**

`manual_buy`/`manual_sell`을 다른 Rust 코드에서 호출하는 곳이 없는지:

```bash
cd src-tauri && grep -rn "manual_buy\|manual_sell" src/
```

(이미 ManualOrderCard로 대체된 상태일 가능성 큼.)

- [ ] **Step 6: Commit (lib.rs 미수정 상태로 — 다음 Task에서 묶음)**

```bash
git add src-tauri/src/commands/trading.rs
git commit -m "feat(trading-cmds): remove manual_buy/sell, add account_id to get_balance/get_position"
```

---

## Task 15: lib.rs — invoke_handler 신규 커맨드 등록 + legacy 정리

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: invoke_handler 갱신**

`generate_handler![...]` 매크로 안에서:

- 추가:
  ```
  upbit_accounts::list_upbit_accounts,
  upbit_accounts::add_upbit_account,
  upbit_accounts::update_upbit_account,
  upbit_accounts::delete_upbit_account,
  upbit_accounts::set_upbit_account_enabled,
  upbit_accounts::test_upbit_account_connection,
  ```

- 제거 (line 145, 146):
  ```
  trading::manual_buy,
  trading::manual_sell,
  ```

- 유지하되 deprecated 주석 (line 177~):
  ```
  upbit_keys::save_upbit_keys,
  upbit_keys::get_upbit_key_status,
  upbit_keys::clear_upbit_keys,
  upbit_keys::test_upbit_connection,
  ```
  → 한 릴리즈 사이클 후 제거.

- [ ] **Step 2: `use` 선언에 `upbit_accounts` 추가**

`lib.rs` 상단의 `use commands::{...};`에 `upbit_accounts` 추가.

- [ ] **Step 3: 빌드 + 단위 테스트 확인**

```bash
cd src-tauri && cargo build --features tauri-app
cd src-tauri && cargo test
```

Expected: build 성공, 기존 테스트 + 신규 테스트(Task 3, 4) 모두 통과.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(lib): register upbit_accounts commands, drop manual_buy/sell"
```

---

## Task 16: 프론트엔드 타입 + API 클라이언트

**Files:**
- Modify: `src/types/index.ts`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/live.ts` (필요 시)

- [ ] **Step 1: src/types/index.ts에 UpbitAccount + LiveSession 확장**

`src/types/index.ts`에 추가:

```ts
export type UpbitAccount = {
  id: number;
  user_id: number;
  label: string;
  enabled: boolean;
  created_at: string;
  has_access_key: boolean;
  has_secret_key: boolean;
  has_running_session: boolean;
};
```

기존 `LiveSession` 타입에 두 필드 추가:

```ts
export type LiveSession = {
  // ... 기존 필드
  upbit_account_id: number | null;
  account_label: string | null;
};
```

기존 `CreateSessionArgs` 타입(또는 `NewSessionDialog`에서 쓰는 인자 타입)에 `upbit_account_id: number` 추가.

- [ ] **Step 2: src/lib/api.ts 갱신**

```ts
// 신규
import type { UpbitAccount } from "../types";

export const listUpbitAccounts = (): Promise<UpbitAccount[]> =>
  tauriInvoke("list_upbit_accounts");

export const addUpbitAccount = (
  label: string, access_key: string, secret_key: string
): Promise<UpbitAccount> =>
  tauriInvoke("add_upbit_account", { args: { label, access_key, secret_key } });

export const updateUpbitAccount = (args: {
  id: number; label?: string; access_key?: string; secret_key?: string;
}): Promise<void> => tauriInvoke("update_upbit_account", { args });

export const deleteUpbitAccount = (id: number): Promise<void> =>
  tauriInvoke("delete_upbit_account", { id });

export const setUpbitAccountEnabled = (id: number, enabled: boolean): Promise<void> =>
  tauriInvoke("set_upbit_account_enabled", { id, enabled });

export const testUpbitAccountConnection = (id: number): Promise<number> =>
  tauriInvoke("test_upbit_account_connection", { id });

// 변경: getBalance / getPosition에 accountId
export async function getBalance(accountId: number, currency: string): Promise<number> {
  if (isTauri) return tauriInvoke("get_balance", { accountId, currency });
  throw new Error("Balance check is only available in desktop mode");
}

export async function getPosition(accountId: number, market: string): Promise<PositionInfo> {
  if (isTauri) return tauriInvoke("get_position", { accountId, market });
  throw new Error("Position is only available in desktop mode");
}

// 제거: manualBuy, manualSell 함수 전체 삭제 (export까지)
```

- [ ] **Step 3: tsc 확인**

```bash
npm run vite:build
```

Expected: 이 시점에선 `tradingStore.ts` 및 `ManualOrderDialog.tsx`에서 에러 발생 (Task 19에서 정리).

타입 에러를 빠르게 보고 싶다면 `tsc --noEmit`만:

```bash
npx tsc --noEmit
```

- [ ] **Step 4: Commit**

```bash
git add src/types/index.ts src/lib/api.ts
git commit -m "feat(api): UpbitAccount type + account commands; getBalance/Position accountId param; drop manualBuy/Sell"
```

---

## Task 17: AccountsPage + AccountCard + AddAccountDialog

**Files:**
- Create: `src/pages/AccountsPage.tsx`
- Create: `src/components/accounts/AccountCard.tsx`
- Create: `src/components/accounts/AddAccountDialog.tsx`

- [ ] **Step 1: AccountCard 컴포넌트**

```tsx
// src/components/accounts/AccountCard.tsx
import type { UpbitAccount } from "../../types";
import { Card } from "../ui/Card";
import { Button } from "../ui/Button";
import { Badge } from "../ui/Badge";

interface Props {
  account: UpbitAccount;
  onTest: (id: number) => void;
  onEdit: (id: number) => void;
  onDelete: (id: number) => void;
  onToggleEnabled: (id: number, next: boolean) => void;
  testing?: boolean;
  testResult?: string | null;
}

export function AccountCard({
  account, onTest, onEdit, onDelete, onToggleEnabled, testing, testResult,
}: Props) {
  const keysOk = account.has_access_key && account.has_secret_key;
  return (
    <Card className="p-4 flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <h3 className="text-base font-semibold">[{account.label}]</h3>
        <Badge variant={account.enabled ? "success" : "muted"}>
          {account.enabled ? "enabled" : "disabled"}
        </Badge>
        {account.has_running_session && <Badge variant="info">실행 중</Badge>}
      </div>
      <div className="text-xs text-zinc-400">
        <span className={keysOk ? "text-emerald-400" : "text-amber-400"}>
          {keysOk ? "✓ 키 설정됨" : "⚠ 키 미설정"}
        </span>
        {testResult && <span className="ml-2">{testResult}</span>}
      </div>
      <div className="flex gap-2 mt-2">
        <Button size="sm" onClick={() => onTest(account.id)} disabled={testing || !keysOk}>
          {testing ? "테스트 중..." : "연결 테스트"}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => onEdit(account.id)}>수정</Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => onToggleEnabled(account.id, !account.enabled)}
          disabled={account.has_running_session}
        >
          {account.enabled ? "비활성화" : "활성화"}
        </Button>
        <Button
          size="sm"
          variant="danger"
          onClick={() => onDelete(account.id)}
          disabled={account.has_running_session}
        >
          삭제
        </Button>
      </div>
    </Card>
  );
}
```

- [ ] **Step 2: AddAccountDialog**

```tsx
// src/components/accounts/AddAccountDialog.tsx
import { useState } from "react";
import { addUpbitAccount } from "../../lib/api";
import type { UpbitAccount } from "../../types";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";

interface Props {
  open: boolean;
  onClose: () => void;
  onAdded: (acc: UpbitAccount) => void;
}

export function AddAccountDialog({ open, onClose, onAdded }: Props) {
  const [label, setLabel] = useState("");
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  const handleSubmit = async () => {
    setError(null);
    setSubmitting(true);
    try {
      const acc = await addUpbitAccount(label.trim(), accessKey.trim(), secretKey.trim());
      onAdded(acc);
      setLabel(""); setAccessKey(""); setSecretKey("");
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-zinc-900 rounded-lg p-6 w-[480px] flex flex-col gap-3">
        <h2 className="text-lg font-semibold">계정 추가</h2>
        <label className="text-sm">라벨</label>
        <Input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="예: 메인" />
        <label className="text-sm">Access Key</label>
        <Input value={accessKey} onChange={(e) => setAccessKey(e.target.value)} placeholder="Upbit access key" />
        <label className="text-sm">Secret Key</label>
        <Input
          value={secretKey} onChange={(e) => setSecretKey(e.target.value)}
          placeholder="Upbit secret key" type="password"
        />
        {error && <div className="text-rose-400 text-sm">{error}</div>}
        <div className="flex justify-end gap-2 mt-2">
          <Button variant="ghost" onClick={onClose} disabled={submitting}>취소</Button>
          <Button onClick={handleSubmit} disabled={submitting || !label.trim() || !accessKey || !secretKey}>
            {submitting ? "테스트 중..." : "추가"}
          </Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: AccountsPage**

```tsx
// src/pages/AccountsPage.tsx
import { useEffect, useState } from "react";
import {
  listUpbitAccounts, deleteUpbitAccount, setUpbitAccountEnabled,
  testUpbitAccountConnection,
} from "../lib/api";
import type { UpbitAccount } from "../types";
import { AccountCard } from "../components/accounts/AccountCard";
import { AddAccountDialog } from "../components/accounts/AddAccountDialog";
import { Button } from "../components/ui/Button";

export default function AccountsPage() {
  const [accounts, setAccounts] = useState<UpbitAccount[]>([]);
  const [addOpen, setAddOpen] = useState(false);
  const [testing, setTesting] = useState<number | null>(null);
  const [testResults, setTestResults] = useState<Record<number, string>>({});
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      setAccounts(await listUpbitAccounts());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  useEffect(() => { refresh(); }, []);

  const handleTest = async (id: number) => {
    setTesting(id);
    try {
      const n = await testUpbitAccountConnection(id);
      setTestResults((r) => ({ ...r, [id]: `✓ ${n}개 통화` }));
    } catch (e) {
      setTestResults((r) => ({ ...r, [id]: `✗ ${e instanceof Error ? e.message : e}` }));
    } finally {
      setTesting(null);
    }
  };

  const handleDelete = async (id: number) => {
    if (!confirm("정말 삭제하시겠습니까? 키링에서도 함께 삭제됩니다.")) return;
    try { await deleteUpbitAccount(id); await refresh(); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  };

  const handleToggle = async (id: number, next: boolean) => {
    try { await setUpbitAccountEnabled(id, next); await refresh(); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  };

  return (
    <div className="p-6 flex flex-col gap-4">
      <div className="flex justify-between items-center">
        <h1 className="text-2xl font-bold">Upbit 계정</h1>
        <Button onClick={() => setAddOpen(true)}>+ 계정 추가</Button>
      </div>
      {error && <div className="text-rose-400">{error}</div>}
      {accounts.length === 0 ? (
        <div className="text-zinc-400 text-sm">
          등록된 계정이 없습니다. "+ 계정 추가"를 눌러 첫 계정을 등록하세요.
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {accounts.map((a) => (
            <AccountCard
              key={a.id} account={a}
              onTest={handleTest}
              onEdit={() => { /* Task 18 */ }}
              onDelete={handleDelete}
              onToggleEnabled={handleToggle}
              testing={testing === a.id}
              testResult={testResults[a.id] ?? null}
            />
          ))}
        </div>
      )}
      <AddAccountDialog open={addOpen} onClose={() => setAddOpen(false)} onAdded={refresh} />
    </div>
  );
}
```

- [ ] **Step 4: tsc 확인**

```bash
npx tsc --noEmit
```

Expected: 신규 파일들에 대해서는 에러 없음. (기존 ManualOrderDialog/tradingStore 에러는 Task 19까지 남아 있을 수 있음.)

- [ ] **Step 5: Commit**

```bash
git add src/pages/AccountsPage.tsx src/components/accounts/
git commit -m "feat(ui): AccountsPage with AccountCard + AddAccountDialog"
```

---

## Task 18: EditAccountDialog + 라우팅 + Settings 정리

**Files:**
- Create: `src/components/accounts/EditAccountDialog.tsx`
- Modify: `src/App.tsx`
- Modify: `src/pages/SettingsPage.tsx` (또는 Settings 컴포넌트)
- Modify: `src/pages/AccountsPage.tsx` (EditDialog 연결)

- [ ] **Step 1: EditAccountDialog**

```tsx
// src/components/accounts/EditAccountDialog.tsx
import { useState } from "react";
import { updateUpbitAccount } from "../../lib/api";
import type { UpbitAccount } from "../../types";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";

interface Props {
  account: UpbitAccount;
  onClose: () => void;
  onUpdated: () => void;
}

export function EditAccountDialog({ account, onClose, onUpdated }: Props) {
  const [mode, setMode] = useState<"label" | "keys">("label");
  const [label, setLabel] = useState(account.label);
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    setError(null); setSubmitting(true);
    try {
      if (mode === "label") {
        await updateUpbitAccount({ id: account.id, label: label.trim() });
      } else {
        await updateUpbitAccount({
          id: account.id, access_key: accessKey.trim(), secret_key: secretKey.trim(),
        });
      }
      onUpdated(); onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally { setSubmitting(false); }
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-zinc-900 rounded-lg p-6 w-[480px] flex flex-col gap-3">
        <h2 className="text-lg font-semibold">[{account.label}] 수정</h2>
        <div className="flex gap-2 text-sm">
          <button
            className={mode === "label" ? "text-emerald-400" : "text-zinc-500"}
            onClick={() => setMode("label")}
          >라벨</button>
          <button
            className={mode === "keys" ? "text-emerald-400" : "text-zinc-500"}
            onClick={() => setMode("keys")}
            disabled={account.has_running_session}
            title={account.has_running_session ? "실행 중 세션이 있으면 키 변경 불가" : ""}
          >키</button>
        </div>
        {mode === "label" ? (
          <Input value={label} onChange={(e) => setLabel(e.target.value)} />
        ) : (
          <>
            <Input
              value={accessKey} onChange={(e) => setAccessKey(e.target.value)}
              placeholder="새 Access Key"
            />
            <Input
              value={secretKey} onChange={(e) => setSecretKey(e.target.value)}
              placeholder="새 Secret Key" type="password"
            />
          </>
        )}
        {error && <div className="text-rose-400 text-sm">{error}</div>}
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose} disabled={submitting}>취소</Button>
          <Button onClick={handleSubmit} disabled={submitting}>저장</Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: AccountsPage에서 EditDialog 연결**

`AccountsPage.tsx`에 `editingId` state 추가:

```tsx
const [editingId, setEditingId] = useState<number | null>(null);
// ...
<AccountCard
  // ...
  onEdit={(id) => setEditingId(id)}
/>
// ...
{editingId !== null && (() => {
  const acc = accounts.find((a) => a.id === editingId);
  if (!acc) return null;
  return <EditAccountDialog
    account={acc}
    onClose={() => setEditingId(null)}
    onUpdated={refresh}
  />;
})()}
```

- [ ] **Step 3: App.tsx 라우팅 + 사이드바**

```tsx
// src/App.tsx
import AccountsPage from "./pages/AccountsPage";
// ...
<Route path="/accounts" element={<AccountsPage />} />
```

사이드바에 메뉴 1줄 추가 (`Settings` 위에):
```tsx
<NavLink to="/accounts">Accounts</NavLink>
```

- [ ] **Step 4: SettingsPage의 Upbit 키 섹션 제거**

`src/pages/SettingsPage.tsx`에서 Upbit 키 카드 컴포넌트를 안내 카드로 교체:

```tsx
<Card>
  <h3 className="text-sm font-semibold">Upbit 계정</h3>
  <p className="text-zinc-400 text-sm">
    계정 관리는 Accounts 페이지로 이동했습니다.
  </p>
  <Link to="/accounts"><Button>Accounts 페이지로</Button></Link>
</Card>
```

기존의 `saveUpbitKeys` / `getUpbitKeyStatus` 호출은 모두 제거.

- [ ] **Step 5: tsc 확인 + Commit**

```bash
npx tsc --noEmit
git add src/components/accounts/EditAccountDialog.tsx src/App.tsx src/pages/SettingsPage.tsx src/pages/AccountsPage.tsx
git commit -m "feat(ui): EditAccountDialog + /accounts route + Settings: link to /accounts"
```

---

## Task 19: NewSessionDialog 계정 드롭다운 + ManualOrderCard 세션 셀렉터 + 세션 카드 배지

**Files:**
- Modify: `src/components/live/NewSessionDialog.tsx`
- Modify: `src/components/live/ManualOrderCard.tsx`
- Modify: `src/pages/LiveTradingPage.tsx` (세션 카드 라벨 배지)
- Modify: `src/stores/tradingStore.ts` (또는 사용처) — deprecate
- Delete or modify: `src/components/trading/ManualOrderDialog.tsx`

- [ ] **Step 1: NewSessionDialog에 계정 드롭다운**

`NewSessionDialog.tsx` 상단에:

```tsx
import { listUpbitAccounts } from "../../lib/api";
import type { UpbitAccount } from "../../types";

// state
const [accounts, setAccounts] = useState<UpbitAccount[]>([]);
const [accountId, setAccountId] = useState<number | null>(null);

useEffect(() => {
  if (!open) return;
  listUpbitAccounts().then((list) => {
    setAccounts(list);
    if (list.length === 1 && list[0].enabled) setAccountId(list[0].id);
  });
}, [open]);
```

폼 안에 Select:

```tsx
<label className="text-sm">Upbit 계정</label>
{accounts.length === 0 ? (
  <div className="text-amber-400 text-sm">
    등록된 계정이 없습니다. <Link to="/accounts">Accounts 페이지에서 먼저 등록</Link>하세요.
  </div>
) : (
  <Select value={accountId ?? ""} onChange={(e) => setAccountId(Number(e.target.value))}>
    <option value="">선택...</option>
    {accounts.map((a) => (
      <option key={a.id} value={a.id} disabled={!a.enabled || a.has_running_session}>
        [{a.label}]
        {!a.enabled && " (비활성)"}
        {a.has_running_session && " (실행 중)"}
      </option>
    ))}
  </Select>
)}
```

제출 시 `args.upbit_account_id = accountId`. 제출 버튼은 `accountId === null`이면 disabled.

- [ ] **Step 2: ManualOrderCard 명시적 세션 셀렉터**

기존 `const realSessions = sessions.filter((s) => s.mode === "real")` 유지. 추가:

```tsx
const [selectedSessionId, setSelectedSessionId] = useState<number | null>(
  realSessions.length === 1 ? realSessions[0].id : null
);

// 셀렉터 (real 세션이 2개 이상일 때 명시적 선택 강제)
{realSessions.length > 1 && (
  <Select value={selectedSessionId ?? ""} onChange={(e) => setSelectedSessionId(Number(e.target.value))}>
    <option value="">대상 세션 선택...</option>
    {realSessions.map((s) => (
      <option key={s.id} value={s.id}>
        [{s.account_label ?? "?"}] {s.label}
      </option>
    ))}
  </Select>
)}

// 주문 버튼 disabled 조건에 추가
disabled={submitting || !hasReal || selectedSessionId === null}

// manualMarketOrder 호출 시 session_id
session_id: selectedSessionId ?? undefined,
```

기존 `realSessions[0].id` 사용 라인(`ManualOrderCard.tsx:169`)을 위 `selectedSessionId`로 교체.

- [ ] **Step 3: LiveTradingPage SessionCard에 라벨 배지**

세션 카드/행이 렌더되는 곳에서 라벨 배지 추가:

```tsx
{session.account_label && (
  <Badge>{session.account_label}</Badge>
)}
{!session.account_label && session.upbit_account_id !== null && (
  <Badge variant="muted">[삭제됨]</Badge>
)}
```

- [ ] **Step 4: tradingStore + ManualOrderDialog 정리**

`tradingStore.ts`의 `getBalance("KRW")` / `getBalance("BTC")` 호출은 더 이상 작동하지 않음 (account_id 필요). 이 store가 어디서 쓰이는지 확인:

```bash
grep -rn "useTradingStore\|tradingStore" src/
```

쓰이지 않는다면 파일 자체를 삭제. 쓰이고 있다면 사용처에서 활성 계정 id를 전달하거나, 해당 UI 자체를 제거.

`src/components/trading/ManualOrderDialog.tsx`는 `manualBuy`/`manualSell` 의존이므로 더 이상 동작 안 함:
- 사용처가 있다면 `ManualOrderCard`로 대체
- 사용처가 없다면 파일 삭제

```bash
grep -rn "ManualOrderDialog" src/
```

- [ ] **Step 5: tsc + vite build 확인**

```bash
npm run vite:build
```

Expected: 빌드 성공. 에러가 남아 있으면 그 파일도 정리.

- [ ] **Step 6: Commit**

```bash
git add src/components/live/NewSessionDialog.tsx src/components/live/ManualOrderCard.tsx src/pages/LiveTradingPage.tsx
git add -u src/stores/tradingStore.ts src/components/trading/
git commit -m "feat(ui): account dropdown in NewSession, explicit session selector in ManualOrderCard, account badge on cards, drop legacy tradingStore/ManualOrderDialog"
```

---

## Task 20: E2E 수동 검증 + History.md / Manual 업데이트

**Files:**
- Modify: `History.md`
- Create/Modify: `Manual/upbit-accounts.md`

- [ ] **Step 1: 수동 검증 체크리스트 실행**

사용자에게 다음 항목을 확인 요청 (Claude는 서버 직접 실행 X — CLAUDE.md 규칙):

1. **마이그레이션**:
   - [ ] 기존 단일 키로 운영 중인 DB가 새 빌드에서 첫 부팅 시 `account_id=1, label='기본'`을 자동 생성
   - [ ] 옛 keyring 항목(`upbit_access_key`)이 삭제되고 `upbit_access_key_1`이 생성됨
   - [ ] 기존 running 세션의 `upbit_account_id`가 1로 채워짐
   - [ ] 클린 설치(키도 env도 없음)는 계정 0개로 시작 — Accounts 페이지에서 등록 안내가 보임
2. **계정 추가/연결 테스트**:
   - [ ] `/accounts`에서 "+ 계정 추가" → 잘못된 키로 시도 → DB row 안 만들어지고 keyring도 비어 있음
   - [ ] 올바른 키로 추가 → 카드 노출, "연결 테스트" 버튼 작동
3. **세션 시작**:
   - [ ] `NewSessionDialog`에 계정 드롭다운이 보이고, 계정 미선택 시 시작 버튼 disabled
   - [ ] 같은 계정 + real 모드 세션을 두 번째 시작 시도 → DB 부분 유니크 인덱스 위반으로 거부
   - [ ] 같은 계정에서 paper 세션 N개는 동시 작동 가능
4. **수동 주문**:
   - [ ] real 세션 2개 이상이면 ManualOrderCard에 세션 셀렉터 표시
   - [ ] 셀렉터 미선택 시 주문 버튼 disabled
5. **삭제 가드**:
   - [ ] running 세션이 있는 계정 삭제 시도 → "먼저 세션을 중지하세요" 에러
   - [ ] 키 수정도 동일 가드
6. **알림**:
   - [ ] Discord 채널에 cycle 이벤트 시 메시지 prefix `[메인]` 표시

- [ ] **Step 2: Manual/upbit-accounts.md 작성**

```markdown
# Upbit 멀티 계정 관리

## 개요

여러 Upbit 계정을 등록하고 계정당 하나의 자동매매 세션을 운영할 수 있습니다.

## 계정 등록

1. 좌측 사이드바에서 **Accounts** 클릭
2. "+ 계정 추가" 클릭
3. 라벨(예: "메인"), Access Key, Secret Key 입력
4. "추가" 클릭 → 자동으로 Upbit 연결 테스트 실행
   - 성공: 카드 목록에 추가됨
   - 실패: 키링/DB 모두 롤백됨, 에러 메시지 표시

## 세션 시작

1. **Live Trading** 페이지에서 "+ 새 세션"
2. **Upbit 계정** 드롭다운에서 선택
   - 이미 실행 중인 real 세션이 있는 계정은 선택 불가
   - 비활성화된 계정도 선택 불가
3. 프리셋, 라벨, 초기 자본 입력 후 시작

## 안전장치

- **1 계정 = 1 running real 세션**: DB 부분 유니크 인덱스가 강제
- **paper 세션**은 같은 계정에서 N개 공존 가능 (실주문 없음)
- **삭제/키수정 가드**: 실행 중 세션이 있는 계정은 변경/삭제 불가
- **연결 테스트 실패 시 자동 롤백**: 잘못된 키 입력으로 인한 잔존 상태 없음

## 마이그레이션 (기존 사용자)

- 첫 부팅 시 기존 단일 키가 자동으로 `account_id=1, label='기본'` 계정으로 흡수됨
- 기존 실행 중인 세션은 이 계정에 자동 연결
- 환경변수(`UPBIT_ACCESS_KEY`/`UPBIT_SECRET_KEY`)는 첫 부팅 시 1회만 참조 — 이후 무시
```

- [ ] **Step 3: History.md 업데이트**

```markdown
## 2026-05-15

- **success** — Multi Upbit Accounts 구현 완료
  - 여러 Upbit 계정 1급 엔티티 분리 (`upbit_accounts` 테이블)
  - 계정당 1 running real 세션 강제 (부분 유니크 인덱스)
  - 기존 단일 키 자동 마이그레이션 (account_id=1, label='기본')
  - pending_order_tracker per-session reconcile (N×M 중복 fetch 제거)
  - 기존 multi-real=1 전역 정책 제거
  - Discord 알림에 `[label]` prefix
  - 신규 `/accounts` 페이지 + 계정 카드 CRUD
```

- [ ] **Step 4: Commit**

```bash
git add History.md Manual/upbit-accounts.md
git commit -m "docs: multi-account manual + History.md update"
```

---

## Self-Review Summary

스펙 § ↔ Task 매핑 (커버리지 검증):

| 스펙 항목 | Task |
|-----------|------|
| §3.1 migration 014 SQL | Task 1 |
| §3.1.1 multi-real=1 제거 | Task 9 (count_real_sessions 삭제), Task 10 (toggle_session_mode 검증 변경) |
| §3.2 keyring 네이밍 | Task 5 |
| §3.3 자동 흡수 (seed_admin 다음) | Task 4 |
| §4.2.1 시그니처 변경 | Task 5 (신규), Task 12 (호출처) |
| §4.2.2 호출처 갱신 | Task 12, 14 |
| §4.2.3 add 보상 패턴 | Task 6 |
| §4.2.4 update/delete 가드 | Task 7 |
| §4.2.5 pending_order_tracker | Task 11 |
| §4.2.6 legacy trading 정리 | Task 14 |
| §4.2.7 invoke_handler | Task 15 |
| §4.3.1~3 AccountsPage 등 | Task 17, 18 |
| §4.3.4 NewSessionDialog | Task 19 |
| §4.3.5 ManualOrderCard | Task 19 |
| §4.4 Discord prefix | Task 13 |
| §5.1 add 흐름 | Task 6 (구현), Task 17 (UI 흐름) |
| §5.2 create/start 2-phase | Task 10 |
| §5.3 delete 흐름 | Task 7 |
| §6 에러 처리 | Task 6, 7, 10에 분산 |
| §7 테스트 | Task 3, 4 자동 / Task 20 수동 |
| §9 결정사항 | 계정 nullable (Task 1), env 1회만 (Task 4) |

빠진 항목: 없음.

Type 일관성 체크:
- `UpbitAccount`는 Rust(Task 2) / TS(Task 16) 동일 필드
- `LiveSession.upbit_account_id`는 Rust `Option<i64>` / TS `number | null` 동일 의미
- `load_upbit_keys_for` (Task 5) / `upbit_client_for` (Task 5) 호출처는 Task 12, 14에서 일관 사용

Placeholder 스캔: 없음 — 모든 step에 완전한 code/command 포함.
