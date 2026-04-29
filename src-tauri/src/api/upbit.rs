use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use uuid::Uuid;

/// Subset of Upbit /v1/orders response fields we actually use.
/// All numeric fields ship as strings — Upbit returns "0.00150000" style
/// formatted decimals, not numbers. Optional everywhere because market
/// buys omit `volume` and market sells omit `price`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub uuid: String,
    pub side: String,         // "bid" | "ask"
    pub ord_type: String,     // "limit" | "price" | "market"
    pub state: String,        // "wait" | "done" | "cancel"
    pub market: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub volume: Option<String>,
    #[serde(default)]
    pub remaining_volume: Option<String>,
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default)]
    pub executed_volume: Option<String>,
    #[serde(default)]
    pub paid_fee: Option<String>,
    #[serde(default)]
    pub trades_count: Option<i32>,
}

impl OrderResponse {
    /// Helper: parse `executed_volume` as f64 (defaults to 0.0).
    pub fn executed_volume_f64(&self) -> f64 {
        self.executed_volume
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    }

    /// `state == "done"` — Upbit's "fully executed" indicator.
    pub fn is_done(&self) -> bool {
        self.state == "done"
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtPayload {
    access_key: String,
    nonce: String,
    timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    query_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query_hash_alg: Option<String>,
}

pub struct UpbitClient {
    access_key: String,
    secret_key: String,
    client: reqwest::Client,
}

impl UpbitClient {
    pub fn new(access_key: String, secret_key: String) -> Self {
        Self {
            access_key,
            secret_key,
            client: reqwest::Client::new(),
        }
    }

    /// Generate JWT token for Upbit API auth.
    /// If query_hash is provided, it's included in the payload for order endpoints.
    pub fn generate_token(
        &self,
        query_hash: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;

        let payload = JwtPayload {
            access_key: self.access_key.clone(),
            nonce: Uuid::new_v4().to_string(),
            timestamp: now,
            query_hash: query_hash.map(|h| h.to_string()),
            query_hash_alg: query_hash.map(|_| "SHA512".to_string()),
        };

        let token = encode(
            &Header::default(),
            &payload,
            &EncodingKey::from_secret(self.secret_key.as_bytes()),
        )?;

        Ok(token)
    }

    /// SHA512 hash of a query string for authenticated order requests.
    fn hash_query(query: &str) -> String {
        let mut hasher = Sha512::new();
        hasher.update(query.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    // ─── Market data (no auth) ───

    pub async fn get_current_price(
        &self,
        market: &str,
    ) -> Result<f64, Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.upbit.com/v1/ticker?markets={}",
            market
        );
        let resp: Vec<serde_json::Value> = self.client.get(&url).send().await?.json().await?;
        let price = resp
            .first()
            .and_then(|v| v.get("trade_price"))
            .and_then(|v| v.as_f64())
            .ok_or("Failed to parse price")?;
        Ok(price)
    }

    pub async fn get_candles(
        &self,
        market: &str,
        interval: &str,
        count: u32,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        self.get_candles_before(market, interval, count, None).await
    }

    /// Fetch candles with optional `to` parameter for pagination.
    /// `to`: ISO 8601 timestamp — returns candles before this time.
    pub async fn get_candles_before(
        &self,
        market: &str,
        interval: &str,
        count: u32,
        to: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let path = match interval {
            "1" | "3" | "5" | "15" | "30" | "60" | "240" => {
                format!("minutes/{}", interval)
            }
            "day" => "days".to_string(),
            "week" => "weeks".to_string(),
            _ => format!("minutes/{}", interval),
        };
        let mut url = format!(
            "https://api.upbit.com/v1/candles/{}?market={}&count={}",
            path, market, count
        );
        if let Some(to_ts) = to {
            url.push_str(&format!("&to={}", to_ts));
        }
        let resp: Vec<serde_json::Value> = self.client.get(&url).send().await?.json().await?;
        Ok(resp)
    }

    // ─── Trading (auth needed) ───

    pub async fn place_limit_buy(
        &self,
        market: &str,
        volume: f64,
        price: f64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.place_order(market, "bid", volume, price).await
    }

    pub async fn place_limit_sell(
        &self,
        market: &str,
        volume: f64,
        price: f64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.place_order(market, "ask", volume, price).await
    }

    async fn place_order(
        &self,
        market: &str,
        side: &str,
        volume: f64,
        price: f64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let query = format!(
            "market={}&side={}&volume={}&price={}&ord_type=limit",
            market, side, volume, price
        );
        let query_hash = Self::hash_query(&query);
        let token = self.generate_token(Some(&query_hash))?;

        let body = serde_json::json!({
            "market": market,
            "side": side,
            "volume": volume.to_string(),
            "price": price.to_string(),
            "ord_type": "limit",
        });

        let resp: serde_json::Value = self
            .client
            .post("https://api.upbit.com/v1/orders")
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        Ok(resp)
    }

    // ─── Market orders + parsed responses (Phase 4A.4) ─────────────────────
    //
    // Upbit market-order semantics:
    //   - Market BUY: ord_type="price", price=KRW_AMOUNT (no volume).
    //     Buy as much coin as `price` KRW will get at current ask.
    //   - Market SELL: ord_type="market", volume=COIN_AMOUNT (no price).
    //     Sell `volume` coin at current bid.
    //
    // Errors are stringified up front so callers can use the result inside
    // async tasks without dragging non-Send dyn Error trait objects across
    // .await boundaries.

    /// Market BUY using KRW. `krw_amount` is the total quote currency to spend.
    pub async fn place_market_buy(
        &self,
        market: &str,
        krw_amount: f64,
    ) -> Result<OrderResponse, String> {
        // Upbit truncates to integer KRW; pass as integer string to avoid
        // "Decimal precision" rejection.
        let price_str = format!("{}", krw_amount.floor() as u64);
        let query = format!(
            "market={}&side=bid&price={}&ord_type=price",
            market, price_str
        );
        let query_hash = Self::hash_query(&query);
        let token = self
            .generate_token(Some(&query_hash))
            .map_err(|e| format!("token: {e}"))?;

        let body = serde_json::json!({
            "market": market,
            "side": "bid",
            "price": price_str,
            "ord_type": "price",
        });

        self.send_order(&token, body).await
    }

    /// Market SELL given a coin volume.
    pub async fn place_market_sell(
        &self,
        market: &str,
        volume: f64,
    ) -> Result<OrderResponse, String> {
        // Volume kept at 8-decimal precision (Upbit max).
        let volume_str = format!("{:.8}", volume);
        let query = format!(
            "market={}&side=ask&volume={}&ord_type=market",
            market, volume_str
        );
        let query_hash = Self::hash_query(&query);
        let token = self
            .generate_token(Some(&query_hash))
            .map_err(|e| format!("token: {e}"))?;

        let body = serde_json::json!({
            "market": market,
            "side": "ask",
            "volume": volume_str,
            "ord_type": "market",
        });

        self.send_order(&token, body).await
    }

    async fn send_order(
        &self,
        token: &str,
        body: serde_json::Value,
    ) -> Result<OrderResponse, String> {
        let resp = self
            .client
            .post("https://api.upbit.com/v1/orders")
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("send: {e}"))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body read: {e}"))?;
        if !status.is_success() {
            return Err(format!("Upbit {} — {}", status, text));
        }
        serde_json::from_str::<OrderResponse>(&text)
            .map_err(|e| format!("parse OrderResponse: {e} — body: {text}"))
    }

    /// Cancel an open order by uuid. Used by 4A.5's stale-order timeout.
    /// Returns the cancelled order's final state.
    pub async fn cancel_order(&self, uuid: &str) -> Result<OrderResponse, String> {
        let query = format!("uuid={}", uuid);
        let query_hash = Self::hash_query(&query);
        let token = self
            .generate_token(Some(&query_hash))
            .map_err(|e| format!("token: {e}"))?;

        let url = format!("https://api.upbit.com/v1/order?uuid={}", uuid);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("send: {e}"))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body read: {e}"))?;
        if !status.is_success() {
            return Err(format!("Upbit {} — {}", status, text));
        }
        serde_json::from_str::<OrderResponse>(&text)
            .map_err(|e| format!("parse OrderResponse: {e} — body: {text}"))
    }

    /// Fetch a single order's current state by uuid. Used by 4A.5 to track
    /// pending limit orders and by 4A.4 to confirm execution.
    pub async fn get_order(&self, uuid: &str) -> Result<OrderResponse, String> {
        let query = format!("uuid={}", uuid);
        let query_hash = Self::hash_query(&query);
        let token = self
            .generate_token(Some(&query_hash))
            .map_err(|e| format!("token: {e}"))?;

        let url = format!("https://api.upbit.com/v1/order?uuid={}", uuid);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("send: {e}"))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body read: {e}"))?;
        if !status.is_success() {
            return Err(format!("Upbit {} — {}", status, text));
        }
        serde_json::from_str::<OrderResponse>(&text)
            .map_err(|e| format!("parse OrderResponse: {e} — body: {text}"))
    }

    // ─── Account (auth needed) ───

    pub async fn get_balance(
        &self,
        currency: &str,
    ) -> Result<f64, Box<dyn std::error::Error>> {
        let balances = self.get_all_balances().await?;
        let balance = balances
            .iter()
            .find(|b| {
                b.get("currency")
                    .and_then(|v| v.as_str())
                    .map(|c| c == currency)
                    .unwrap_or(false)
            })
            .and_then(|b| b.get("balance"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(balance)
    }

    pub async fn get_all_balances(
        &self,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let token = self.generate_token(None)?;
        let resp: Vec<serde_json::Value> = self
            .client
            .get("https://api.upbit.com/v1/accounts")
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_client() {
        let client = UpbitClient::new("test_access".to_string(), "test_secret".to_string());
        assert_eq!(client.access_key, "test_access");
        assert_eq!(client.secret_key, "test_secret");
    }

    #[test]
    fn test_generate_token_without_query_hash() {
        let client = UpbitClient::new("my_access_key".to_string(), "my_secret_key".to_string());
        let token = client.generate_token(None).expect("should generate token");
        // JWT has 3 parts separated by dots
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn test_generate_token_with_query_hash() {
        let client = UpbitClient::new("my_access_key".to_string(), "my_secret_key".to_string());
        let hash = UpbitClient::hash_query("market=KRW-BTC&side=bid&volume=1&price=100&ord_type=limit");
        let token = client
            .generate_token(Some(&hash))
            .expect("should generate token with hash");
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn test_hash_query() {
        let hash = UpbitClient::hash_query("test_query");
        // SHA512 produces 128 hex characters
        assert_eq!(hash.len(), 128);
        // Same input should produce same hash
        assert_eq!(hash, UpbitClient::hash_query("test_query"));
        // Different input should produce different hash
        assert_ne!(hash, UpbitClient::hash_query("other_query"));
    }
}
