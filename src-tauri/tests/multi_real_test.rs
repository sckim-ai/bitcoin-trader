//! Phase 4A.7 — multi-real=1 invariant + mode-toggle data integrity.
//!
//! These tests validate the DB-layer building blocks the
//! `toggle_session_mode` Tauri command depends on. The command itself wraps
//! `count_real_sessions(exclude=self) > 0 → reject` + key-presence check,
//! both of which are simple match arms in the command body — the
//! invariants are owned by the repo functions tested here.

use bitcoin_trader_lib::db::live_repo::{
    count_real_sessions, insert_preset, insert_session, set_session_mode,
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
    conn
}

fn make_session(conn: &Connection, label: &str) -> i64 {
    let pid = insert_preset(
        conn, 1, &format!("p_{label}"), "V3", "{}", "manual",
        None, None, None, None, None, None, None,
    ).unwrap();
    insert_session(conn, 1, label, pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-29T00:00:00Z").unwrap()
}

// ─── multi-real=1 invariant ────────────────────────────────────────────────

#[test]
fn fresh_db_has_zero_real_sessions() {
    let conn = setup_db();
    let _s1 = make_session(&conn, "A");
    let _s2 = make_session(&conn, "B");
    assert_eq!(count_real_sessions(&conn, None).unwrap(), 0);
}

#[test]
fn promoting_first_session_self_exclusion_returns_zero_others() {
    let conn = setup_db();
    let s1 = make_session(&conn, "A");
    set_session_mode(&conn, s1, "real").unwrap();
    // The command logic: count_real_sessions(exclude=self) — should be 0 → allow.
    assert_eq!(count_real_sessions(&conn, Some(s1)).unwrap(), 0);
    assert_eq!(count_real_sessions(&conn, None).unwrap(), 1);
}

#[test]
fn second_promotion_is_blocked_via_count() {
    // Simulates the toggle_session_mode command's check:
    //   if count_real_sessions(exclude=candidate) > 0 → reject
    let conn = setup_db();
    let s1 = make_session(&conn, "A");
    let s2 = make_session(&conn, "B");
    set_session_mode(&conn, s1, "real").unwrap();

    // candidate = s2; "any OTHER real?" = 1 → block.
    let other = count_real_sessions(&conn, Some(s2)).unwrap();
    assert!(other > 0, "second promotion must be blocked");
}

#[test]
fn demoting_releases_the_slot() {
    // Demote s1 → paper, then s2 should be promotable.
    let conn = setup_db();
    let s1 = make_session(&conn, "A");
    let s2 = make_session(&conn, "B");
    set_session_mode(&conn, s1, "real").unwrap();
    // demote
    set_session_mode(&conn, s1, "paper").unwrap();
    // now s2 can promote
    let other = count_real_sessions(&conn, Some(s2)).unwrap();
    assert_eq!(other, 0);
}

#[test]
fn stopped_real_sessions_still_count() {
    // CRITICAL: a 'stopped' real session occupies the slot — its prior
    // position state is still on Upbit. multi-real=1 must NOT be relaxed
    // for stopped sessions, otherwise two real sessions could both think
    // the same coin balance is theirs.
    let conn = setup_db();
    let s1 = make_session(&conn, "A");
    let s2 = make_session(&conn, "B");
    set_session_mode(&conn, s1, "real").unwrap();
    set_session_status(&conn, s1, "running").unwrap();
    set_session_status(&conn, s1, "stopped").unwrap();
    // s1 is stopped but mode='real'. count_real_sessions doesn't filter on
    // status — slot still occupied.
    let other = count_real_sessions(&conn, Some(s2)).unwrap();
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
    // s_real_stop stays 'stopped'

    let stopped = stop_all_real_sessions(&conn).unwrap();
    assert_eq!(stopped, vec![s_real_run], "only running real should stop");
}

#[test]
fn kill_switch_preserves_mode() {
    // Mode='real' must survive emergency stop — the user explicitly chose
    // it; silent demote would be surprising.
    let conn = setup_db();
    let s = make_session(&conn, "R");
    set_session_mode(&conn, s, "real").unwrap();
    set_session_status(&conn, s, "running").unwrap();
    stop_all_real_sessions(&conn).unwrap();
    // Mode preserved
    assert_eq!(count_real_sessions(&conn, None).unwrap(), 1);
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
    // Only mode changes; everything else preserved.
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
