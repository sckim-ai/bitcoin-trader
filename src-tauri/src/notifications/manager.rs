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

    /// "매수 대기" / "매도 대기" 알림. 신호 발생 직후 한 번만 보내도록
    /// caller가 상태 비교 후 호출해야 함 (중복 방지는 호출자 책임).
    /// session_label은 다중 세션 운용 환경에서 어떤 세션의 신호인지 식별.
    pub async fn notify_ready(
        &self,
        market: &str,
        side: &str,
        target_price: f64,
        session_label: Option<&str>,
    ) {
        let (icon, label) = match side {
            "buy" => ("🔵", "매수 대기"),
            "sell" => ("🟠", "매도 대기"),
            _ => return,
        };
        let session_part = match session_label {
            Some(s) if !s.is_empty() => format!("\n  ↳ session: {}", s),
            _ => String::new(),
        };
        self.send_all(&format!(
            "{} {} {} (close ≈ {:.0}원){}",
            icon, market, label, target_price, session_part,
        )).await;
    }

    /// Limit 주문이 호가창에 등록만 되고 미체결 상태일 때. 매수/매도 분리.
    pub async fn notify_order_registered(
        &self,
        market: &str,
        side: &str,
        target_price: f64,
        session_label: Option<&str>,
    ) {
        let (icon, label) = match side {
            "buy" => ("📥", "매수 주문 등록"),
            "sell" => ("📤", "매도 주문 등록"),
            _ => return,
        };
        let session_part = match session_label {
            Some(s) if !s.is_empty() => format!("\n  ↳ session: {}", s),
            _ => String::new(),
        };
        self.send_all(&format!(
            "{} {} {} (지정가 {:.0}원, wait){}",
            icon, market, label, target_price, session_part,
        )).await;
    }

    /// 매수/매도 후 즉시 체결(or 추정 booking) 알림. P/L은 sell만 의미.
    /// `late=true`면 동기 cycle이 아닌 tracker가 늦게 잡은 fill — 다른 emoji.
    pub async fn notify_trade_rich(
        &self,
        side: &str,
        market: &str,
        price: f64,
        volume: f64,
        pnl_pct: Option<f64>,
        note: Option<&str>,
    ) {
        self.notify_trade_full(side, market, price, volume, pnl_pct, note, false).await;
    }

    /// notify_trade_rich와 같지만 late=true면 ⏱ 아이콘으로 시각 구분.
    pub async fn notify_trade_full(
        &self,
        side: &str,
        market: &str,
        price: f64,
        volume: f64,
        pnl_pct: Option<f64>,
        note: Option<&str>,
        late: bool,
    ) {
        let icon = match (side, late) {
            ("buy", false)  => "🟢",
            ("buy", true)   => "⏱🟢",
            ("sell", false) => "🔴",
            ("sell", true)  => "⏱🔴",
            _ => return,
        };
        let head = match side {
            "buy" => format!("{} {} 매수: {:.0}원 × {:.8}", icon, market, price, volume),
            "sell" => format!(
                "{} {} 매도: {:.0}원 × {:.8} (P/L: {:+.2}%)",
                icon, market, price, volume, pnl_pct.unwrap_or(0.0),
            ),
            _ => return,
        };
        // KST 자동 변환 — Discord 자체 타임스탬프와 별개로, 메시지 본문에도
        // 명시되어 있으면 사후 분석 시 “이 메시지가 언제의 cycle인지” 즉시 인지.
        let kst = (chrono::Utc::now() + chrono::Duration::hours(9))
            .format("%Y-%m-%d %H:%M:%S KST")
            .to_string();
        let mut msg = head;
        msg.push_str(&format!("\n  ⏰ {}", kst));
        if let Some(n) = note {
            if !n.is_empty() {
                msg.push_str(&format!("\n  ↳ {}", n));
            }
        }
        self.send_all(&msg).await;
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
