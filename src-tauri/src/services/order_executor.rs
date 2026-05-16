//! Split-order execution — direct port of legacy
//! `LiveTradingService.ExecuteSplitBuy/SellAsync` (LiveTradingService.cs:1427-1555).
//!
//! Why split: large single-shot market orders eat through several order-book
//! levels and pay an avoidable slippage premium. Splitting into three chunks
//! with 2s gaps gives the book time to refill at tighter spreads.
//!
//! Constants are intentionally identical to the legacy values so the first
//! real run matches the validated paper-vs-real behaviour from the C#
//! deployment.

use crate::api::upbit::{OrderResponse, UpbitClient};
use std::time::Duration;

/// 50만원 이상 매수/매도면 분할. 그 미만은 단일 주문.
pub const SPLIT_THRESHOLD_KRW: f64 = 500_000.0;
/// 분할 횟수.
pub const SPLIT_COUNT: usize = 3;
/// 분할 사이 대기 시간.
pub const SPLIT_INTERVAL_MS: u64 = 2000;
/// Upbit 최소 주문 금액 (시장가).
pub const MIN_ORDER_KRW: f64 = 5_000.0;

/// Result of a multi-chunk order. `orders` may have fewer entries than
/// `SPLIT_COUNT` when individual chunks failed; `success` is true if at
/// least one chunk filled.
#[derive(Debug, Default)]
pub struct SplitOrderResult {
    pub orders: Vec<OrderResponse>,
    pub success: bool,
    /// Errors collected from failed chunks (for logging).
    pub errors: Vec<String>,
}

/// Compute the per-chunk KRW amounts for a market BUY split. Last chunk
/// absorbs the remainder so chunks sum to the exact total.
pub fn split_buy_chunks(total_krw: f64) -> Vec<f64> {
    if total_krw < SPLIT_THRESHOLD_KRW
        || total_krw / (SPLIT_COUNT as f64) < MIN_ORDER_KRW
    {
        return vec![total_krw];
    }
    let per = (total_krw / SPLIT_COUNT as f64).floor();
    let remainder = total_krw - per * (SPLIT_COUNT as f64 - 1.0);
    let mut out: Vec<f64> = (0..SPLIT_COUNT - 1).map(|_| per).collect();
    out.push(remainder);
    out.into_iter().filter(|a| *a >= MIN_ORDER_KRW).collect()
}

/// Compute the per-chunk volume amounts for a market SELL split. The
/// `current_price` is used only to gate sub-MIN_ORDER chunks.
pub fn split_sell_chunks(total_volume: f64, current_price: f64) -> Vec<f64> {
    let total_value_krw = total_volume * current_price;
    if total_value_krw < SPLIT_THRESHOLD_KRW {
        return vec![total_volume];
    }
    // 8-decimal truncation to match Upbit precision.
    let per = ((total_volume / SPLIT_COUNT as f64) * 1e8).floor() / 1e8;
    let remainder = total_volume - per * (SPLIT_COUNT as f64 - 1.0);
    let mut out: Vec<f64> = (0..SPLIT_COUNT - 1).map(|_| per).collect();
    out.push(remainder);
    out.into_iter()
        .filter(|v| *v * current_price >= MIN_ORDER_KRW)
        .collect()
}

/// Execute a split BUY. If `target_price > 0` → limit orders at target_price
/// (post-4A.7 default policy: peg at the last bar's close). If 0 → market.
pub async fn execute_split_buy(
    client: &UpbitClient,
    market: &str,
    total_krw: f64,
    target_price: f64,
) -> SplitOrderResult {
    let chunks = split_buy_chunks(total_krw);
    let n = chunks.len();
    let mut out = SplitOrderResult::default();
    for (i, amount) in chunks.iter().enumerate() {
        let result = if target_price > 0.0 {
            // Limit: derive volume from KRW / target_price (8-decimal floor).
            let volume = (amount / target_price * 1e8).floor() / 1e8;
            client.place_limit_buy_typed(market, volume, target_price).await
        } else {
            client.place_market_buy(market, *amount).await
        };
        match result {
            Ok(order) => {
                out.orders.push(order);
                out.success = true;
            }
            Err(e) => {
                out.errors.push(format!("buy chunk {}/{}: {}", i + 1, n, e));
                crate::live_log!("[order_executor] buy chunk {}/{} failed: {}", i + 1, n, e);
            }
        }
        if i + 1 < n {
            tokio::time::sleep(Duration::from_millis(SPLIT_INTERVAL_MS)).await;
        }
    }
    out
}

/// Execute a split SELL. `target_price > 0` → limit, 0 → market.
pub async fn execute_split_sell(
    client: &UpbitClient,
    market: &str,
    total_volume: f64,
    current_price: f64,
    target_price: f64,
) -> SplitOrderResult {
    let chunks = split_sell_chunks(total_volume, current_price);
    let n = chunks.len();
    let mut out = SplitOrderResult::default();
    for (i, vol) in chunks.iter().enumerate() {
        let result = if target_price > 0.0 {
            client.place_limit_sell_typed(market, *vol, target_price).await
        } else {
            client.place_market_sell(market, *vol).await
        };
        match result {
            Ok(order) => {
                out.orders.push(order);
                out.success = true;
            }
            Err(e) => {
                out.errors.push(format!("sell chunk {}/{}: {}", i + 1, n, e));
                crate::live_log!("[order_executor] sell chunk {}/{} failed: {}", i + 1, n, e);
            }
        }
        if i + 1 < n {
            tokio::time::sleep(Duration::from_millis(SPLIT_INTERVAL_MS)).await;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Buy chunk math ───

    #[test]
    fn buy_below_threshold_is_single_order() {
        let chunks = split_buy_chunks(100_000.0);
        assert_eq!(chunks.len(), 1);
        assert!((chunks[0] - 100_000.0).abs() < 0.5);
    }

    #[test]
    fn buy_at_threshold_boundary_still_single() {
        // < threshold → single
        let chunks = split_buy_chunks(499_999.0);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn buy_above_threshold_splits_into_three() {
        let chunks = split_buy_chunks(600_000.0);
        assert_eq!(chunks.len(), 3);
        let sum: f64 = chunks.iter().sum();
        assert!((sum - 600_000.0).abs() < 1.0);
    }

    #[test]
    fn buy_split_remainder_goes_to_last_chunk() {
        // 600_001 / 3 = 200_000.33... → floor 200_000, last = 600_001 - 200_000*2 = 200_001
        let chunks = split_buy_chunks(600_001.0);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], 200_000.0);
        assert_eq!(chunks[1], 200_000.0);
        assert!((chunks[2] - 200_001.0).abs() < 0.5);
    }

    // ─── Sell chunk math ───

    #[test]
    fn sell_below_threshold_value_is_single() {
        // 0.1 ETH × 3,000,000 = 300_000 < 500_000 → single
        let chunks = split_sell_chunks(0.1, 3_000_000.0);
        assert_eq!(chunks.len(), 1);
        assert!((chunks[0] - 0.1).abs() < 1e-10);
    }

    #[test]
    fn sell_above_threshold_splits() {
        // 0.2 ETH × 3,000,000 = 600_000 → split
        let chunks = split_sell_chunks(0.2, 3_000_000.0);
        assert_eq!(chunks.len(), 3);
        let sum: f64 = chunks.iter().sum();
        assert!((sum - 0.2).abs() < 1e-7);
    }

    // ─── OrderResponse parsing ───

    #[test]
    fn order_response_done_state() {
        let json = r#"{
            "uuid": "abc-123", "side": "bid", "ord_type": "price",
            "state": "done", "market": "KRW-ETH",
            "executed_volume": "0.12345678",
            "trades_count": 2
        }"#;
        let r: OrderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.uuid, "abc-123");
        assert!(r.is_done());
        assert!((r.executed_volume_f64() - 0.12345678).abs() < 1e-9);
    }

    #[test]
    fn order_response_wait_state_market_buy_no_volume() {
        // Market buy responses often omit `volume` (it's price-driven).
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
}
