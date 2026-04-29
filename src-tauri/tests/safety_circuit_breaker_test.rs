//! Phase 4A.7 — circuit breaker (daily loss / trade count) regression guards.
//!
//! Validates that the SQL-level aggregation matches what session_engine
//! reads at the start of each real cycle. The aggregation is the single
//! source of truth for whether the auto-stop fires; if these queries
//! drift, the breaker silently stops working.

use bitcoin_trader_lib::db::live_repo::{
    insert_preset, insert_session, insert_trade,
    today_real_trades_count, today_realized_pnl_pct,
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

fn make_session(conn: &Connection) -> i64 {
    let pid = insert_preset(
        conn, 1, "p", "V3", "{}", "manual",
        None, None, None, None, None, None, None,
    ).unwrap();
    insert_session(conn, 1, "S", pid, "KRW-ETH", "real", 1_000_000.0, "2026-04-29T00:00:00Z").unwrap()
}

/// SQLite's `date('now')` returns the current UTC date. We craft trade
/// timestamps with that prefix so the aggregation actually matches them
/// regardless of when the test runs.
fn today_ts_at(hour: u32, min: u32) -> String {
    let conn = Connection::open_in_memory().unwrap();
    let today: String = conn.query_row("SELECT date('now')", [], |r| r.get(0)).unwrap();
    format!("{}T{:02}:{:02}:00Z", today, hour, min)
}

fn yesterday_ts() -> String {
    let conn = Connection::open_in_memory().unwrap();
    let y: String = conn.query_row("SELECT date('now', '-1 day')", [], |r| r.get(0)).unwrap();
    format!("{}T12:00:00Z", y)
}

// ─── today_realized_pnl_pct ────────────────────────────────────────────────

#[test]
fn empty_history_returns_zero() {
    let conn = setup_db();
    let sid = make_session(&conn);
    let pct = today_realized_pnl_pct(&conn, sid, 1_000_000.0).unwrap();
    assert!((pct - 0.0).abs() < 1e-9);
}

#[test]
fn single_loss_today_drops_below_threshold() {
    let conn = setup_db();
    let sid = make_session(&conn);
    // Insert a paired buy + sell with -5,000 KRW pnl (today).
    insert_trade(&conn, sid, &today_ts_at(9, 0), "buy",
        3_000_000.0, 0.1, 150.0, "real_buy", None, None, true).unwrap();
    insert_trade(&conn, sid, &today_ts_at(10, 0), "sell",
        2_950_000.0, 0.1, 147.5, "real_sell",
        Some(-5_000.0), Some(-1.667), true).unwrap();

    // -5,000 / 1,000,000 × 100 = -0.5%
    let pct = today_realized_pnl_pct(&conn, sid, 1_000_000.0).unwrap();
    assert!((pct - (-0.5)).abs() < 0.01);
}

#[test]
fn yesterdays_pnl_is_excluded() {
    let conn = setup_db();
    let sid = make_session(&conn);
    insert_trade(&conn, sid, &yesterday_ts(), "sell",
        3_000_000.0, 0.1, 150.0, "real_sell",
        Some(-50_000.0), Some(-5.0), true).unwrap();
    // Today: nothing → today_pnl = 0 even though yesterday lost 5%.
    let pct = today_realized_pnl_pct(&conn, sid, 1_000_000.0).unwrap();
    assert!((pct - 0.0).abs() < 1e-9);
}

#[test]
fn paper_trades_are_excluded_from_real_breaker() {
    let conn = setup_db();
    let sid = make_session(&conn);
    // is_real=false should NOT count toward the circuit breaker.
    insert_trade(&conn, sid, &today_ts_at(9, 0), "sell",
        2_900_000.0, 0.1, 145.0, "paper_sell",
        Some(-100_000.0), Some(-10.0), false).unwrap();
    let pct = today_realized_pnl_pct(&conn, sid, 1_000_000.0).unwrap();
    assert!((pct - 0.0).abs() < 1e-9);
}

#[test]
fn breaker_threshold_comparison_correctness() {
    let conn = setup_db();
    let sid = make_session(&conn);
    // Two sells totalling -110,000 KRW pnl → -11%.
    insert_trade(&conn, sid, &today_ts_at(9, 0), "sell",
        3_000_000.0, 0.1, 150.0, "real_sell",
        Some(-50_000.0), Some(-5.0), true).unwrap();
    insert_trade(&conn, sid, &today_ts_at(11, 0), "sell",
        3_000_000.0, 0.1, 150.0, "real_sell",
        Some(-60_000.0), Some(-6.0), true).unwrap();

    let pct = today_realized_pnl_pct(&conn, sid, 1_000_000.0).unwrap();
    assert!((pct - (-11.0)).abs() < 0.01);
    // The session_engine guard: today_pnl_pct <= max_daily_loss_pct.
    // With max=-10.0, -11.0 <= -10.0 → tripped.
    let max = -10.0;
    assert!(pct <= max, "should trip the circuit breaker");
    // With max=-15.0, -11.0 > -15.0 → not tripped.
    let max_lenient = -15.0;
    assert!(pct > max_lenient, "should NOT trip with lenient threshold");
}

// ─── today_real_trades_count ───────────────────────────────────────────────

#[test]
fn count_excludes_buys_and_paper() {
    let conn = setup_db();
    let sid = make_session(&conn);
    insert_trade(&conn, sid, &today_ts_at(9, 0), "buy",
        3_000_000.0, 0.1, 150.0, "real_buy", None, None, true).unwrap();
    insert_trade(&conn, sid, &today_ts_at(10, 0), "sell",
        3_100_000.0, 0.1, 155.0, "paper_sell",
        Some(10_000.0), Some(3.33), false).unwrap();
    insert_trade(&conn, sid, &today_ts_at(11, 0), "sell",
        3_050_000.0, 0.1, 152.5, "real_sell",
        Some(5_000.0), Some(1.67), true).unwrap();
    insert_trade(&conn, sid, &today_ts_at(12, 0), "sell",
        3_080_000.0, 0.1, 154.0, "real_sell",
        Some(8_000.0), Some(2.67), true).unwrap();

    // Only the two real sells count.
    assert_eq!(today_real_trades_count(&conn, sid).unwrap(), 2);
}

#[test]
fn count_excludes_yesterday() {
    let conn = setup_db();
    let sid = make_session(&conn);
    for _ in 0..5 {
        insert_trade(&conn, sid, &yesterday_ts(), "sell",
            3_000_000.0, 0.1, 150.0, "real_sell",
            Some(1_000.0), Some(0.33), true).unwrap();
    }
    assert_eq!(today_real_trades_count(&conn, sid).unwrap(), 0);
}

#[test]
fn count_threshold_comparison() {
    let conn = setup_db();
    let sid = make_session(&conn);
    for i in 0..20 {
        insert_trade(&conn, sid, &today_ts_at(10, i), "sell",
            3_000_000.0, 0.001, 1.5, "real_sell",
            Some(0.0), Some(0.0), true).unwrap();
    }
    let n = today_real_trades_count(&conn, sid).unwrap();
    assert_eq!(n, 20);
    // session_engine guard: today_count >= max_daily_trades.
    // With max=20, 20 >= 20 → tripped.
    assert!(n >= 20);
    // With max=21, 20 < 21 → not tripped.
    assert!(n < 21);
}
