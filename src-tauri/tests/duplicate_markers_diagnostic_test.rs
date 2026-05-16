//! Diagnostic test for the "live chart shows duplicate buy/sell labels when
//! multiple sessions exist" report.
//!
//! Hypothesis chain we're verifying:
//!   (A) `Strategy::run_simulation` is deterministic — same data + same params
//!       produce byte-identical TradeRecords (same timestamps + pnl_pct).
//!   (B) `live_repo::list_trades` returns each session's own trades; identical
//!       (ts, pnl_pct) tuples persisted under different session_ids do NOT
//!       collide or dedupe — the DB faithfully holds N copies for N sessions.
//!
//! Together (A) + (B) explain why two sessions sharing one preset cause the
//! candle chart to draw 2 sets of (label + pnl) markers stacked at the same
//! (time, position) — see also `src/components/live/duplicateMarkers.test.ts`
//! which proves the marker-construction half of the chain.

use bitcoin_trader_lib::db::live_repo;
use bitcoin_trader_lib::models::market::{Candle, IndicatorSet, MarketData};
use bitcoin_trader_lib::models::trading::TradingParameters;
use bitcoin_trader_lib::strategies::{regime_adaptive_v31::RegimeAdaptiveV31Strategy, Strategy};
use chrono::{Duration, TimeZone, Utc};
use rusqlite::Connection;

fn make_candle(ts_hour: i64, close: f64, volume: f64, rsi: f64, psy: f64) -> MarketData {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + Duration::hours(ts_hour);
    MarketData {
        candle: Candle {
            timestamp: ts,
            open: close,
            high: close * 1.01,
            low: close * 0.99,
            close,
            volume,
        },
        indicators: IndicatorSet {
            rsi,
            psy_day: psy,
            ..Default::default()
        },
    }
}

/// Synthetic data that reliably produces at least one completed buy → sell
/// pair on V3.1, mirroring the scaffold in `v31_strategy_test.rs`. Returns
/// 200 hourly bars with a trade-value spike at index 100.
fn synthetic_series_with_a_sell() -> Vec<MarketData> {
    let mut data: Vec<MarketData> = (0..100)
        .map(|i| make_candle(i as i64, 4_000_000.0, 100.0, 50.0, -0.3))
        .collect();
    // spike bar — fires urgent_buy
    data.push(make_candle(100, 3_720_000.0, 100_000.0, 30.0, -0.3));
    // recovery + price drift up so the position eventually closes for profit
    for i in 1..100 {
        data.push(make_candle(100 + i, 4_200_000.0 + (i as f64) * 1_000.0, 200.0, 55.0, 0.1));
    }
    data
}

fn make_params() -> TradingParameters {
    let mut p = TradingParameters::default();
    p.v31_min_hold_bars = 3; // shorten so the sell completes inside the window
    p
}

// ─── (A) Determinism — two cycles with same input emit identical trades ──

#[test]
fn same_preset_same_data_produces_identical_trades() {
    let data = synthetic_series_with_a_sell();
    let params = make_params();
    let strategy = RegimeAdaptiveV31Strategy;

    let r1 = strategy.run_simulation(&data, &params);
    let r2 = strategy.run_simulation(&data, &params);

    assert!(
        !r1.trades.is_empty(),
        "synthetic series must produce at least one completed trade for the test to be meaningful"
    );
    assert_eq!(r1.trades.len(), r2.trades.len(), "trade counts must match");

    for (i, (a, b)) in r1.trades.iter().zip(r2.trades.iter()).enumerate() {
        assert_eq!(a.buy_timestamp, b.buy_timestamp, "trade #{i} buy_timestamp mismatch");
        assert_eq!(a.sell_timestamp, b.sell_timestamp, "trade #{i} sell_timestamp mismatch");
        assert_eq!(a.buy_price, b.buy_price, "trade #{i} buy_price mismatch");
        assert_eq!(a.sell_price, b.sell_price, "trade #{i} sell_price mismatch");
        assert_eq!(a.pnl_pct, b.pnl_pct, "trade #{i} pnl_pct mismatch");
    }

    // Two distinct sessions running this same preset would each pass these
    // identical TradeRecords through `live_repo::insert_trade(..., session_id, ...)`,
    // ending up with N row-sets — one per session — sharing every byte of
    // (ts, pnl_pct). That's the engine-side cause of the chart duplication.
}

// ─── (B) Per-session storage holds duplicates without dedup ──────────────

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

#[test]
fn two_sessions_same_label_each_keep_their_own_identical_sell_row() {
    let conn = setup_db();

    // Single preset, simulating "Short_156"
    let preset_id = live_repo::insert_preset(
        &conn, 1, "Short_156", "V3", "{}", "manual",
        None, None, None, None, None, None, None,
    ).unwrap();

    // Two sessions sharing the preset AND the default-from-preset label.
    // (NewSessionDialog defaults `label = preset.name` until the user types.)
    let s1 = live_repo::insert_session(
        &conn, 1, "Short_156", preset_id, "KRW-ETH", "paper",
        1_000_000.0, "2026-04-24T00:00:00Z", None,
    ).unwrap();
    let s2 = live_repo::insert_session(
        &conn, 1, "Short_156", preset_id, "KRW-ETH", "paper",
        1_000_000.0, "2026-04-24T00:00:00Z", None,
    ).unwrap();
    assert_ne!(s1, s2, "sessions must have distinct ids");

    // Mirror what `run_session_cycle` does: each session writes the SAME
    // simulation output (same buy/sell ts + pnl_pct) under its own session_id.
    let buy_ts = "2026-04-24T05:00:00Z";
    let sell_ts = "2026-04-24T08:00:00Z";
    let pnl_pct = 0.0154; // matches the screenshot's "+1.54%"

    for sid in [s1, s2] {
        live_repo::insert_trade(
            &conn, sid, buy_ts, "buy", 4_000_000.0, 0.25, 500.0,
            "buy", None, None, false,
        ).unwrap();
        live_repo::insert_trade(
            &conn, sid, sell_ts, "sell", 4_061_600.0, 0.25, 507.7,
            "sell", Some(0.0154 * 1_000_000.0), Some(pnl_pct), false,
        ).unwrap();
    }

    // List per-session — both sessions return their OWN sell with the
    // identical timestamp + pnl_pct. The DB does not dedupe across session_id.
    let t1 = live_repo::list_trades(&conn, s1).unwrap();
    let t2 = live_repo::list_trades(&conn, s2).unwrap();

    let sell1 = t1.iter().find(|t| t.side == "sell").expect("session 1 sell row");
    let sell2 = t2.iter().find(|t| t.side == "sell").expect("session 2 sell row");

    assert_eq!(sell1.ts, sell_ts);
    assert_eq!(sell2.ts, sell_ts);
    assert_eq!(sell1.pnl_pct, Some(pnl_pct));
    assert_eq!(sell2.pnl_pct, Some(pnl_pct));

    // The frontend builds tradesBySession = { [s1]: t1, [s2]: t2 } and the
    // marker loop iterates BOTH session arrays. With identical (ts, pnl_pct,
    // label), the chart receives 4 markers stacked at one (time, position)
    // — see duplicateMarkers.test.ts for that half of the proof.
}
