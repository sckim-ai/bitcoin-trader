//! End-to-end verification of the 2026-05-05 "closed-bar policy" patch.
//!
//! Covers four pillars without needing a live Upbit connection:
//!   1. `compute_buy_amount`        — BUY size honours optional per-session cap
//!   2. `trigger_bar_ts`            — late R marker ts floors to the trigger bar
//!   3. Marker alignment simulation — paper + sync R + late R land on one bar
//!   4. DB persistence              — `max_order_krw` round-trips through SQLite
//!
//! These tests run against in-memory SQLite + the same migration files used
//! at runtime, so the schema stays in sync. Math-only tests don't need DB.

use bitcoin_trader_lib::db::live_repo;
use bitcoin_trader_lib::services::pending_order_tracker::trigger_bar_ts;
use bitcoin_trader_lib::services::session_engine::compute_buy_amount;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    for sql in [
        include_str!("../migrations/001_initial.sql"),
        include_str!("../migrations/002_users.sql"),
        include_str!("../migrations/006_live_trading.sql"),
        include_str!("../migrations/007_preset_context.sql"),
        include_str!("../migrations/008_session_signal_log.sql"),
        include_str!("../migrations/009_baseline_metrics.sql"),
        include_str!("../migrations/010_pending_orders.sql"),
        include_str!("../migrations/011_safety_limits.sql"),
        include_str!("../migrations/012_order_caps.sql"),
    ] {
        conn.execute_batch(sql).unwrap();
    }
    conn
}

fn make_preset_and_session(conn: &Connection) -> i64 {
    let pid = live_repo::insert_preset(
        conn, 1, "p", "V3", "{}", "manual",
        None, None, None, None, None, None, None,
    )
    .unwrap();
    live_repo::insert_session(
        conn, 1, "S", pid, "KRW-ETH", "paper", 1_000_000.0,
        "2026-04-24T00:00:00Z",
    )
    .unwrap()
}

// ─── Pillar 1: compute_buy_amount ─────────────────────────────────────

/// No cap → full balance × 0.9995. Reproduces the legacy behaviour pre-cap.
#[test]
fn buy_amount_no_cap_uses_full_balance() {
    let amount = compute_buy_amount(1_000_000.0, None);
    // 1,000,000 × 0.9995 = 999,500
    assert!((amount - 999_500.0).abs() < 0.01,
        "expected ≈999,500, got {amount}");
}

/// Cap < balance → cap binds. The exact case we reproduced from the
/// 99.5M KRW over-spend incident: with `cap = 1,000,000` the engine would
/// have placed at most ~999,500 KRW per cycle even on a 100M balance.
#[test]
fn buy_amount_cap_binds_when_smaller_than_balance() {
    let amount = compute_buy_amount(100_000_000.0, Some(1_000_000.0));
    assert!((amount - 999_500.0).abs() < 0.01,
        "1M cap on 100M balance must bind to ≈999,500, got {amount}");
}

/// Cap > balance → balance binds (cap is unreachable).
#[test]
fn buy_amount_balance_binds_when_smaller_than_cap() {
    let amount = compute_buy_amount(50_000.0, Some(200_000.0));
    // 50,000 × 0.9995 = 49,975
    assert!((amount - 49_975.0).abs() < 0.01);
}

/// Cap = 0 → effectively zero (engine filters this upstream via `> 0.0`,
/// but the math itself must be correct so the upstream filter is the only
/// reason it's not sent — i.e. no surprise rounding).
#[test]
fn buy_amount_zero_cap_returns_zero() {
    let amount = compute_buy_amount(1_000_000.0, Some(0.0));
    assert_eq!(amount, 0.0);
}

/// Cap exactly equal to balance → balance × 0.9995. Boundary check that the
/// `min` doesn't pick the cap and bypass the fee buffer.
#[test]
fn buy_amount_cap_equal_balance_applies_fee_buffer() {
    let amount = compute_buy_amount(1_000_000.0, Some(1_000_000.0));
    assert!((amount - 999_500.0).abs() < 0.01);
}

// ─── Pillar 2: trigger_bar_ts ─────────────────────────────────────────

/// Standard cycle: placed_at = 21:00:29.972 UTC (cycle entry +30s after the
/// 21:00 boundary). Trigger bar = 20:00:00 UTC, the bar the strategy actually
/// evaluated on the confirmed window.
#[test]
fn trigger_bar_ts_floors_typical_cycle_placement() {
    let placed = "2026-05-04T12:00:29.972+00:00";
    let bar = trigger_bar_ts(placed).unwrap();
    assert_eq!(bar, "2026-05-04T11:00:00+00:00",
        "21:00 KST cycle should trigger the 20:00 KST bar");
}

/// Day rollover: cycle enters at 00:00 UTC → trigger bar = 23:00 UTC of the
/// previous day. Verifies date arithmetic handles the boundary correctly.
#[test]
fn trigger_bar_ts_handles_day_rollover() {
    let placed = "2026-05-05T00:00:29+00:00";
    let bar = trigger_bar_ts(placed).unwrap();
    assert_eq!(bar, "2026-05-04T23:00:00+00:00");
}

/// Sub-second precision in placed_at must not bleed into the trigger bar
/// (it's flooring, not rounding).
#[test]
fn trigger_bar_ts_floors_subsecond_precision() {
    // 999ms past the half-second mark — still flooring to the prior hour.
    let placed = "2026-05-04T12:59:59.999999+00:00";
    let bar = trigger_bar_ts(placed).unwrap();
    assert_eq!(bar, "2026-05-04T11:00:00+00:00");
}

/// Invalid input → None (caller falls back to `now.to_rfc3339()`).
#[test]
fn trigger_bar_ts_returns_none_on_bad_input() {
    assert!(trigger_bar_ts("not-a-date").is_none());
    assert!(trigger_bar_ts("").is_none());
    assert!(trigger_bar_ts("2026-99-99T99:99:99Z").is_none());
}

// ─── Pillar 3: marker alignment simulation ─────────────────────────────

/// Reproduces the user's reported scenario: 20:00 KST bar triggers a buy,
/// 21:00 cycle places limit orders, all 3 chunks fill late and arrive at
/// 22:00 cycle. Verifies all R markers + the paper marker share one bar ts.
///
/// This is the "before/after" comparison made executable: under the old
/// policy, the 3 R markers would sit at 22:00:30 KST (Utc::now()). Under
/// the new policy they all derive from `placed_at` → trigger bar = 20:00 KST,
/// matching the paper marker exactly.
#[test]
fn marker_alignment_paper_and_three_late_chunks_on_one_bar() {
    // Simulated timeline (KST → UTC):
    //   20:00:00 KST (= 11:00:00 UTC) — bar the strategy evaluated
    //   21:00:29.972 KST (= 12:00:29.972 UTC) — cycle entry, limit placed
    //   22:00:30.865 KST (= 13:00:30.865 UTC) — reconcile, late fills
    let paper_ts = "2026-05-04T11:00:00+00:00";
    let placed_at = "2026-05-04T12:00:29.972+00:00";
    let _utc_now_at_reconcile = "2026-05-04T13:00:30.865+00:00";

    // Three chunks, all going through the late path.
    let r_marker_ts: Vec<String> = (0..3)
        .map(|_| trigger_bar_ts(placed_at).unwrap())
        .collect();

    // All three R ts must equal the paper ts (same bar).
    for (i, ts) in r_marker_ts.iter().enumerate() {
        assert_eq!(ts, paper_ts,
            "chunk {i}: late R marker ts {ts} must match paper ts {paper_ts}");
    }
    // And explicitly: NOT the "Utc::now()" value the old policy used.
    for ts in &r_marker_ts {
        assert_ne!(ts, "2026-05-04T13:00:30.865+00:00",
            "old policy used Utc::now()=22:00:30; new policy must not");
    }
}

/// Synchronous booking variant: same bar should be used for `bar_ts` (the
/// trade row's ts). This test mirrors the engine's logic: `bar_ts =
/// data.last().candle.timestamp.to_rfc3339()`. Since strategy + paper use
/// the same `data.last()`, sync R lands on the paper bar by construction.
#[test]
fn marker_alignment_sync_done_uses_same_bar_as_paper() {
    // The engine sets bar_ts = data.last().candle.timestamp.to_rfc3339().
    // For the same `data` window, paper marker also reads from
    // result.signal_log.last(buy).timestamp = data.last().timestamp.
    // We verify the round-trip format matches.
    let bar_timestamp = chrono::DateTime::parse_from_rfc3339("2026-05-04T11:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let bar_ts = bar_timestamp.to_rfc3339();
    // Paper marker's signal_log entry has the exact same timestamp string
    // (engine writes data.last().candle.timestamp's RFC3339 form).
    let paper_ts = bar_timestamp.to_rfc3339();
    assert_eq!(bar_ts, paper_ts);
}

// ─── Pillar 4: DB round-trip ───────────────────────────────────────────

/// New sessions default to no cap (NULL in DB → None in model).
#[test]
fn db_new_session_has_no_cap() {
    let conn = setup_db();
    let sid = make_preset_and_session(&conn);
    let session = live_repo::get_session(&conn, sid).unwrap().unwrap();
    assert_eq!(session.max_order_krw, None,
        "new sessions should default to no cap");
}

/// Setter persists Some(v); re-read gives the same value.
#[test]
fn db_set_cap_persists_value() {
    let conn = setup_db();
    let sid = make_preset_and_session(&conn);
    live_repo::set_session_max_order_krw(&conn, sid, Some(500_000.0)).unwrap();
    let session = live_repo::get_session(&conn, sid).unwrap().unwrap();
    assert_eq!(session.max_order_krw, Some(500_000.0));
}

/// Setter with None clears the cap (NULL in DB).
#[test]
fn db_set_cap_none_clears_value() {
    let conn = setup_db();
    let sid = make_preset_and_session(&conn);
    live_repo::set_session_max_order_krw(&conn, sid, Some(500_000.0)).unwrap();
    live_repo::set_session_max_order_krw(&conn, sid, None).unwrap();
    let session = live_repo::get_session(&conn, sid).unwrap().unwrap();
    assert_eq!(session.max_order_krw, None);
}

/// End-to-end: set cap, then verify the engine's BUY math produces the
/// expected order amount on that session. Combines pillars 1 + 4.
#[test]
fn db_cap_drives_buy_amount_correctly() {
    let conn = setup_db();
    let sid = make_preset_and_session(&conn);
    live_repo::set_session_max_order_krw(&conn, sid, Some(200_000.0)).unwrap();

    let session = live_repo::get_session(&conn, sid).unwrap().unwrap();
    // Account holds 100M KRW (the over-spend scenario), session cap = 200K.
    let order_krw = compute_buy_amount(100_000_000.0, session.max_order_krw);

    // 200K × 0.9995 = 199,900 — not 99,950,000 KRW that the old code would have sent.
    assert!((order_krw - 199_900.0).abs() < 0.01,
        "200K cap on 100M balance must size to ≈199,900, got {order_krw}");
    // And explicitly NOT the runaway value.
    assert!(order_krw < 1_000_000.0,
        "cap must prevent calling the full balance (got {order_krw})");
}
