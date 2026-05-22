use rusqlite::{Connection, Result};
use std::path::Path;

/// Initialize the SQLite database: open (or create) the file, enable WAL + foreign keys,
/// and run all schema migrations.
pub fn initialize(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    let schema_v1 = include_str!("../../migrations/001_initial.sql");
    conn.execute_batch(schema_v1)?;
    let schema_v2 = include_str!("../../migrations/002_users.sql");
    conn.execute_batch(schema_v2)?;
    // 003: ALTER TABLE ADD COLUMN is not idempotent — ignore "duplicate column" error on repeat boot.
    let schema_v3 = include_str!("../../migrations/003_day_psy.sql");
    if let Err(e) = conn.execute_batch(schema_v3) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") {
            return Err(e);
        }
    }
    let schema_v4 = include_str!("../../migrations/004_opt_metrics.sql");
    if let Err(e) = conn.execute_batch(schema_v4) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") {
            return Err(e);
        }
    }
    let schema_v5 = include_str!("../../migrations/005_opt_indexes.sql");
    if let Err(e) = conn.execute_batch(schema_v5) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") {
            return Err(e);
        }
    }
    let schema_v6 = include_str!("../../migrations/006_live_trading.sql");
    if let Err(e) = conn.execute_batch(schema_v6) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v7 = include_str!("../../migrations/007_preset_context.sql");
    if let Err(e) = conn.execute_batch(schema_v7) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v8 = include_str!("../../migrations/008_session_signal_log.sql");
    if let Err(e) = conn.execute_batch(schema_v8) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v9 = include_str!("../../migrations/009_baseline_metrics.sql");
    if let Err(e) = conn.execute_batch(schema_v9) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v10 = include_str!("../../migrations/010_pending_orders.sql");
    if let Err(e) = conn.execute_batch(schema_v10) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v11 = include_str!("../../migrations/011_safety_limits.sql");
    if let Err(e) = conn.execute_batch(schema_v11) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v12 = include_str!("../../migrations/012_order_caps.sql");
    if let Err(e) = conn.execute_batch(schema_v12) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
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
    let schema_v15 = include_str!("../../migrations/015_account_discord_webhook.sql");
    if let Err(e) = conn.execute_batch(schema_v15) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v16 = include_str!("../../migrations/016_session_notify_discord.sql");
    if let Err(e) = conn.execute_batch(schema_v16) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    let schema_v17 = include_str!("../../migrations/017_session_notify_accounts.sql");
    if let Err(e) = conn.execute_batch(schema_v17) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
    // Backfill best_return cache for pre-migration runs so the listing
    // query works uniformly. Idempotent: only touches NULL rows.
    conn.execute(
        "UPDATE optimization_runs
         SET best_return = (
            SELECT MAX(total_return) FROM optimization_results WHERE run_id = optimization_runs.id
         )
         WHERE best_return IS NULL",
        [],
    )?;
    seed_admin(&conn)?;
    // FK on upbit_accounts.user_id needs users(1) to exist → must run AFTER seed_admin.
    migrate_legacy_upbit_key(&conn)?;
    Ok(conn)
}

/// Seed default admin user if not exists (password: admin123, argon2 hashed at build time).
fn seed_admin(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users WHERE username = 'admin'",
        [],
        |row| row.get(0),
    )?;
    if count == 0 {
        // Pre-computed argon2 hash — or compute at runtime
        let hash = crate::auth::password::hash_password("admin123")
            .unwrap_or_default();
        conn.execute(
            "INSERT INTO users (username, password_hash, role) VALUES ('admin', ?1, 'admin')",
            [&hash],
        )?;
    }
    Ok(())
}

/// Idempotent one-time migration: 기존 단일 keyring `upbit_access_key` /
/// `upbit_secret_key` (또는 env fallback) → `upbit_accounts(id=1, label='기본')`.
///
/// 순서:
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
            _ => return Ok(()),
        },
    };

    // Keyring errors converted to rusqlite::Error so ? composes.
    let to_err = |e: keyring::Error| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERNAL),
            Some(format!("keyring: {e}")),
        )
    };

    // 3. 새 위치 keyring 저장 — 두 항목 모두 성공해야 다음 단계로 진행.
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

    // 5. 옛 keyring 항목 삭제 (실패해도 치명적이지 않음)
    let _ = keyring::Entry::new(SERVICE, "upbit_access_key")
        .and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new(SERVICE, "upbit_secret_key")
        .and_then(|e| e.delete_credential());

    // 6. 기존 세션 백필
    conn.execute(
        "UPDATE live_sessions SET upbit_account_id = 1 WHERE upbit_account_id IS NULL",
        [],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_in_memory() {
        // Use a temp file since include_str schema needs real connection
        let dir = std::env::temp_dir().join("bitcoin_trader_test_schema");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.db");
        let _ = std::fs::remove_file(&db_path);

        let conn = initialize(&db_path).expect("should initialize");

        // Verify tables exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='market_data'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='trades'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let _ = std::fs::remove_file(&db_path);
    }
}
