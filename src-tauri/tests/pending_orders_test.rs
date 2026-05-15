//! Phase 4A.5 — pending_orders repo lifecycle.
//!
//! No live network calls. Validates the DB schema + insert/list/update
//! transitions that the tracker depends on.

use bitcoin_trader_lib::db::live_repo::{
    insert_pending_order, insert_preset, insert_session, list_pending_wait,
    mark_pending_resolved, touch_pending_check,
};
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    conn.execute_batch(include_str!("../migrations/001_initial.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/002_users.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/006_live_trading.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/007_preset_context.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/008_session_signal_log.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/009_baseline_metrics.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/010_pending_orders.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/011_safety_limits.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/013_pending_cost_basis.sql")).unwrap();
    conn
}

fn make_session(conn: &Connection) -> i64 {
    let pid = insert_preset(
        conn, 1, "p", "V3", "{}", "manual",
        None, None, None, None, None, None, None,
    ).unwrap();
    insert_session(conn, 1, "S", pid, "KRW-ETH", "real", 1e6, "2026-04-29T00:00:00Z").unwrap()
}

#[test]
fn insert_then_list_picks_up_wait_only() {
    let conn = setup_db();
    let sid = make_session(&conn);
    insert_pending_order(
        &conn, "u-wait", sid, "bid", "KRW-ETH", "price", None, 50_000.0,
        "2026-04-29T00:00:00Z", "wait", None,
    ).unwrap();
    insert_pending_order(
        &conn, "u-done", sid, "ask", "KRW-ETH", "market", None, 0.05,
        "2026-04-29T00:00:00Z", "done", Some(3_000_000.0),
    ).unwrap();
    let wait = list_pending_wait(&conn).unwrap();
    assert_eq!(wait.len(), 1);
    assert_eq!(wait[0].uuid, "u-wait");
    assert_eq!(wait[0].side, "bid");
    assert_eq!(wait[0].status, "wait");
    assert_eq!(wait[0].cost_basis_price, None);
}

#[test]
fn mark_done_drops_from_wait_list() {
    let conn = setup_db();
    let sid = make_session(&conn);
    insert_pending_order(
        &conn, "u-1", sid, "bid", "KRW-ETH", "price", None, 50_000.0,
        "2026-04-29T00:00:00Z", "wait", None,
    ).unwrap();
    let n = mark_pending_resolved(&conn, "u-1", "done", "2026-04-29T00:01:00Z").unwrap();
    assert_eq!(n, 1);
    assert_eq!(list_pending_wait(&conn).unwrap().len(), 0);
}

#[test]
fn touch_pending_check_updates_timestamp_only() {
    let conn = setup_db();
    let sid = make_session(&conn);
    insert_pending_order(
        &conn, "u-1", sid, "bid", "KRW-ETH", "price", None, 50_000.0,
        "2026-04-29T00:00:00Z", "wait", None,
    ).unwrap();
    let n = touch_pending_check(&conn, "u-1", "2026-04-29T00:30:00Z").unwrap();
    assert_eq!(n, 1);
    let wait = list_pending_wait(&conn).unwrap();
    assert_eq!(wait.len(), 1); // still wait
    assert_eq!(wait[0].last_checked.as_deref(), Some("2026-04-29T00:30:00Z"));
}

#[test]
fn upsert_replaces_status_on_conflict() {
    // Same uuid inserted twice — second insert (e.g. retry) should update
    // status without violating PK.
    let conn = setup_db();
    let sid = make_session(&conn);
    insert_pending_order(
        &conn, "u-1", sid, "bid", "KRW-ETH", "price", None, 50_000.0,
        "2026-04-29T00:00:00Z", "wait", None,
    ).unwrap();
    insert_pending_order(
        &conn, "u-1", sid, "bid", "KRW-ETH", "price", None, 50_000.0,
        "2026-04-29T00:00:00Z", "done", None,
    ).unwrap();
    assert_eq!(list_pending_wait(&conn).unwrap().len(), 0);
}

#[test]
fn cascade_delete_removes_pending_orders() {
    use bitcoin_trader_lib::db::live_repo::delete_session;
    let conn = setup_db();
    let sid = make_session(&conn);
    insert_pending_order(
        &conn, "u-1", sid, "bid", "KRW-ETH", "price", None, 50_000.0,
        "2026-04-29T00:00:00Z", "wait", None,
    ).unwrap();
    delete_session(&conn, sid).unwrap();
    assert_eq!(list_pending_wait(&conn).unwrap().len(), 0);
}
