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
