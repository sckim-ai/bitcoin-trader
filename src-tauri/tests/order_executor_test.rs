//! Phase 4A.4 — split-chunk math + OrderResponse parsing.
//!
//! Lives as an integration test (not `#[cfg(test)] mod tests`) so it runs
//! independently of the lib bin — the dev environment has a
//! STATUS_ENTRYPOINT_NOT_FOUND issue with cargo test --lib that doesn't
//! affect leaf integration binaries.
//!
//! No live network calls — only the pure split math + JSON parsing. Real
//! order execution is validated manually with a small test account.

use bitcoin_trader_lib::api::upbit::OrderResponse;
use bitcoin_trader_lib::services::order_executor::{
    split_buy_chunks, split_sell_chunks, MIN_ORDER_KRW, SPLIT_THRESHOLD_KRW,
};

// ─── Buy splits ───────────────────────────────────────────────────────────

#[test]
fn buy_below_threshold_is_single_order() {
    let chunks = split_buy_chunks(100_000.0);
    assert_eq!(chunks.len(), 1);
    assert!((chunks[0] - 100_000.0).abs() < 0.5);
}

#[test]
fn buy_at_threshold_minus_one_is_single() {
    let chunks = split_buy_chunks(SPLIT_THRESHOLD_KRW - 1.0);
    assert_eq!(chunks.len(), 1);
}

#[test]
fn buy_at_threshold_plus_one_splits() {
    let chunks = split_buy_chunks(SPLIT_THRESHOLD_KRW + 1.0);
    assert_eq!(chunks.len(), 3);
    let sum: f64 = chunks.iter().sum();
    assert!((sum - (SPLIT_THRESHOLD_KRW + 1.0)).abs() < 1.5);
}

#[test]
fn buy_above_threshold_three_chunks_summing_to_total() {
    let total = 1_000_000.0;
    let chunks = split_buy_chunks(total);
    assert_eq!(chunks.len(), 3);
    let sum: f64 = chunks.iter().sum();
    assert!((sum - total).abs() < 1.0);
}

#[test]
fn buy_below_min_per_chunk_collapses_to_single() {
    // Even though total is technically >= threshold, divisor would push
    // each chunk under MIN_ORDER_KRW. (Not relevant in practice with the
    // 500K threshold + 5K min, but defensive.)
    let chunks = split_buy_chunks(MIN_ORDER_KRW * 2.0);
    assert_eq!(chunks.len(), 1);
}

// ─── Sell splits ──────────────────────────────────────────────────────────

#[test]
fn sell_below_value_threshold_is_single() {
    // 0.1 ETH × 3,000,000 KRW = 300,000 KRW < 500,000 → single
    let chunks = split_sell_chunks(0.1, 3_000_000.0);
    assert_eq!(chunks.len(), 1);
    assert!((chunks[0] - 0.1).abs() < 1e-10);
}

#[test]
fn sell_above_threshold_splits() {
    // 0.2 ETH × 3,000,000 = 600,000 KRW → split
    let chunks = split_sell_chunks(0.2, 3_000_000.0);
    assert_eq!(chunks.len(), 3);
    let sum: f64 = chunks.iter().sum();
    assert!((sum - 0.2).abs() < 1e-7);
}

#[test]
fn sell_split_volumes_truncated_to_8_decimals() {
    // 0.99999999 / 3 = 0.33333333 (truncated)
    let chunks = split_sell_chunks(0.99999999, 3_000_000.0);
    assert_eq!(chunks.len(), 3);
    // Per-chunk should be 8-decimal representable.
    for c in &chunks {
        let scaled = (c * 1e8).round();
        assert!((c * 1e8 - scaled).abs() < 1e-3);
    }
}

// ─── OrderResponse parsing ────────────────────────────────────────────────

#[test]
fn parse_done_market_buy_response() {
    let json = r#"{
        "uuid": "abc-123", "side": "bid", "ord_type": "price",
        "state": "done", "market": "KRW-ETH",
        "executed_volume": "0.12345678",
        "trades_count": 2,
        "paid_fee": "150"
    }"#;
    let r: OrderResponse = serde_json::from_str(json).unwrap();
    assert_eq!(r.uuid, "abc-123");
    assert!(r.is_done());
    assert!((r.executed_volume_f64() - 0.12345678).abs() < 1e-9);
}

#[test]
fn parse_wait_market_buy_omits_volume() {
    // Real Upbit immediate response for market buy — settling.
    let json = r#"{
        "uuid": "x", "side": "bid", "ord_type": "price",
        "state": "wait", "market": "KRW-ETH",
        "price": "100000"
    }"#;
    let r: OrderResponse = serde_json::from_str(json).unwrap();
    assert_eq!(r.state, "wait");
    assert!(!r.is_done());
    assert!(r.volume.is_none());
    assert_eq!(r.executed_volume_f64(), 0.0);
}

#[test]
fn parse_market_sell_response() {
    let json = r#"{
        "uuid": "sell-1", "side": "ask", "ord_type": "market",
        "state": "done", "market": "KRW-ETH",
        "volume": "0.05000000",
        "executed_volume": "0.05000000",
        "trades_count": 1
    }"#;
    let r: OrderResponse = serde_json::from_str(json).unwrap();
    assert_eq!(r.side, "ask");
    assert!(r.is_done());
    assert!((r.executed_volume_f64() - 0.05).abs() < 1e-9);
}
