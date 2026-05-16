//! Integration test for `core::live_signal::resolve_live_signal`.
//!
//! Lives in tests/ rather than as `#[cfg(test)] mod tests` inside the source
//! file so it builds as a separate binary — sidesteps the
//! `STATUS_ENTRYPOINT_NOT_FOUND` issue this dev environment has when running
//! `cargo test --lib` (DLL load fails for the full `bitcoin_trader_lib`
//! cdylib but a leaf integration binary works).
//!
//! Mirrors the legacy C# `ResolveSignalFromSimulation` 12-case matrix
//! (2 positions × 6 simulation signal vocabularies).

use bitcoin_trader_lib::core::live_signal::resolve_live_signal;

#[test]
fn idle_with_buy_signal_buys() {
    assert_eq!(resolve_live_signal("buy", 0), "buy");
}

#[test]
fn idle_with_buy_ready_signals_buy_ready() {
    assert_eq!(resolve_live_signal("buy ready", 0), "buy ready");
}

#[test]
fn idle_with_sell_signal_does_nothing() {
    // Cash user: sim's sell signal is meaningless (nothing to sell).
    assert_eq!(resolve_live_signal("sell", 0), "ready");
}

#[test]
fn idle_with_sell_ready_does_nothing() {
    assert_eq!(resolve_live_signal("sell ready", 0), "ready");
}

#[test]
fn idle_with_hold_does_nothing() {
    // CRITICAL: cash user, sim says hold → must be "ready" not "hold".
    // This is the regression guard for "user just promoted to real but sim
    // claims to be holding because it would have bought 3 months ago".
    assert_eq!(resolve_live_signal("hold", 0), "ready");
}

#[test]
fn idle_with_ready_stays_ready() {
    assert_eq!(resolve_live_signal("ready", 0), "ready");
}

#[test]
fn holding_with_buy_signal_does_not_double_buy() {
    // Already holding: ignore further buy signals.
    assert_eq!(resolve_live_signal("buy", 1), "hold");
}

#[test]
fn holding_with_buy_ready_does_not_double_buy() {
    assert_eq!(resolve_live_signal("buy ready", 1), "hold");
}

#[test]
fn holding_with_sell_signal_sells() {
    assert_eq!(resolve_live_signal("sell", 1), "sell");
}

#[test]
fn holding_with_sell_ready_signals_sell_ready() {
    assert_eq!(resolve_live_signal("sell ready", 1), "sell ready");
}

#[test]
fn holding_with_hold_keeps_position() {
    assert_eq!(resolve_live_signal("hold", 1), "hold");
}

#[test]
fn holding_with_ready_keeps_position() {
    assert_eq!(resolve_live_signal("ready", 1), "hold");
}

#[test]
fn unknown_position_defaults_to_ready() {
    assert_eq!(resolve_live_signal("buy", 99), "ready");
    assert_eq!(resolve_live_signal("sell", -1), "ready");
}
