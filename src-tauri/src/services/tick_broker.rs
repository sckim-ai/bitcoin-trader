use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TickData {
    pub market: String,
    pub price: f64,
    pub change_pct: f64,   // e.g., +1.24 => 1.24
    pub volume_24h: f64,
    pub ts_ms: i64,
}

/// Parse an Upbit ticker WebSocket message into TickData.
/// Returns None if the payload is not a ticker event or is malformed.
pub fn parse_ticker(bytes: &[u8]) -> Option<TickData> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    // Upbit ticker identifies itself with `"type":"ticker"` + market code in `code`.
    if v.get("type").and_then(|t| t.as_str()) != Some("ticker") {
        return None;
    }
    let market = v.get("code")?.as_str()?.to_string();
    let price = v.get("trade_price")?.as_f64()?;
    // Upbit: `signed_change_rate` is fractional (0.0124 == +1.24%).
    let change_rate = v.get("signed_change_rate").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let volume_24h = v.get("acc_trade_volume_24h").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let ts_ms = v.get("timestamp").and_then(|x| x.as_i64()).unwrap_or(0);

    Some(TickData {
        market,
        price,
        change_pct: change_rate * 100.0,
        volume_24h,
        ts_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_eth_ticker_event() {
        let msg = br#"{
            "type": "ticker",
            "code": "KRW-ETH",
            "trade_price": 3245000.0,
            "signed_change_rate": 0.0124,
            "acc_trade_volume_24h": 15234.5,
            "timestamp": 1714000000000
        }"#;
        let t = parse_ticker(msg).expect("should parse");
        assert_eq!(t.market, "KRW-ETH");
        assert!((t.price - 3_245_000.0).abs() < 0.01);
        assert!((t.change_pct - 1.24).abs() < 1e-6);
        assert!((t.volume_24h - 15234.5).abs() < 0.01);
        assert_eq!(t.ts_ms, 1_714_000_000_000);
    }

    #[test]
    fn rejects_non_ticker_event() {
        // Upbit also emits `"type":"trade"` and others.
        let msg = br#"{"type":"trade","code":"KRW-ETH"}"#;
        assert!(parse_ticker(msg).is_none());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_ticker(b"not json").is_none());
    }

    #[test]
    fn handles_negative_change_rate() {
        let msg = br#"{
            "type": "ticker",
            "code": "KRW-ETH",
            "trade_price": 3000000.0,
            "signed_change_rate": -0.02,
            "acc_trade_volume_24h": 5000.0,
            "timestamp": 1714000000000
        }"#;
        let t = parse_ticker(msg).expect("should parse");
        assert!((t.change_pct - (-2.0)).abs() < 1e-6);
    }

    #[test]
    fn missing_optional_fields_default_to_zero() {
        let msg = br#"{
            "type": "ticker",
            "code": "KRW-ETH",
            "trade_price": 3000000.0
        }"#;
        let t = parse_ticker(msg).expect("should parse");
        assert_eq!(t.change_pct, 0.0);
        assert_eq!(t.volume_24h, 0.0);
        assert_eq!(t.ts_ms, 0);
    }
}
