//! 마이그레이션 014 + migrate_legacy_upbit_key의 DB 부분 검증.
//! keyring write는 dev/CI 환경에 따라 결과가 달라 통합 테스트가 어려우므로,
//! "옛 키/env가 없을 때 무동작" 케이스만 자동화한다. keyring 경로는 수동 검증.

use bitcoin_trader_lib::db::schema;

#[test]
fn clean_install_creates_no_account() {
    std::env::remove_var("UPBIT_ACCESS_KEY");
    std::env::remove_var("UPBIT_SECRET_KEY");

    let dir = std::env::temp_dir().join(format!("bt_mig_clean_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = schema::initialize(&dir.join("test.db")).unwrap();

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM upbit_accounts", [], |r| r.get::<_, i64>(0))
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
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(table_count, 1);

    let idx_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='index' AND name='idx_session_account_running_real'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(idx_count, 1);

    let col_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('live_sessions')
             WHERE name='upbit_account_id'",
            [],
            |r| r.get::<_, i64>(0),
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
    let _conn2 = schema::initialize(&path).unwrap();
}
