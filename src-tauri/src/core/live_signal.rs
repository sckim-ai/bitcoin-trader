//! Live signal resolution — bridges simulation output (`SimulationResult`) and
//! the actual Upbit position into a "what action to take now" decision.
//!
//! Direct port of legacy `D:\SW\Bitcoin\Strategies\StrategyRegistry.cs`
//! `VolumeDecayStrategy.ResolveSignalFromSimulation`. The whole point is the
//! split between "the simulation's preferred state" (which assumes the user
//! has been running this preset since `since_ts`) and "what's actually in
//! the user's Upbit account right now". The simulation may want to be in
//! a holding state because it would have bought 3 months ago, but if the
//! user only just promoted this session to real *today* and is sitting in
//! cash, we must NOT auto-buy on cycle 1 — we wait for a fresh buy signal.
//!
//! Vocabulary: simulation produces six signal types via
//! `core::engine::determine_signal_type` —
//!   "ready" / "buy ready" / "buy" / "hold" / "sell ready" / "sell"
//!
//! Resolution rules (from legacy):
//!   position == 0 (idle, holding cash):
//!     "buy"       → "buy"        (place buy order)
//!     "buy ready" → "buy ready"  (notify only, no order)
//!     anything else → "ready"    (do nothing — protects against auto-buying
//!                                  when sim says "hold" because of an earlier
//!                                  virtual entry the user wasn't part of)
//!   position == 1 (holding coin):
//!     "sell"       → "sell"       (place sell order)
//!     "sell ready" → "sell ready" (notify only)
//!     anything else → "hold"      (don't double-buy; just keep position)

use crate::models::market::MarketData;
use crate::models::trading::SimulationResult;
use chrono::{DateTime, Duration, Utc};

/// Last transition signal in the simulation's signal log. The log only
/// contains transitions, so the last entry's type is the prevailing state
/// at the final candle. Defaults to "ready" if the log is empty (sim too
/// short or strategy emitted nothing).
pub fn last_signal_from_simulation(result: &SimulationResult) -> &str {
    result
        .signal_log
        .last()
        .map(|e| e.signal_type.as_str())
        .unwrap_or("ready")
}

/// Drop the trailing candle if it hasn't completed yet — the most recent
/// hour-candle is "still forming" until the next hour boundary, and running
/// the simulation on a half-formed candle produces unstable signals that
/// flicker as ticks come in.
///
/// Port of legacy `LiveTradingService.FilterConfirmedCandles` (LiveTradingService.cs:1324-1345).
/// Interval is auto-inferred from the last two candles' timestamps so the
/// helper works for hour/day/week without configuration.
///
/// Returns the input slice's data with the last element removed when it's
/// still inside its candle window. If the input is too short to infer, the
/// data is returned untouched.
pub fn filter_confirmed_candles(data: Vec<MarketData>) -> Vec<MarketData> {
    if data.len() < 2 {
        return data;
    }
    let last = &data[data.len() - 1].candle;
    let prev = &data[data.len() - 2].candle;
    let interval = last.timestamp - prev.timestamp;
    // Default to 1h if the inferred interval is non-positive (shouldn't
    // happen with sorted candles, but guard against bad data).
    let interval = if interval <= Duration::zero() {
        Duration::hours(1)
    } else {
        interval
    };
    let candle_end: DateTime<Utc> = last.timestamp + interval;
    if Utc::now() < candle_end {
        // Last candle still forming — drop it.
        let mut trimmed = data;
        trimmed.pop();
        trimmed
    } else {
        data
    }
}

/// Map (real position, sim signal) → live action signal. See module docs for
/// the why; this is intentionally narrow and deterministic so the matrix can
/// be unit-tested exhaustively.
pub fn resolve_live_signal(sim_signal: &str, position: i32) -> &'static str {
    match (position, sim_signal) {
        (0, "buy")        => "buy",
        (0, "buy ready")  => "buy ready",
        (0, _)            => "ready",
        (1, "sell")       => "sell",
        (1, "sell ready") => "sell ready",
        (1, _)            => "hold",
        // Unknown position (defensive) — treat as idle.
        _ => "ready",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::trading::{SignalEvent, SimulationResult};

    // ─── 12-case matrix: 2 positions × 6 signal vocabularies ───
    //
    // The legacy contract is asymmetric on purpose:
    //   - `position=0 + sim "hold"` becomes "ready" (NOT "hold") because the
    //     user is in cash; the sim's hold means "the virtual position is
    //     held", which is irrelevant when there's no actual coin.
    //   - `position=1 + sim "buy"` becomes "hold" (NOT "buy") because the
    //     user already has coin; another buy would over-allocate.

    #[test]
    fn idle_buy() { assert_eq!(resolve_live_signal("buy", 0), "buy"); }
    #[test]
    fn idle_buy_ready() { assert_eq!(resolve_live_signal("buy ready", 0), "buy ready"); }
    #[test]
    fn idle_sell() { assert_eq!(resolve_live_signal("sell", 0), "ready"); }
    #[test]
    fn idle_sell_ready() { assert_eq!(resolve_live_signal("sell ready", 0), "ready"); }
    #[test]
    fn idle_hold() { assert_eq!(resolve_live_signal("hold", 0), "ready"); }
    #[test]
    fn idle_ready() { assert_eq!(resolve_live_signal("ready", 0), "ready"); }

    #[test]
    fn holding_buy() { assert_eq!(resolve_live_signal("buy", 1), "hold"); }
    #[test]
    fn holding_buy_ready() { assert_eq!(resolve_live_signal("buy ready", 1), "hold"); }
    #[test]
    fn holding_sell() { assert_eq!(resolve_live_signal("sell", 1), "sell"); }
    #[test]
    fn holding_sell_ready() { assert_eq!(resolve_live_signal("sell ready", 1), "sell ready"); }
    #[test]
    fn holding_hold() { assert_eq!(resolve_live_signal("hold", 1), "hold"); }
    #[test]
    fn holding_ready() { assert_eq!(resolve_live_signal("ready", 1), "hold"); }

    #[test]
    fn unknown_position_defaults_to_ready() {
        assert_eq!(resolve_live_signal("buy", 99), "ready");
        assert_eq!(resolve_live_signal("sell", -1), "ready");
    }

    #[test]
    fn last_signal_empty_log_is_ready() {
        let result = SimulationResult::default();
        assert_eq!(last_signal_from_simulation(&result), "ready");
    }

    #[test]
    fn last_signal_picks_final_entry() {
        let mut result = SimulationResult::default();
        result.signal_log.push(SignalEvent {
            index: 0,
            timestamp: "2026-04-29T00:00:00Z".into(),
            signal_type: "buy ready".into(),
            price: 100.0,
            position: 0,
        });
        result.signal_log.push(SignalEvent {
            index: 1,
            timestamp: "2026-04-29T01:00:00Z".into(),
            signal_type: "buy".into(),
            price: 102.0,
            position: 1,
        });
        assert_eq!(last_signal_from_simulation(&result), "buy");
    }

    /// Critical regression: this is THE "user just promoted to real, sim
    /// says holding because it would have bought 3 months ago" case. We
    /// must return "ready" (no action), NOT "hold".
    #[test]
    fn cash_user_with_sim_holding_is_ready_not_hold() {
        // Sim's last log entry is "hold" because it has been holding for
        // many bars. User is in cash (position=0).
        assert_eq!(resolve_live_signal("hold", 0), "ready");
    }

    // ─── filter_confirmed_candles ───
    use crate::models::market::{Candle, IndicatorSet};

    fn mk_md(ts: DateTime<Utc>) -> MarketData {
        MarketData {
            candle: Candle {
                timestamp: ts,
                open: 100.0, high: 100.0, low: 100.0, close: 100.0, volume: 0.0,
            },
            indicators: IndicatorSet::default(),
        }
    }

    #[test]
    fn filter_drops_unfinished_last_candle() {
        // Last candle started 30 minutes ago, hour interval → still forming.
        let now = Utc::now();
        let prev = now - Duration::hours(1) - Duration::minutes(30);
        let last = now - Duration::minutes(30);
        let data = vec![mk_md(prev), mk_md(last)];
        let out = filter_confirmed_candles(data);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].candle.timestamp, prev);
    }

    #[test]
    fn filter_keeps_finished_last_candle() {
        // Last candle's window has fully passed (started 90 min ago → ended 30 min ago).
        let now = Utc::now();
        let prev = now - Duration::hours(2) - Duration::minutes(30);
        let last = now - Duration::hours(1) - Duration::minutes(30);
        let data = vec![mk_md(prev), mk_md(last)];
        let out = filter_confirmed_candles(data);
        assert_eq!(out.len(), 2); // both retained
    }

    #[test]
    fn filter_too_short_input_returns_as_is() {
        let now = Utc::now();
        let data = vec![mk_md(now)];
        let out = filter_confirmed_candles(data);
        assert_eq!(out.len(), 1);
    }
}
