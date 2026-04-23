//! V3 RegimeAdaptive — runs the Rust V3 engine with the **exact** values
//! from legacy `TradingConfig_V3_RegimeAdaptive_20260401_140149.json` on
//! **ETH/hour** data (the legacy project traded ETH exclusively, so the
//! JSON thresholds were calibrated to ETH volume/price distribution).
//!
//! After the C-1 engine realignment, the Rust V3 engine interprets those
//! values with the same multiplier semantics as the legacy C# engine:
//!   * `prev × price_drop_ratio > curr` for buy entry
//!   * `curr < buy × stop_loss_ratio` for volume-based stop loss
//!   * `curr > buy × profit_ratio` for urgent sell
//!   * fixed_sl stays percent-based
//!
//! This test is diagnostic (prints full metrics) rather than assertion-
//! heavy, so that a human can sanity-check the numbers against any future
//! legacy-replay run.

use bitcoin_trader_lib::core::{day_psy_store, indicators};
use bitcoin_trader_lib::migration::csv_import;
use bitcoin_trader_lib::models::market::MarketData;
use bitcoin_trader_lib::models::trading::{SimulationResult, TradingParameters};
use bitcoin_trader_lib::strategies::StrategyRegistry;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn db_path() -> Option<PathBuf> {
    let home = std::env::var("LOCALAPPDATA").ok()?;
    let p = PathBuf::from(home).join("bitcoin-trader").join("bitcoin_trader.db");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn load_market_hour(market: &str) -> Option<Vec<MarketData>> {
    let path = db_path()?;
    let conn = Connection::open(&path).ok()?;
    // Clean pipeline: load hour candles + persisted day_psy from DB and let
    // `day_psy_store::load_market_data` assemble MarketData with indicators.
    // NULL day_psy → NaN, which strategies treat as "skip the bar".
    let data = day_psy_store::load_market_data(&conn, market, None, None).ok()?;
    if data.is_empty() { None } else { Some(data) }
}

/// Rust V3 params holding the legacy JSON values verbatim.
fn params_literal_legacy() -> TradingParameters {
    TradingParameters::default()
}

struct Metrics<'a> {
    label: &'a str,
    r: &'a SimulationResult,
}

fn print_metrics(m: &Metrics<'_>) {
    let r = m.r;
    eprintln!("── {} ───────────────────────────────────", m.label);
    eprintln!("  trades             : {}", r.total_trades);
    eprintln!("  buy_signals        : {}", r.buy_signals);
    eprintln!("  sell_signals       : {}", r.sell_signals);
    eprintln!("  total_return       : {:.2}%", r.total_return);
    eprintln!("  market_return      : {:.2}%", r.market_return);
    eprintln!("  annual_return      : {:.2}%", r.annual_return);
    eprintln!("  win_rate           : {:.2}%", r.win_rate);
    eprintln!("  profit_factor      : {:.3}", r.profit_factor);
    eprintln!("  max_drawdown       : {:.2}%", r.max_drawdown);
    eprintln!("  max_consec_losses  : {}", r.max_consecutive_losses);
    eprintln!("  sharpe_ratio       : {:.4}", r.sharpe_ratio);
    eprintln!("  sortino_ratio      : {:.4}", r.sortino_ratio);
    eprintln!("  calmar_ratio       : {:.4}", r.calmar_ratio);
    eprintln!("  avg_trade_return   : {:.4}", r.avg_trade_return);
    eprintln!("  last_position      : {}", r.last_position);
    if !r.trades.is_empty() {
        let first = r.trades.first().unwrap();
        let last = r.trades.last().unwrap();
        eprintln!(
            "  first_trade        : bar {}→{} PnL {:.2}%",
            first.buy_index,
            first.sell_index,
            first.pnl_pct * 100.0
        );
        eprintln!(
            "  last_trade         : bar {}→{} PnL {:.2}%",
            last.buy_index,
            last.sell_index,
            last.pnl_pct * 100.0
        );
    }
}

#[test]
fn v3_detailed_comparison_against_legacy_config() {
    // Legacy C# project is ETH-only — load ETH/hour to match the
    // JSON's calibrated distribution.
    let Some(data) = load_market_hour("ETH") else {
        eprintln!("skip: local DB with ETH/hour candles not found");
        return;
    };
    eprintln!("=== V3 legacy-config replay on {} ETH/hour bars ===", data.len());

    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").expect("V3 registered");

    let p = params_literal_legacy();
    eprintln!(
        "\nLegacy JSON values (multiplier semantics) fed into realigned Rust V3 engine:"
    );
    eprintln!(
        "  urgent_buy_vol  lo={:>8.0}  hi={:>8.0}  pow={:.1}",
        p.v3_urgent_buy_volume_lo, p.v3_urgent_buy_volume_hi, p.v3_urgent_buy_volume_pow
    );
    eprintln!(
        "  ready_buy_vol   lo={:>8.0}  hi={:>8.0}  pow={:.1}",
        p.v3_buy_volume_lo, p.v3_buy_volume_hi, p.v3_buy_volume_pow
    );
    eprintln!(
        "  buy_price_drop  lo={:>8.3}  hi={:>8.3}  pow={:.1}   (multiplier: prev × ratio > curr)",
        p.v3_buy_price_drop_lo, p.v3_buy_price_drop_hi, p.v3_buy_price_drop_pow
    );
    eprintln!(
        "  buy_decay       lo={:>8.3}  hi={:>8.3}  pow={:.1}",
        p.v3_buy_decay_lo, p.v3_buy_decay_hi, p.v3_buy_decay_pow
    );
    eprintln!(
        "  buy_psy         lo={:>8.3}  hi={:>8.3}  pow={:.1}   (psy_day threshold)",
        p.v3_buy_psy_lo, p.v3_buy_psy_hi, p.v3_buy_psy_pow
    );
    eprintln!(
        "  buy_wait        lo={:>8.0}  hi={:>8.0}  pow={:.1}   (bars)",
        p.v3_buy_wait_lo, p.v3_buy_wait_hi, p.v3_buy_wait_pow
    );
    eprintln!(
        "  stop_loss       lo={:>8.3}  hi={:>8.3}  pow={:.1}   (multiplier: curr < buy × ratio)",
        p.v3_sell_stop_loss_lo, p.v3_sell_stop_loss_hi, p.v3_sell_stop_loss_pow
    );
    eprintln!(
        "  sell_profit     lo={:>8.3}  hi={:>8.3}  pow={:.1}   (multiplier: curr > buy × ratio)",
        p.v3_sell_profit_lo, p.v3_sell_profit_hi, p.v3_sell_profit_pow
    );
    eprintln!(
        "  sell_vol        lo={:>8.0}  hi={:>8.0}  pow={:.1}",
        p.v3_sell_volume_lo, p.v3_sell_volume_hi, p.v3_sell_volume_pow
    );
    eprintln!(
        "  fixed_sl        lo={:>8.3}  hi={:>8.3}  pow={:.1}   (percent loss)",
        p.v3_sell_fixed_sl_lo, p.v3_sell_fixed_sl_hi, p.v3_sell_fixed_sl_pow
    );
    eprintln!(
        "  max_hold        lo={:>8.0}  hi={:>8.0}  pow={:.1}   (bars)",
        p.v3_sell_max_hold_lo, p.v3_sell_max_hold_hi, p.v3_sell_max_hold_pow
    );
    eprintln!(
        "  misc: fee_rate={} min_hold_bars={} volume_lookback={}",
        p.v3_fee_rate, p.v3_min_hold_bars, p.v3_volume_lookback
    );

    let r = v3.run_simulation(&data, &p);
    print_metrics(&Metrics { label: "Rust V3 @ legacy config", r: &r });

    let max_vol = data.iter().map(|d| d.candle.volume).fold(0.0_f64, f64::max);
    let avg_vol = data.iter().map(|d| d.candle.volume).sum::<f64>() / data.len() as f64;
    let psy_range_min = data
        .iter()
        .map(|d| d.indicators.psy_day)
        .fold(f64::INFINITY, f64::min);
    let psy_range_max = data
        .iter()
        .map(|d| d.indicators.psy_day)
        .fold(f64::NEG_INFINITY, f64::max);
    eprintln!("\n── Feasibility diagnostics (ETH/hour) ──");
    eprintln!(
        "  volume          : avg={:.0}  max={:.0}   (legacy ready_buy_vol lo/hi = {:.0}/{:.0})",
        avg_vol,
        max_vol,
        p.v3_buy_volume_lo,
        p.v3_buy_volume_hi
    );
    eprintln!(
        "  psy_day range   : [{:.1}, {:.1}]       (legacy buy_psy lo/hi = {:.2}/{:.2})",
        psy_range_min, psy_range_max, p.v3_buy_psy_lo, p.v3_buy_psy_hi
    );

    assert!(r.total_return.is_finite());
    assert!(r.max_drawdown.is_finite());
    assert!((0.0..=100.0).contains(&r.win_rate));
    assert!(
        r.total_trades > 0,
        "ETH/hour is the correct market for legacy V3 defaults; expected trades > 0"
    );
}

/// Also compare against BTC/hour to show that the same legacy config is
/// effectively inert on BTC (avg volume ~221 BTC vs 13× that on ETH).
#[test]
fn v3_legacy_config_on_btc_shows_mismatch() {
    let Some(data) = load_market_hour("BTC") else {
        eprintln!("skip: local DB with BTC/hour candles not found");
        return;
    };
    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();
    let r = v3.run_simulation(&data, &TradingParameters::default());
    eprintln!(
        "=== BTC/hour, legacy ETH-tuned V3 config ===\n  trades={}  buy_signals={}  total_return={:.2}%  market_return={:.2}%",
        r.total_trades, r.buy_signals, r.total_return, r.market_return
    );
    // On BTC/hour the ETH-tuned volume thresholds are ~13× too high, so
    // trade count is suppressed. PSY filter no longer blocks (now in
    // [-1,1]) but overall activity stays far below the ETH run.
    assert!(
        r.total_trades < 15,
        "Legacy ETH-tuned V3 on BTC should stay well below ETH trade count; got {}",
        r.total_trades
    );
}

/// Window sweep: slice ETH/hour into the same time-windows the legacy author
/// might have been looking at (JSON saved on 2026-04-01 → likely tuned on a
/// trailing 1y / 2y / 3y / 4y window of data). Report per-window metrics so
/// we can pin down which period yielded the ~900% number cited for legacy.
#[test]
fn v3_window_sweep_vs_legacy_cited_return() {
    let Some(full) = load_market_hour("ETH") else {
        eprintln!("skip: local DB with ETH/hour candles not found");
        return;
    };
    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();
    let p = TradingParameters::default();

    // JSON saved 2026-04-01; assume the author tuned looking back from there.
    let cutoff = DateTime::parse_from_rfc3339("2026-04-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let windows_days: [(&str, i64); 6] = [
        ("full history", i64::MAX),
        ("6 months", 183),
        ("1 year", 365),
        ("2 years", 730),
        ("3 years", 1095),
        ("4 years", 1460),
    ];

    eprintln!(
        "\n┌─────────────────────────┬─────────┬────────┬──────────────┬──────────────┐"
    );
    eprintln!(
        "│ Window ending 2026-04-01│ Bars    │ Trades │ Total Return │ Market Return│"
    );
    eprintln!(
        "├─────────────────────────┼─────────┼────────┼──────────────┼──────────────┤"
    );
    for (label, days) in windows_days {
        let start = if days == i64::MAX {
            DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        } else {
            cutoff - chrono::Duration::days(days)
        };
        let sliced: Vec<MarketData> = full
            .iter()
            .filter(|d| d.candle.timestamp >= start && d.candle.timestamp <= cutoff)
            .cloned()
            .collect();
        if sliced.is_empty() {
            continue;
        }
        let r = v3.run_simulation(&sliced, &p);
        eprintln!(
            "│ {:<23} │ {:>7} │ {:>6} │ {:>11.2}% │ {:>11.2}% │",
            label, sliced.len(), r.total_trades, r.total_return, r.market_return
        );
    }
    eprintln!(
        "└─────────────────────────┴─────────┴────────┴──────────────┴──────────────┘"
    );
}

/// Replay legacy V3 trade log window (KST 2025-01-01 → 2026-04-01) and
/// print a trade-by-trade comparison. Legacy timestamps in the log are in
/// KST (UTC+9), so window bounds are shifted by -9h to match the same UTC
/// bars.
///
/// Legacy log: `C:/Users/user/Desktop/legacy_V3.txt` — first buy ready
/// 2025-01-08 00:00 KST @ 5,153,000; last sell 2026-03-31 01:00 KST.
#[test]
fn v3_replay_legacy_trade_log_window() {
    let Some(full) = load_market_hour("ETH") else {
        eprintln!("skip: local DB with ETH/hour candles not found");
        return;
    };
    // Exact legacy StartDate/EndDate from JSON (KST → UTC):
    //   StartDate 2025-01-01T18:00 KST → UTC 2025-01-01T09:00
    //   EndDate   2026-03-31T17:00 KST → UTC 2026-03-31T08:00
    let start = DateTime::parse_from_rfc3339("2025-01-01T09:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339("2026-03-31T08:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let window: Vec<MarketData> = full
        .into_iter()
        .filter(|d| d.candle.timestamp >= start && d.candle.timestamp <= end)
        .collect();
    assert!(!window.is_empty(), "ETH data missing for legacy log window");

    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();
    let p = TradingParameters::default();
    let r = v3.run_simulation(&window, &p);

    eprintln!(
        "\n=== Rust V3 on legacy log window ({} bars, {} → {}) ===",
        window.len(),
        window.first().unwrap().candle.timestamp.format("%Y-%m-%d %H:%M"),
        window.last().unwrap().candle.timestamp.format("%Y-%m-%d %H:%M")
    );
    eprintln!(
        "  trades={}  buy_signals={}  total_return={:.2}%  market_return={:.2}%  win_rate={:.2}%  max_dd={:.2}%",
        r.total_trades,
        r.buy_signals,
        r.total_return,
        r.market_return,
        r.win_rate,
        r.max_drawdown
    );
    eprintln!("\n── First 20 Rust trades (timestamps shown in KST for legacy alignment) ──");
    for (idx, t) in r.trades.iter().take(20).enumerate() {
        let buy_utc = DateTime::parse_from_rfc3339(&t.buy_timestamp).unwrap();
        let sell_utc = DateTime::parse_from_rfc3339(&t.sell_timestamp).unwrap();
        let buy_kst = buy_utc + chrono::Duration::hours(9);
        let sell_kst = sell_utc + chrono::Duration::hours(9);
        eprintln!(
            "  #{:<2} buy {} @ {:>11.0}  →  sell {} @ {:>11.0}   pnl={:>6.2}%  hold={}",
            idx + 1,
            buy_kst.format("%Y-%m-%d %H:%M"),
            t.buy_price,
            sell_kst.format("%Y-%m-%d %H:%M"),
            t.sell_price,
            t.pnl_pct * 100.0,
            t.hold_bars
        );
    }
    eprintln!("\n── Legacy first 13 trades (for reference) ──");
    eprintln!("  #1  buy 2025-01-08 19:00 @   5018000  →  sell 2025-01-10 02:00 @   4912000  pnl= -2.11%");
    eprintln!("  #2  buy 2025-01-15 03:00 @   4795000  →  sell 2025-01-22 02:00 @   4971000  pnl= +3.67%");
    eprintln!("  #3  buy 2025-01-28 04:00 @   4733000  →  sell 2025-01-31 02:00 @   4954000  pnl= +4.67%");
    eprintln!("  #4  buy 2025-02-03 10:00 @   4060000  →  sell 2025-02-05 01:00 @   4362000  pnl= +7.44%");
    eprintln!("  #5  buy 2025-02-09 02:00 @   4052000  →  sell 2025-02-11 02:00 @   4055000  pnl= +0.07%");
    eprintln!("  #6  buy 2025-02-14 02:00 @   3992000  →  sell 2025-02-18 05:00 @   4123000  pnl= +3.28%");
    eprintln!("  #7  buy 2025-02-25 16:00 @   3428000  →  sell 2025-02-27 08:00 @   3408000  pnl= -0.58%");
    eprintln!("  #8  buy 2025-03-02 02:00 @   3270000  →  sell 2025-03-03 09:00 @   3684000  pnl=+12.66%");

    assert!(r.total_trades > 0, "Rust must produce trades on the legacy log window");
}

// ────────────────────────────────────────────────────────────────────────
// Legacy trade-log parsing + bar-level diff
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct LegacyTrade {
    buy_kst: DateTime<Utc>,   // stored as UTC (converted from KST)
    buy_price: f64,
    sell_kst: DateTime<Utc>,
    sell_price: f64,
}

fn parse_legacy_log(path: &str) -> Option<Vec<LegacyTrade>> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut trades = Vec::new();
    let mut pending_buy: Option<(DateTime<Utc>, f64)> = None;

    for line in content.lines() {
        // format: \t{ts KST}\t{signal}\t{price}\t{bs}\t{ss}\t{bpo}\t{spo}\t{pos}
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            continue;
        }
        let ts_raw = fields[1].trim();
        let signal = fields[2].trim();
        let price_raw = fields[3].trim().replace(",", "");

        // Parse KST timestamp "2025-01-08 19:00" → UTC by subtracting 9h.
        let ts_kst = match chrono::NaiveDateTime::parse_from_str(ts_raw, "%Y-%m-%d %H:%M") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts_utc = Utc.from_utc_datetime(&ts_kst) - chrono::Duration::hours(9);
        let price: f64 = price_raw.parse().unwrap_or(0.0);

        match signal {
            "buy" => pending_buy = Some((ts_utc, price)),
            "sell" => {
                if let Some((bts, bpx)) = pending_buy.take() {
                    trades.push(LegacyTrade {
                        buy_kst: bts,
                        buy_price: bpx,
                        sell_kst: ts_utc,
                        sell_price: price,
                    });
                }
            }
            _ => {}
        }
    }
    Some(trades)
}

/// Full bar-level diff: parse legacy log, simulate Rust V3 on same window,
/// and emit per-trade comparison so we can classify discrepancies.
#[test]
fn v3_trade_level_diff_vs_legacy_log() {
    let legacy_path = "C:/Users/user/Desktop/legacy_V3.txt";
    let Some(legacy_trades) = parse_legacy_log(legacy_path) else {
        eprintln!("skip: legacy log {} not readable", legacy_path);
        return;
    };
    let Some(full) = load_market_hour("ETH") else {
        eprintln!("skip: DB missing");
        return;
    };
    let start = DateTime::parse_from_rfc3339("2024-12-31T15:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339("2026-03-31T15:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let window: Vec<MarketData> = full
        .into_iter()
        .filter(|d| d.candle.timestamp >= start && d.candle.timestamp <= end)
        .collect();

    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();
    let p = TradingParameters::default();
    let rust = v3.run_simulation(&window, &p);

    eprintln!(
        "\n=== Legacy {} trades vs Rust {} trades ===",
        legacy_trades.len(),
        rust.trades.len()
    );

    // Two-pointer walk matching by buy-time. Tolerate up to ±2h slack
    // (float boundaries may shift entry by a bar).
    let mut li = 0usize;
    let mut ri = 0usize;
    let mut matched = 0;
    let mut legacy_only: Vec<&LegacyTrade> = Vec::new();
    let mut rust_only: Vec<usize> = Vec::new();
    let mut exit_shifts: Vec<(DateTime<Utc>, i64)> = Vec::new();

    while li < legacy_trades.len() && ri < rust.trades.len() {
        let lt = &legacy_trades[li];
        let rt = &rust.trades[ri];
        let rt_buy = DateTime::parse_from_rfc3339(&rt.buy_timestamp).unwrap().with_timezone(&Utc);
        let rt_sell = DateTime::parse_from_rfc3339(&rt.sell_timestamp).unwrap().with_timezone(&Utc);
        let buy_diff = (rt_buy - lt.buy_kst).num_hours();
        if buy_diff.abs() <= 2 {
            matched += 1;
            let sell_diff = (rt_sell - lt.sell_kst).num_hours();
            if sell_diff.abs() > 0 {
                exit_shifts.push((lt.buy_kst, sell_diff));
            }
            li += 1;
            ri += 1;
        } else if buy_diff < 0 {
            // Rust entered before legacy → Rust-only trade
            rust_only.push(ri);
            ri += 1;
        } else {
            // Legacy entered before Rust → legacy-only trade (Rust missed)
            legacy_only.push(lt);
            li += 1;
        }
    }
    while li < legacy_trades.len() {
        legacy_only.push(&legacy_trades[li]);
        li += 1;
    }
    while ri < rust.trades.len() {
        rust_only.push(ri);
        ri += 1;
    }

    eprintln!(
        "  matched={}  legacy-only={}  rust-only={}  exit-shifts={}",
        matched,
        legacy_only.len(),
        rust_only.len(),
        exit_shifts.len()
    );

    eprintln!("\n── First 15 LEGACY-ONLY trades (Rust missed entry) ──");
    for lt in legacy_only.iter().take(15) {
        let kst = lt.buy_kst + chrono::Duration::hours(9);
        eprintln!(
            "  {}  buy @ {:>11.0}  sell @ {:>11.0}  pnl={:+.2}%",
            kst.format("%Y-%m-%d %H:%M"),
            lt.buy_price,
            lt.sell_price,
            (lt.sell_price - lt.buy_price) / lt.buy_price * 100.0
        );
    }
    eprintln!("\n── First 10 RUST-ONLY trades (legacy didn't enter) ──");
    for idx in rust_only.iter().take(10) {
        let t = &rust.trades[*idx];
        let bkst =
            DateTime::parse_from_rfc3339(&t.buy_timestamp).unwrap() + chrono::Duration::hours(9);
        let skst =
            DateTime::parse_from_rfc3339(&t.sell_timestamp).unwrap() + chrono::Duration::hours(9);
        eprintln!(
            "  {} → {}  buy @ {:>11.0}  sell @ {:>11.0}  pnl={:+.2}%",
            bkst.format("%Y-%m-%d %H:%M"),
            skst.format("%Y-%m-%d %H:%M"),
            t.buy_price,
            t.sell_price,
            t.pnl_pct * 100.0
        );
    }

    let avg_exit_shift = if exit_shifts.is_empty() {
        0.0
    } else {
        exit_shifts.iter().map(|(_, h)| *h as f64).sum::<f64>() / exit_shifts.len() as f64
    };
    eprintln!(
        "\n── Exit-shift stats over {} matched trades: avg {:+.1}h ──",
        exit_shifts.len(),
        avg_exit_shift
    );
}

#[test]
#[ignore] // run manually — requires legacy trace CSV to exist
fn v3_dump_rust_trace_and_diff() {
    let Some(full) = load_market_hour("ETH") else {
        eprintln!("skip: DB missing");
        return;
    };
    let start = DateTime::parse_from_rfc3339("2025-01-01T09:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339("2026-03-31T08:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    // Start with the full dataset — we re-filter after building day-PSY map.
    let window: Vec<MarketData> = full;

    // Legacy parity: `MarketDataConverter` reads the `ETH_day_new_psy`
    // column directly from `merged_data_hour.csv`. That column is a mix of
    // real day-scale PSY (when the daily calc succeeded) and fallback
    // hour-PSY (DataUpdateManager wrote `day = hour` when day was 0). So
    // the only faithful reproduction is to parse the CSV column as-is.
    let day_psy_map: BTreeMap<NaiveDate, f64> = {
        let csv =
            std::fs::read_to_string("D:/SW/Bitcoin/merged_data_hour.csv").unwrap_or_default();
        let mut map = BTreeMap::new();
        let header_fields: Vec<&str> = csv.lines().next().unwrap_or("").split(',').collect();
        let time_idx = header_fields
            .iter()
            .position(|&c| c == "time")
            .unwrap_or(0);
        let day_psy_idx = header_fields
            .iter()
            .position(|&c| c == "ETH_day_new_psy")
            .expect("ETH_day_new_psy column missing");
        for (i, line) in csv.lines().enumerate() {
            if i == 0 {
                continue;
            }
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() <= day_psy_idx {
                continue;
            }
            let time_str = fields[time_idx].trim();
            let date = match chrono::NaiveDate::parse_from_str(
                &time_str[..time_str.len().min(10)],
                "%Y-%m-%d",
            ) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let v: f64 = fields[day_psy_idx].parse().unwrap_or(0.0);
            if v != 0.0 {
                map.insert(date, v); // overwrite — last non-zero per KST date
            }
        }
        map
    };

    let full_candles: Vec<_> = window.iter().map(|m| m.candle.clone()).collect();

    // Filter to the legacy test window
    let window_candles: Vec<_> = full_candles
        .into_iter()
        .filter(|c| c.timestamp >= start && c.timestamp <= end)
        .collect();
    let window_indicators =
        indicators::calculate_all_with_day_psy(&window_candles, Some(&day_psy_map));
    let window: Vec<MarketData> = window_candles
        .into_iter()
        .zip(window_indicators)
        .map(|(candle, indicators)| MarketData { candle, indicators })
        .collect();

    let rust_trace_path = "D:/SW/Bitcoin/v3_rust_trace.csv";
    std::env::set_var("V3_TRACE_PATH", rust_trace_path);
    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();
    let _ = v3.run_simulation(&window, &TradingParameters::default());
    std::env::remove_var("V3_TRACE_PATH");

    // Load both traces keyed by timestamp
    let legacy = std::fs::read_to_string("D:/SW/Bitcoin/v3_legacy_trace.csv")
        .expect("legacy trace missing");
    let rust = std::fs::read_to_string(rust_trace_path).expect("rust trace missing");

    // Each row: timestamp column is index 1 (KST). Compare numeric columns.
    let parse = |txt: &str| -> std::collections::BTreeMap<String, Vec<String>> {
        let mut m = std::collections::BTreeMap::new();
        for (i, line) in txt.lines().enumerate() {
            if i == 0 {
                continue;
            }
            let cols: Vec<String> = line.split(',').map(|s| s.to_string()).collect();
            if cols.len() < 19 {
                continue;
            }
            m.insert(cols[1].clone(), cols);
        }
        m
    };
    let l = parse(&legacy);
    let r = parse(&rust);
    let mut first_diff: Option<(String, Vec<(String, String, String)>)> = None;
    let mut diff_count = 0usize;
    for ts in l.keys() {
        let Some(lr) = l.get(ts) else { continue };
        let Some(rr) = r.get(ts) else { continue };
        // Compare key columns: close, vol, rsi, psyDay, pos, bSign, sSign,
        // buyPo, sellPo, setVol, entryRsi, dynBuyVol, dynBuyDrop, dynBuyDecay,
        // dynBuyPsy, dynBuyWait. (columns 2..18)
        let col_names = [
            "close", "vol", "rsi", "psyDay", "pos", "bSign", "sSign",
            "buyPo", "sellPo", "setVol", "entryRsi", "dynBuyVol",
            "dynBuyDrop", "dynBuyDecay", "dynBuyPsy", "dynBuyWait",
        ];
        let mut row_diffs = Vec::new();
        for (ci, name) in col_names.iter().enumerate() {
            let li = ci + 2;
            let lv = &lr[li];
            let rv = &rr[li];
            // Numeric comparison with tolerance
            let same = match (lv.parse::<f64>(), rv.parse::<f64>()) {
                (Ok(a), Ok(b)) => (a - b).abs() <= a.abs().max(b.abs()) * 1e-4 + 1e-9,
                _ => lv == rv,
            };
            if !same {
                row_diffs.push((name.to_string(), lv.clone(), rv.clone()));
            }
        }
        if !row_diffs.is_empty() {
            diff_count += 1;
            if first_diff.is_none() {
                first_diff = Some((ts.clone(), row_diffs));
            }
        }
    }
    eprintln!(
        "=== trace diff ===  legacy rows={}  rust rows={}  diff rows={}",
        l.len(),
        r.len(),
        diff_count
    );
    if let Some((ts, diffs)) = first_diff {
        eprintln!("FIRST DIVERGENCE at {}:", ts);
        for (name, lv, rv) in diffs {
            eprintln!("    {:<12} legacy={:<20} rust={:<20}", name, lv, rv);
        }
    } else {
        eprintln!("No per-bar divergence within tolerance — perfect parity.");
    }
}

#[test]
#[ignore] // diagnostic — keep but don't run by default
fn v3_probe_high_profit_misses() {
    let Some(data) = load_market_hour("ETH") else {
        eprintln!("skip");
        return;
    };
    let p = TradingParameters::default();
    // UTC timestamps for each legacy-only high-profit miss (KST listed)
    let probes = [
        ("2025-10-10 15:00 KST", "2025-10-10T06:00:00Z"), // legacy buy_ready
        ("2025-12-26 18:00 KST", "2025-12-26T09:00:00Z"),
        ("2026-03-13 03:00 KST", "2026-03-12T18:00:00Z"),
        ("2025-12-06 05:00 KST", "2025-12-05T20:00:00Z"),
        ("2025-12-18 16:00 KST", "2025-12-18T07:00:00Z"),
    ];
    for (label, iso) in probes {
        let t = DateTime::parse_from_rfc3339(iso).unwrap().with_timezone(&Utc);
        let idx = match data.iter().position(|d| d.candle.timestamp == t) {
            Some(v) => v,
            None => {
                eprintln!("[{}] bar not in DB", label);
                continue;
            }
        };
        let cur = &data[idx];
        let prev = &data[idx - 1];
        let rsi = cur.indicators.rsi.max(0.0);
        let rsi_safe = if rsi > 0.0 { rsi } else { 50.0 };
        let tt = ((rsi_safe - 20.0) / 60.0).clamp(0.0, 1.0);
        let dyn_buy_vol = p.v3_buy_volume_lo
            + (p.v3_buy_volume_hi - p.v3_buy_volume_lo) * tt.powf(p.v3_buy_volume_pow);
        let dyn_drop = p.v3_buy_price_drop_lo
            + (p.v3_buy_price_drop_hi - p.v3_buy_price_drop_lo)
                * tt.powf(p.v3_buy_price_drop_pow);
        let cond_vol = cur.candle.volume > dyn_buy_vol;
        let cond_price = prev.candle.close * dyn_drop > cur.candle.close;
        eprintln!(
            "[{}]  close={:>8.0}  vol={:>7.0}  rsi={:>5.2}  psy_day={:>7.4}  prev={:>8.0}",
            label,
            cur.candle.close,
            cur.candle.volume,
            rsi_safe,
            cur.indicators.psy_day,
            prev.candle.close
        );
        eprintln!(
            "    dyn_buy_vol={:>8.2}  vol>dyn? {}   dyn_drop={:.4}  prev*drop={:.0}  price-cond? {}   both? {}",
            dyn_buy_vol, cond_vol, dyn_drop,
            prev.candle.close * dyn_drop, cond_price,
            cond_vol && cond_price
        );
    }
}

#[test]
#[ignore] // diagnostic
fn v3_list_all_rust_trades_kst() {
    let Some(full) = load_market_hour("ETH") else {
        eprintln!("skip: DB missing");
        return;
    };
    let start = DateTime::parse_from_rfc3339("2025-01-01T09:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339("2026-03-31T08:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let window: Vec<MarketData> = full
        .into_iter()
        .filter(|d| d.candle.timestamp >= start && d.candle.timestamp <= end)
        .collect();
    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();
    let r = v3.run_simulation(&window, &TradingParameters::default());
    eprintln!("=== ALL {} Rust trades (KST) ===", r.trades.len());
    for (idx, t) in r.trades.iter().enumerate() {
        let bk = DateTime::parse_from_rfc3339(&t.buy_timestamp).unwrap()
            + chrono::Duration::hours(9);
        let sk = DateTime::parse_from_rfc3339(&t.sell_timestamp).unwrap()
            + chrono::Duration::hours(9);
        eprintln!(
            "  #{:<2} {} → {}  buy {:>9.0} sell {:>9.0}  pnl={:>6.2}% hold={}",
            idx + 1,
            bk.format("%m-%d %H:%M"),
            sk.format("%m-%d %H:%M"),
            t.buy_price,
            t.sell_price,
            t.pnl_pct * 100.0,
            t.hold_bars
        );
    }
}

#[test]
#[ignore] // diagnostic
fn v3_dump_confirm_bar_2025_04_24() {
    let Some(data) = load_market_hour("ETH") else {
        eprintln!("skip: DB missing");
        return;
    };
    // KST 2025-04-24 04:00 = UTC 2025-04-23 19:00 — legacy confirm bar
    let target = DateTime::parse_from_rfc3339("2025-04-23T19:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let idx = data.iter().position(|d| d.candle.timestamp == target).unwrap();
    let p = TradingParameters::default();
    let cur = &data[idx];
    let rsi = cur.indicators.rsi.max(0.0);
    let rsi_safe = if rsi > 0.0 { rsi } else { 50.0 };
    let t = ((rsi_safe - 20.0) / 60.0).clamp(0.0, 1.0);
    let dyn_decay = p.v3_buy_decay_lo
        + (p.v3_buy_decay_hi - p.v3_buy_decay_lo) * t.powf(p.v3_buy_decay_pow);
    let dyn_psy = p.v3_buy_psy_lo
        + (p.v3_buy_psy_hi - p.v3_buy_psy_lo) * t.powf(p.v3_buy_psy_pow);
    eprintln!("=== Confirm bar (Rust side) 2025-04-24 04:00 KST ===");
    eprintln!("  UTC                     : {}", cur.candle.timestamp);
    eprintln!("  rsi                     : {:.4}", rsi_safe);
    eprintln!("  psy_day (legacy prevDay): {:.6}", cur.indicators.psy_day);
    eprintln!("  psy_hour                : {:.6}", cur.indicators.psy_hour);
    eprintln!("  dyn_buy_psy threshold   : {:.6}", dyn_psy);
    eprintln!("  dyn_buy_decay           : {:.6}", dyn_decay);
    eprintln!("  psy_day < threshold ?   : {}", cur.indicators.psy_day < dyn_psy);
}

/// Dump indicator values and the V3 buy-condition evaluation at the
/// specific bar where legacy entered but Rust skipped, so we can pin
/// down which field disagrees.
#[test]
#[ignore] // diagnostic
fn v3_dump_missed_entry_bar() {
    let Some(data) = load_market_hour("ETH") else {
        eprintln!("skip: DB missing");
        return;
    };
    // Target: legacy buy_ready at 2025-04-22 16:00 KST = UTC 2025-04-22 07:00
    let target = DateTime::parse_from_rfc3339("2025-04-22T07:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let idx = data
        .iter()
        .position(|d| d.candle.timestamp == target)
        .expect("target bar missing in DB");
    let p = TradingParameters::default();
    let cur = &data[idx];
    let prev = &data[idx - 1];
    let rsi = cur.indicators.rsi.max(0.0);
    let rsi_safe = if rsi > 0.0 { rsi } else { 50.0 };

    let dyn_buy_vol = {
        let t = ((rsi_safe - 20.0) / 60.0).clamp(0.0, 1.0);
        p.v3_buy_volume_lo
            + (p.v3_buy_volume_hi - p.v3_buy_volume_lo)
                * t.powf(p.v3_buy_volume_pow)
    };
    let dyn_buy_price_drop = {
        let t = ((rsi_safe - 20.0) / 60.0).clamp(0.0, 1.0);
        p.v3_buy_price_drop_lo
            + (p.v3_buy_price_drop_hi - p.v3_buy_price_drop_lo)
                * t.powf(p.v3_buy_price_drop_pow)
    };
    let cond_vol = cur.candle.volume > dyn_buy_vol;
    let cond_price = prev.candle.close * dyn_buy_price_drop > cur.candle.close;
    eprintln!("=== Missed entry bar diagnostic ===");
    eprintln!("  timestamp (UTC)         : {}", cur.candle.timestamp);
    eprintln!("  timestamp (KST)         : {}", cur.candle.timestamp + chrono::Duration::hours(9));
    eprintln!("  prev_close              : {:.0}", prev.candle.close);
    eprintln!("  cur close/volume        : {:.0} / {:.1}", cur.candle.close, cur.candle.volume);
    eprintln!("  rsi (safe)              : {:.4}", rsi_safe);
    eprintln!("  psy_day                 : {:.6}", cur.indicators.psy_day);
    eprintln!("  dyn_buy_volume          : {:.2}  (vol > this? {})", dyn_buy_vol, cond_vol);
    eprintln!(
        "  dyn_buy_price_drop      : {:.4}  (prev×r = {:.0} > cur? {})",
        dyn_buy_price_drop,
        prev.candle.close * dyn_buy_price_drop,
        cond_price
    );
    eprintln!(
        "  buy_ready would trigger : {}",
        cond_vol && cond_price
    );
}

/// Flat-market sanity: no price movement → no trades regardless of ratios.
#[test]
fn v3_literal_legacy_on_flat_market_yields_no_trades() {
    use bitcoin_trader_lib::models::market::{Candle, IndicatorSet};
    use chrono::{TimeZone, Utc};

    let base = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let data: Vec<MarketData> = (0..500)
        .map(|i| MarketData {
            candle: Candle {
                timestamp: base + chrono::Duration::hours(i as i64),
                open: 100.0,
                high: 100.1,
                low: 99.9,
                close: 100.0,
                volume: 500.0,
            },
            indicators: IndicatorSet::default(),
        })
        .collect();

    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();
    let r = v3.run_simulation(&data, &TradingParameters::default());
    assert_eq!(
        r.total_trades, 0,
        "literal legacy V3 default on flat market must not trade (price_drop ratio >1 is unreachable)"
    );
}

/// NSGA-II re-optimization of V3 on the clean (non-hybrid) day_psy pipeline.
///
/// Goal: find V3 parameter settings that outperform the legacy 909% number
/// on ETH/hour using the corrected day-scale PSY (no hour_psy fallback).
/// Prints the Pareto front sorted by total_return and saves the best
/// configuration as JSON for consumption by the frontend config editor.
///
/// Marked `#[ignore]` because NSGA-II takes minutes; run manually with
///   `cargo test --test v3_legacy_config_test --no-default-features \
///       v3_nsga2_reoptimize_on_clean_day_psy -- --ignored --nocapture`
#[test]
#[ignore]
fn v3_nsga2_reoptimize_on_clean_day_psy() {
    use bitcoin_trader_lib::core::optimizer::Nsga2Optimizer;
    use bitcoin_trader_lib::models::config::OptimizerConfig;

    let Some(full) = load_market_hour("ETH") else {
        eprintln!("skip: DB missing");
        return;
    };
    // Legacy evaluation window (KST → UTC): same bars legacy used.
    let start = DateTime::parse_from_rfc3339("2025-01-01T09:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339("2026-03-31T08:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let window: Vec<MarketData> = full
        .into_iter()
        .filter(|d| d.candle.timestamp >= start && d.candle.timestamp <= end)
        .collect();
    assert!(!window.is_empty(), "ETH/hour window must have data");
    eprintln!(
        "=== NSGA-II V3 optimization ({} bars, clean day_psy pipeline) ===",
        window.len()
    );

    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();

    // Modest size for a one-shot verification; bump pop/gen for a serious run.
    let config = OptimizerConfig {
        population_size: 80,
        generations: 60,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        objectives: vec!["total_return".to_string(), "win_rate".to_string()],
        min_win_rate: 50.0,   // avoid pathological "1 trade, 100% winrate"
        min_trades: 20,
        min_return: 0.0,
    };
    let optimizer = Nsga2Optimizer::new(config);
    let start_t = std::time::Instant::now();
    let mut population = optimizer.run(&window, v3, None);
    eprintln!("NSGA-II finished in {:.1}s — {} individuals", start_t.elapsed().as_secs_f64(), population.len());

    // Sort Pareto front by total_return descending
    population.retain(|ind| ind.rank == 0);
    population.sort_by(|a, b| b.objectives[0].partial_cmp(&a.objectives[0]).unwrap());

    eprintln!("\n── Pareto front top-10 (by total_return) ──");
    let ranges = v3.parameter_ranges();
    for (i, ind) in population.iter().take(10).enumerate() {
        eprintln!(
            "  #{:<2}  total_return={:>8.2}%   win_rate={:>5.2}%",
            i + 1, ind.objectives[0], ind.objectives[1]
        );
    }

    // Re-run simulation for the #1 solution to get full metrics + save params
    if let Some(best) = population.first() {
        let full_result = v3.run_simulation(&window, &best.parameters);
        eprintln!("\n── Best solution metrics ──");
        eprintln!("  total_return   : {:.2}%", full_result.total_return);
        eprintln!("  trades         : {}", full_result.total_trades);
        eprintln!("  win_rate       : {:.2}%", full_result.win_rate);
        eprintln!("  profit_factor  : {:.3}", full_result.profit_factor);
        eprintln!("  max_drawdown   : {:.2}%", full_result.max_drawdown);
        eprintln!("  sharpe_ratio   : {:.4}", full_result.sharpe_ratio);

        // Serialize best parameters to JSON next to the project
        let params_json = serde_json::to_string_pretty(&best.parameters).unwrap_or_default();
        let out_path = "D:/SW/bitcoin-trader/src-tauri/v3_optimized_clean.json";
        let _ = std::fs::write(out_path, &params_json);
        eprintln!("\nSaved best params → {}", out_path);
        let legacy_baseline = 278.64;
        eprintln!(
            "Baseline (legacy V3 params on clean pipeline): {:.2}%  →  NSGA-II best: {:.2}%  (Δ {:+.2}pp)",
            legacy_baseline,
            full_result.total_return,
            full_result.total_return - legacy_baseline
        );
        // Record parameter deltas for high-salience fields
        eprintln!("\n── Key parameter deltas vs legacy default ──");
        let legacy = TradingParameters::default();
        let names_to_show = [
            "v3_buy_psy_lo", "v3_buy_psy_hi", "v3_buy_psy_pow",
            "v3_buy_volume_lo", "v3_buy_volume_hi",
            "v3_buy_price_drop_lo", "v3_buy_price_drop_hi",
            "v3_sell_profit_lo", "v3_sell_profit_hi",
            "v3_sell_stop_loss_lo", "v3_sell_fixed_sl_lo",
        ];
        for r in &ranges {
            if !names_to_show.contains(&r.name.as_str()) { continue; }
            let opt_val = bitcoin_trader_lib::core::optimizer::get_parameter(&best.parameters, &r.name);
            let lgy_val = bitcoin_trader_lib::core::optimizer::get_parameter(&legacy, &r.name);
            eprintln!(
                "  {:<28} legacy={:>10.4}   opt={:>10.4}   Δ={:+.4}",
                r.name, lgy_val, opt_val, opt_val - lgy_val
            );
        }
    }
}
