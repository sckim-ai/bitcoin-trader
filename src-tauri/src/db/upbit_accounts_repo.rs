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
                       WHERE ls.upbit_account_id = a.id AND ls.status = 'running'),
                a.discord_webhook_url
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
                       WHERE ls.upbit_account_id = a.id AND ls.status = 'running'),
                a.discord_webhook_url
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

/// 빈 문자열은 NULL로 정규화 (UI에서 input을 비워 저장하면 글로벌 fallback으로 돌아감).
pub fn set_discord_webhook(
    conn: &Connection,
    id: i64,
    url: Option<&str>,
) -> Result<usize> {
    let normalized: Option<&str> = url.map(|s| s.trim()).filter(|s| !s.is_empty());
    conn.execute(
        "UPDATE upbit_accounts SET discord_webhook_url = ?1 WHERE id = ?2",
        params![normalized, id],
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
        discord_webhook_url: row.get(6)?,
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

    #[test]
    fn discord_webhook_set_and_clear() {
        let conn = setup();
        let id = insert_account(&conn, 1, "Sub").unwrap();
        // 기본은 NULL → 글로벌 fallback.
        assert!(get_account(&conn, id).unwrap().unwrap().discord_webhook_url.is_none());

        // URL 저장.
        set_discord_webhook(&conn, id, Some("https://discord.com/api/webhooks/abc")).unwrap();
        assert_eq!(
            get_account(&conn, id).unwrap().unwrap().discord_webhook_url.as_deref(),
            Some("https://discord.com/api/webhooks/abc")
        );

        // 빈 문자열은 NULL로 정규화 (UI 입력 비우기 = 글로벌 fallback 복귀).
        set_discord_webhook(&conn, id, Some("   ")).unwrap();
        assert!(get_account(&conn, id).unwrap().unwrap().discord_webhook_url.is_none());
    }
}
