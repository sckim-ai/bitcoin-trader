use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use uuid::Uuid;

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
