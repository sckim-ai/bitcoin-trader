//! Phase 4A.7 — mode-toggle data integrity + kill-switch tests.
//!
//! count_real_sessions has been removed (replaced by per-account partial
//! unique index in Task 10). Tests that relied on it are replaced with
//! direct SQL COUNT queries so the invariants remain documented.

use bitcoin_trader_lib::db::live_repo::{
    insert_preset, insert_session, set_session_mode,
    set_session_status, stop_all_real_sessions,
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
    conn.execute_batch(include_str!("../migrations/012_order_caps.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/013_pending_cost_basis.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/014_upbit_accounts.sql")).unwrap();
    conn
}

fn count_real(conn: &Connection, exclude_id: Option<i64>) -> i64 {
    match exclude_id {
        Some(id) => conn.query_row(
            "SELECT COUNT(*) FROM live_sessions WHERE mode = 'real' AND id != ?1",
            [id],
            |r| r.get(0),
        ).unwrap(),
        None => conn.query_row(
            "SELECT COUNT(*) FROM live_sessions WHERE mode = 'real'",
            [],
            |r| r.get(0),
        ).unwrap(),
    }
}

fn make_session(conn: &Connection, label: &str) -> i64 {
    let pid = insert_preset(
        conn, 1, &format!("p_{label}"), "V3", "{}", "manual",
        None, None, None, None, None, None, None,
    ).unwrap();
    insert_session(conn, 1, label, pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-29T00:00:00Z", None).unwrap()
}

// ─── multi-real=1 invariant ────────────────────────────────────────────────

#[test]
fn fresh_db_has_zero_real_sessions() {
    let conn = setup_db();
    let _s1 = make_session(&conn, "A");
    let _s2 = make_session(&conn, "B");
    assert_eq!(count_real(&conn, None), 0);
}

#[test]
fn promoting_first_session_self_exclusion_returns_zero_others() {
    let conn = setup_db();
    let s1 = make_session(&conn, "A");
    set_session_mode(&conn, s1, "real").unwrap();
    assert_eq!(count_real(&conn, Some(s1)), 0);
    assert_eq!(count_real(&conn, None), 1);
}

#[test]
fn second_promotion_is_blocked_via_count() {
    let conn = setup_db();
    let s1 = make_session(&conn, "A");
    let s2 = make_session(&conn, "B");
    set_session_mode(&conn, s1, "real").unwrap();

    let other = count_real(&conn, Some(s2));
    assert!(other > 0, "second promotion must be blocked");
}

#[test]
fn demoting_releases_the_slot() {
    let conn = setup_db();
    let s1 = make_session(&conn, "A");
    let s2 = make_session(&conn, "B");
    set_session_mode(&conn, s1, "real").unwrap();
    set_session_mode(&conn, s1, "paper").unwrap();
    let other = count_real(&conn, Some(s2));
    assert_eq!(other, 0);
}

#[test]
fn stopped_real_sessions_still_count() {
    let conn = setup_db();
    let s1 = make_session(&conn, "A");
    let s2 = make_session(&conn, "B");
    set_session_mode(&conn, s1, "real").unwrap();
    set_session_status(&conn, s1, "running").unwrap();
    set_session_status(&conn, s1, "stopped").unwrap();
    let other = count_real(&conn, Some(s2));
    assert_eq!(other, 1, "stopped real still occupies the slot");
}

// ─── Kill switch / stop_all_real_sessions ──────────────────────────────────

#[test]
fn kill_switch_only_targets_running_real() {
    let conn = setup_db();
    let s_paper_run = make_session(&conn, "Pr");
    let s_real_run = make_session(&conn, "Rr");
    let s_real_stop = make_session(&conn, "Rs");
    set_session_status(&conn, s_paper_run, "running").unwrap();
    set_session_mode(&conn, s_real_run, "real").unwrap();
    set_session_status(&conn, s_real_run, "running").unwrap();
    set_session_mode(&conn, s_real_stop, "real").unwrap();

    let stopped = stop_all_real_sessions(&conn).unwrap();
    assert_eq!(stopped, vec![s_real_run], "only running real should stop");
}

#[test]
fn kill_switch_preserves_mode() {
    let conn = setup_db();
    let s = make_session(&conn, "R");
    set_session_mode(&conn, s, "real").unwrap();
    set_session_status(&conn, s, "running").unwrap();
    stop_all_real_sessions(&conn).unwrap();
    assert_eq!(count_real(&conn, None), 1);
}

// ─── Mode toggle data integrity ────────────────────────────────────────────

#[test]
fn toggling_does_not_alter_other_session_fields() {
    use bitcoin_trader_lib::db::live_repo::get_session;
    let conn = setup_db();
    let s = make_session(&conn, "A");
    let before = get_session(&conn, s).unwrap().unwrap();
    set_session_mode(&conn, s, "real").unwrap();
    let after = get_session(&conn, s).unwrap().unwrap();
    assert_eq!(after.mode, "real");
    assert_eq!(before.mode, "paper");
    assert_eq!(before.label, after.label);
    assert_eq!(before.preset_id, after.preset_id);
    assert_eq!(before.market, after.market);
    assert_eq!(before.initial_capital, after.initial_capital);
    assert_eq!(before.start_ts, after.start_ts);
    assert_eq!(before.real_started_at, after.real_started_at);
    assert_eq!(before.live_return, after.live_return);
    assert_eq!(before.max_daily_loss_pct, after.max_daily_loss_pct);
    assert_eq!(before.max_daily_trades, after.max_daily_trades);
}
