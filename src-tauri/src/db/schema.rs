use rusqlite::{Connection, Result};
use std::path::Path;

/// Initialize the SQLite database: open (or create) the file, enable WAL + foreign keys,
/// and run the initial schema migration.
pub fn initialize(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    let schema = include_str!("../../migrations/001_initial.sql");
    conn.execute_batch(schema)?;
    Ok(conn)
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
