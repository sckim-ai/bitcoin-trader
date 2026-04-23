//! Cum % vs Total Return parity — diagnostic tests to explain why the
//! frontend's trade-based cumulative return may or may not match the
//! engine's bar-based `total_return`.
//!
//! The engine (regime_adaptive / regime_adaptive_v31 / enhanced_adaptive)
//! computes:
//!     bar_return[i] = previous_position × daily_return[i]
//!                   + (-fee if entering on bar i)
//!                   + (-fee if exiting  on bar i)
//!     total_return  = ∏(1 + bar_return[i]) − 1
//!
//! The frontend UI computes the "Cum %" column with three candidate
//! formulas:
//!   (a) gross   : ∏(1 + pnl_pct)                       — no fee at all
//!   (b) approx  : ∏(1 + pnl_pct − 2·fee)                — current implementation
//!   (c) exact   : ∏((1−f) · (c_{m−1}/c_k) · (c_m/c_{m−1} − f))
//!                   where  c_k = buy_price, c_m = sell_price,
//!                          c_{m−1} = close one bar before exit
//!
//! These tests:
//!   1.  Verify that formula (c) matches the engine exactly for clean,
//!       fully-closed trades (all positions return to 0 by end of data).
//!   2.  Show the drift for formula (b) vs the engine, per trade and
//!       compounded — explaining the residual difference in the UI.
//!   3.  Isolate the "open position at end" case, where a trade that
//!       never closes contributes to `total_return` but not to
//!       `result.trades`, causing a large divergence.

/// Compute the engine's total_return for a given price/position sequence.
/// Mirrors the logic in `regime_adaptive.rs:318-331` exactly (minus the
/// `i > 10` warmup guard, which is irrelevant for pure-math testing).
fn engine_total_return(closes: &[f64], positions: &[u8], fee: f64) -> f64 {
    assert_eq!(closes.len(), positions.len());
    let mut product = 1.0_f64;
    for i in 1..closes.len() {
        let daily = (closes[i] - closes[i - 1]) / closes[i - 1];
        let prev_pos = positions[i - 1] as f64;
        let mut bar_return = prev_pos * daily;
        if positions[i - 1] == 0 && positions[i] == 1 {
            bar_return -= fee;
        }
        if positions[i - 1] == 1 && positions[i] == 0 {
            bar_return -= fee;
        }
        product *= 1.0 + bar_return;
    }
    product - 1.0
}

#[derive(Clone, Copy)]
struct Trade {
    buy_i: usize,
    sell_i: usize,
    buy_price: f64,
    sell_price: f64,
    close_m_minus_1: f64,
}

/// Extract closed trades from a position sequence.  Open positions at
/// end of data are deliberately skipped, mimicking the engine's behavior
/// where no sell → no `TradeRecord` pushed to `result.trades`.
fn extract_trades(closes: &[f64], positions: &[u8]) -> Vec<Trade> {
    let mut out = Vec::new();
    let mut open_buy: Option<usize> = None;
    for i in 1..positions.len() {
        if positions[i - 1] == 0 && positions[i] == 1 {
            open_buy = Some(i);
        }
        if positions[i - 1] == 1 && positions[i] == 0 {
            if let Some(k) = open_buy.take() {
                out.push(Trade {
                    buy_i: k,
                    sell_i: i,
                    buy_price: closes[k],
                    sell_price: closes[i],
                    close_m_minus_1: closes[i - 1],
                });
            }
        }
    }
    out
}

fn pnl(t: &Trade) -> f64 {
    (t.sell_price - t.buy_price) / t.buy_price
}

fn trade_product_gross(trades: &[Trade]) -> f64 {
    trades.iter().map(|t| 1.0 + pnl(t)).product::<f64>() - 1.0
}

fn trade_product_approx(trades: &[Trade], fee: f64) -> f64 {
    trades
        .iter()
        .map(|t| 1.0 + pnl(t) - 2.0 * fee)
        .product::<f64>()
        - 1.0
}

fn trade_product_exact(trades: &[Trade], fee: f64) -> f64 {
    trades
        .iter()
        .map(|t| {
            (1.0 - fee)
                * (t.close_m_minus_1 / t.buy_price)
                * (t.sell_price / t.close_m_minus_1 - fee)
        })
        .product::<f64>()
        - 1.0
}

fn print_breakdown(label: &str, closes: &[f64], positions: &[u8], fee: f64) {
    let engine = engine_total_return(closes, positions, fee);
    let trades = extract_trades(closes, positions);
    let gross = trade_product_gross(&trades);
    let approx = trade_product_approx(&trades, fee);
    let exact = trade_product_exact(&trades, fee);

    eprintln!();
    eprintln!("══════ {} ══════", label);
    eprintln!("  closes    : {:?}", closes);
    eprintln!("  positions : {:?}", positions);
    eprintln!("  fee_rate  : {}", fee);
    eprintln!("  trades    : {}", trades.len());
    for (idx, t) in trades.iter().enumerate() {
        eprintln!(
            "    #{}: bar {}→{}, buy={:.4}, sell={:.4}, c_{{m-1}}={:.4}, pnl={:+.4}%",
            idx + 1,
            t.buy_i,
            t.sell_i,
            t.buy_price,
            t.sell_price,
            t.close_m_minus_1,
            pnl(t) * 100.0
        );
    }
    eprintln!();
    eprintln!("  engine total_return : {:>12.6}%", engine * 100.0);
    eprintln!(
        "  (c) exact per-trade : {:>12.6}%   Δvs_engine = {:+.3e}",
        exact * 100.0,
        exact - engine
    );
    eprintln!(
        "  (b) approx −2f/trade: {:>12.6}%   Δvs_engine = {:+.3e}",
        approx * 100.0,
        approx - engine
    );
    eprintln!(
        "  (a) gross           : {:>12.6}%   Δvs_engine = {:+.3e}",
        gross * 100.0,
        gross - engine
    );
}

// ════════════════════════════════════════════════════════════════════
// Test 1: Single clean trade — formula (c) must match engine exactly.
// ════════════════════════════════════════════════════════════════════
#[test]
fn single_clean_trade_formula_c_matches_engine() {
    // bar:  0     1     2     3     4     5
    // pos:  0     1     1     1     0     0
    //             buy               sell
    let closes = vec![100.0, 102.0, 105.0, 110.0, 108.0, 109.0];
    let positions = vec![0u8, 1, 1, 1, 0, 0];
    let fee = 0.0005;

    print_breakdown("Test 1 · single clean trade", &closes, &positions, fee);

    let engine = engine_total_return(&closes, &positions, fee);
    let trades = extract_trades(&closes, &positions);
    let exact = trade_product_exact(&trades, fee);

    assert!(
        (exact - engine).abs() < 1e-12,
        "formula (c) must match engine exactly: engine={engine}, exact={exact}"
    );
}

// ════════════════════════════════════════════════════════════════════
// Test 2: Multiple trades with idle gaps — price movement between
// trades is ignored by both engine (bar_return = 0 when pos=0) and
// trade-based formulas.  (c) still matches exactly.
// ════════════════════════════════════════════════════════════════════
#[test]
fn multiple_trades_with_idle_gaps_formula_c_matches_engine() {
    // 3 trades with idle gaps between.
    // bar: 0   1   2   3   4   5   6   7   8   9  10  11  12  13  14
    // pos: 0   1   1   0   0   1   1   1   0   0   1   1   0   0   0
    let closes = vec![
        100.0, 102.0, 105.0, 103.0, 108.0, 110.0, 112.0, 115.0, 113.0, 118.0, 120.0, 125.0, 122.0,
        119.0, 121.0,
    ];
    let positions = vec![0u8, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0];
    let fee = 0.0005;

    print_breakdown("Test 2 · 3 trades with idle gaps", &closes, &positions, fee);

    let engine = engine_total_return(&closes, &positions, fee);
    let trades = extract_trades(&closes, &positions);
    let exact = trade_product_exact(&trades, fee);

    assert!(
        (exact - engine).abs() < 1e-12,
        "formula (c) must match engine exactly: engine={engine}, exact={exact}"
    );
    assert_eq!(trades.len(), 3);
}

// ════════════════════════════════════════════════════════════════════
// Test 3: Open position at end — the unclosed trade contributes to
// engine's total_return (via bar_return on held bars) but does NOT
// appear in result.trades.  Both (a), (b), (c) diverge from the engine
// by the unrealized return of the open position.  This is THE most
// common cause of a visible Cum % vs Total Return gap.
// ════════════════════════════════════════════════════════════════════
#[test]
fn open_position_at_end_causes_divergence() {
    // bar: 0   1   2   3   4   5   6   7   8
    // pos: 0   1   1   0   0   1   1   1   1   ← last position still open
    let closes = vec![100.0, 102.0, 105.0, 103.0, 108.0, 110.0, 114.0, 118.0, 125.0];
    let positions = vec![0u8, 1, 1, 0, 0, 1, 1, 1, 1];
    let fee = 0.0005;

    print_breakdown(
        "Test 3 · open position at end",
        &closes,
        &positions,
        fee,
    );

    let engine = engine_total_return(&closes, &positions, fee);
    let trades = extract_trades(&closes, &positions);
    let exact = trade_product_exact(&trades, fee);

    // Closed trades: 1 only (bar 1→3).  The second "trade" (5→still open) is
    // excluded from `trades`, so all trade-based formulas miss that leg.
    assert_eq!(trades.len(), 1);

    // The unrealized return on the open position, applied as engine would:
    // bar 5 entry: factor (1 − f)
    // bars 6, 7, 8 hold: factor c_i / c_{i-1} each
    // no exit → no exit-bar fee
    let unrealized = (1.0 - fee) * (closes[6] / closes[5]) * (closes[7] / closes[6])
        * (closes[8] / closes[7]);
    let closed_leg_factor =
        (1.0 - fee) * (closes[2] / closes[1]) * (closes[3] / closes[2] - fee);
    let expected_engine = closed_leg_factor * unrealized - 1.0;

    eprintln!(
        "  expected engine (manual): {:.6}%  (closed × unrealized)",
        expected_engine * 100.0
    );

    assert!(
        (engine - expected_engine).abs() < 1e-12,
        "engine should = closed_leg × unrealized, got engine={engine}, expected={expected_engine}"
    );

    // formula (c) only accounts for closed trades → drift = (1 + engine) / (1 + exact) − 1
    //                                                       = unrealized_factor − 1 (approx)
    let drift = (1.0 + engine) / (1.0 + exact) - 1.0;
    eprintln!(
        "  drift (c vs engine) : {:+.6}%   (≈ unrealized factor − 1)",
        drift * 100.0
    );

    // Sanity: drift should be close to (unrealized − 1).
    assert!(
        (drift - (unrealized - 1.0)).abs() < 1e-12,
        "drift should equal unrealized − 1: drift={drift}, unrealized−1={}",
        unrealized - 1.0
    );
}

// ════════════════════════════════════════════════════════════════════
// Test 4: Demonstrate WHY formula (b) `1 + pnl − 2f` drifts from the
// engine on every trade, even when all trades close.
//
// Analytical drift per trade (with R = sell/buy, Q = c_{m-1}/buy):
//   factor_exact   = (1−f) · (R − fQ)         = R − fQ − fR + f²Q
//   factor_approx  = R − 2f
//   drift_per_trade = factor_approx − factor_exact
//                   = R − 2f − R + fQ + fR − f²Q
//                   = f · (Q + R − 2) − f²Q
//                   ≈ f · (Q + R − 2)  for small f
//                   ≈ 2f · pnl_pct     if Q ≈ R (flat last bar)
//
// So frontend formula (b) overstates winners and understates losers.
// ════════════════════════════════════════════════════════════════════
#[test]
fn formula_b_drift_analytic_match() {
    // Single trade, strong winner.
    let closes = vec![100.0, 110.0, 115.0, 120.0, 118.0, 119.0];
    let positions = vec![0u8, 1, 1, 1, 0, 0];
    let fee = 0.0005;

    print_breakdown(
        "Test 4 · formula (b) drift decomposition",
        &closes,
        &positions,
        fee,
    );

    let trades = extract_trades(&closes, &positions);
    let t = trades[0];
    let r = t.sell_price / t.buy_price;
    let q = t.close_m_minus_1 / t.buy_price;

    let factor_approx = 1.0 + pnl(&t) - 2.0 * fee;
    let factor_exact =
        (1.0 - fee) * (t.close_m_minus_1 / t.buy_price) * (t.sell_price / t.close_m_minus_1 - fee);
    let analytic_drift = fee * (q + r - 2.0) - fee * fee * q;
    let measured_drift = factor_approx - factor_exact;

    eprintln!(
        "  R = sell/buy         : {:.6}",
        r
    );
    eprintln!("  Q = c_{{m-1}}/buy      : {:.6}", q);
    eprintln!(
        "  analytic drift       : {:+.3e}   ( f·(Q+R−2) − f²·Q )",
        analytic_drift
    );
    eprintln!(
        "  measured drift       : {:+.3e}   ( factor_approx − factor_exact )",
        measured_drift
    );

    assert!(
        (analytic_drift - measured_drift).abs() < 1e-15,
        "analytic and measured drift must agree: analytic={analytic_drift}, measured={measured_drift}"
    );
}

// ════════════════════════════════════════════════════════════════════
// Test 5: Zero-fee scenario — all four formulas MUST agree exactly.
// If any disagrees, there is a bug in the engine or the formula.
// ════════════════════════════════════════════════════════════════════
#[test]
fn zero_fee_all_formulas_agree() {
    let closes = vec![100.0, 102.0, 105.0, 108.0, 106.0, 110.0, 107.0, 112.0, 115.0, 113.0];
    let positions = vec![0u8, 1, 1, 1, 0, 1, 1, 0, 0, 0];
    let fee = 0.0;

    print_breakdown("Test 5 · zero-fee parity check", &closes, &positions, fee);

    let engine = engine_total_return(&closes, &positions, fee);
    let trades = extract_trades(&closes, &positions);
    let gross = trade_product_gross(&trades);
    let approx = trade_product_approx(&trades, fee);
    let exact = trade_product_exact(&trades, fee);

    assert!((gross - engine).abs() < 1e-12);
    assert!((approx - engine).abs() < 1e-12);
    assert!((exact - engine).abs() < 1e-12);
}
