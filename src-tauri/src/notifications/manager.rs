use rusqlite::Connection;
use super::discord::DiscordClient;
use super::fcm::FcmClient;
use super::telegram::TelegramClient;

pub struct NotificationManager {
    fcm: Option<FcmClient>,
    discord: Option<DiscordClient>,
    telegram: Option<TelegramClient>,
    /// FCM device token (stored separately from server key)
    fcm_device_token: String,
}

impl NotificationManager {
    /// Load notification configs from the DB for a given user.
    /// Each channel row has: channel TEXT, config TEXT (JSON), enabled INTEGER.
    pub fn from_db(conn: &Connection, user_id: i64) -> Self {
        let mut mgr = Self {
            fcm: None,
            discord: None,
            telegram: None,
            fcm_device_token: String::new(),
        };

        let mut stmt = match conn.prepare(
            "SELECT channel, config, enabled FROM notification_configs WHERE user_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return mgr,
        };

        let rows = match stmt.query_map([user_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return mgr,
        };

        for row in rows.flatten() {
            let (channel, config_json, enabled) = row;
            if enabled == 0 {
                continue;
            }
            let config: serde_json::Value =
                serde_json::from_str(&config_json).unwrap_or_default();

            match channel.as_str() {
                "fcm" => {
                    if let Some(key) = config["server_key"].as_str() {
                        mgr.fcm = Some(FcmClient::new(key.to_string()));
                        mgr.fcm_device_token = config["device_token"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                    }
                }
                "discord" => {
                    if let Some(url) = config["webhook_url"].as_str() {
                        mgr.discord = Some(DiscordClient::new(url.to_string()));
                    }
                }
                "telegram" => {
                    if let (Some(token), Some(chat_id)) =
                        (config["bot_token"].as_str(), config["chat_id"].as_str())
                    {
                        mgr.telegram =
                            Some(TelegramClient::new(token.to_string(), chat_id.to_string()));
                    }
                }
                _ => {}
            }
        }

        mgr
    }

    pub async fn notify_trade(
        &self,
        side: &str,
        market: &str,
        price: f64,
        volume: f64,
        pnl: Option<f64>,
    ) {
        let msg = match side {
            "buy" => format!("{} 매수: {:.0}원 x {:.6}", market, price, volume),
            "sell" => format!(
                "{} 매도: {:.0}원 (P/L: {:.2}%)",
                market,
                price,
                pnl.unwrap_or(0.0)
            ),
            _ => return,
        };
        self.send_all(&format!("🔔 {}", msg)).await;
    }

    pub async fn notify_signal(&self, market: &str, signal: &str, strategy: &str) {
        self.send_all(&format!("📊 {}: {} ({} 전략)", market, signal, strategy))
            .await;
    }

    pub async fn notify_alert(&self, message: &str) {
        self.send_all(&format!("⚠️ {}", message)).await;
    }

    async fn send_all(&self, message: &str) {
        if let Some(fcm) = &self.fcm {
            let _ = fcm
                .send(&self.fcm_device_token, "BTC Trader", message, "high")
                .await;
        }
        if let Some(discord) = &self.discord {
            let _ = discord.send(message).await;
        }
        if let Some(telegram) = &self.telegram {
            let _ = telegram.send(message).await;
        }
    }
}

/// Format a trade notification message (public for testing).
pub fn format_trade_message(side: &str, market: &str, price: f64, volume: f64, pnl: Option<f64>) -> Option<String> {
    match side {
        "buy" => Some(format!("🔔 {} 매수: {:.0}원 x {:.6}", market, price, volume)),
        "sell" => Some(format!("🔔 {} 매도: {:.0}원 (P/L: {:.2}%)", market, price, pnl.unwrap_or(0.0))),
        _ => None,
    }
}

/// Format a signal notification message (public for testing).
pub fn format_signal_message(market: &str, signal: &str, strategy: &str) -> String {
    format!("📊 {}: {} ({} 전략)", market, signal, strategy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let schema_v1 = include_str!("../../migrations/001_initial.sql");
        conn.execute_batch(schema_v1).unwrap();
        let schema_v2 = include_str!("../../migrations/002_users.sql");
        conn.execute_batch(schema_v2).unwrap();
        conn
    }

    #[test]
    fn test_from_db_empty() {
        let conn = setup_db();
        let mgr = NotificationManager::from_db(&conn, 1);
        assert!(mgr.fcm.is_none());
        assert!(mgr.discord.is_none());
        assert!(mgr.telegram.is_none());
    }

    #[test]
    fn test_from_db_with_configs() {
        let conn = setup_db();
        // Insert a test user first
        conn.execute(
            "INSERT INTO users (username, password_hash, role) VALUES ('test', 'hash', 'trader')",
            [],
        ).unwrap();

        // Insert notification configs
        conn.execute(
            "INSERT INTO notification_configs (user_id, channel, config, enabled) VALUES (1, 'discord', '{\"webhook_url\":\"https://discord.com/test\"}', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO notification_configs (user_id, channel, config, enabled) VALUES (1, 'telegram', '{\"bot_token\":\"tok\",\"chat_id\":\"123\"}', 1)",
            [],
        ).unwrap();
        // Disabled FCM
        conn.execute(
            "INSERT INTO notification_configs (user_id, channel, config, enabled) VALUES (1, 'fcm', '{\"server_key\":\"key\",\"device_token\":\"dt\"}', 0)",
            [],
        ).unwrap();

        let mgr = NotificationManager::from_db(&conn, 1);
        assert!(mgr.fcm.is_none()); // disabled
        assert!(mgr.discord.is_some());
        assert!(mgr.telegram.is_some());
    }

    #[test]
    fn test_format_trade_buy() {
        let msg = format_trade_message("buy", "KRW-BTC", 50000000.0, 0.001, None);
        assert!(msg.is_some());
        let msg = msg.unwrap();
        assert!(msg.contains("KRW-BTC"));
        assert!(msg.contains("매수"));
        assert!(msg.contains("50000000"));
    }

    #[test]
    fn test_format_trade_sell() {
        let msg = format_trade_message("sell", "KRW-BTC", 51000000.0, 0.001, Some(2.5));
        assert!(msg.is_some());
        let msg = msg.unwrap();
        assert!(msg.contains("매도"));
        assert!(msg.contains("2.50"));
    }

    #[test]
    fn test_format_trade_unknown_side() {
        let msg = format_trade_message("unknown", "KRW-BTC", 50000000.0, 0.001, None);
        assert!(msg.is_none());
    }

    #[test]
    fn test_format_signal() {
        let msg = format_signal_message("KRW-BTC", "매수 시그널", "V3");
        assert!(msg.contains("KRW-BTC"));
        assert!(msg.contains("매수 시그널"));
        assert!(msg.contains("V3 전략"));
    }
}
