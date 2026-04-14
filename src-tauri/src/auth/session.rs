use chrono::{Duration, Utc};
use rusqlite::Connection;
use uuid::Uuid;

/// Create a new session for a user, returning the session token. Expires in 24 hours.
pub fn create_session(conn: &Connection, user_id: i64) -> Result<String, String> {
    let token = Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + Duration::hours(24))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    conn.execute(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![token, user_id, expires_at],
    )
    .map_err(|e| format!("Failed to create session: {e}"))?;

    Ok(token)
}

/// Validate a session token. Returns Some(user_id) if valid and not expired.
pub fn validate_session(conn: &Connection, token: &str) -> Result<Option<i64>, String> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let result = conn.query_row(
        "SELECT user_id FROM sessions WHERE id = ?1 AND expires_at > ?2",
        rusqlite::params![token, now],
        |row| row.get::<_, i64>(0),
    );

    match result {
        Ok(user_id) => Ok(Some(user_id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Session validation error: {e}")),
    }
}

/// Delete a session (logout).
pub fn delete_session(conn: &Connection, token: &str) -> Result<(), String> {
    conn.execute("DELETE FROM sessions WHERE id = ?1", [token])
        .map_err(|e| format!("Failed to delete session: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let schema_v1 = include_str!("../../migrations/001_initial.sql");
        conn.execute_batch(schema_v1).unwrap();
        let schema_v2 = include_str!("../../migrations/002_users.sql");
        conn.execute_batch(schema_v2).unwrap();
        // Insert a test user
        conn.execute(
            "INSERT INTO users (username, password_hash, role) VALUES ('testuser', 'hash', 'trader')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_create_and_validate_session() {
        let conn = setup_db();
        let token = create_session(&conn, 1).unwrap();
        assert!(!token.is_empty());

        let user_id = validate_session(&conn, &token).unwrap();
        assert_eq!(user_id, Some(1));
    }

    #[test]
    fn test_invalid_token() {
        let conn = setup_db();
        let user_id = validate_session(&conn, "nonexistent-token").unwrap();
        assert_eq!(user_id, None);
    }

    #[test]
    fn test_delete_session() {
        let conn = setup_db();
        let token = create_session(&conn, 1).unwrap();
        assert!(validate_session(&conn, &token).unwrap().is_some());

        delete_session(&conn, &token).unwrap();
        assert!(validate_session(&conn, &token).unwrap().is_none());
    }
}
