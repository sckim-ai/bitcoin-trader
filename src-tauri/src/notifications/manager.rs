use rusqlite::Connection;
use serde_json::json;
use super::discord::DiscordClient;
use super::fcm::FcmClient;
use super::telegram::TelegramClient;

/// Optional balance + session context for trade notifications.
/// Direct port of legacy `SendTradeWithBalanceNotificationAsync`. When
/// available, the Discord embed shows wallet state ("after this trade
/// you hold X KRW + Y ETH") and session P/L — so the user doesn't need
/// to open the Upbit app to answer "where am I now?".
#[derive(Debug, Clone, Default)]
pub struct TradeContext<'a> {
    pub krw_balance: Option<f64>,
    pub coin_balance: Option<f64>,
    pub coin_currency: Option<&'a str>,
    pub coin_price: Option<f64>,        // for coin_balance × price valuation
    pub total_value_krw: Option<f64>,   // krw + coin × price
    pub session_label: Option<&'a str>,
    pub session_initial: Option<f64>,
    pub session_pnl_pct: Option<f64>,
    pub note: Option<&'a str>,
}

pub struct NotificationManager {
    fcm: Option<FcmClient>,
    discord: Option<DiscordClient>,
    telegram: Option<TelegramClient>,
    /// FCM device token (stored separately from server key)
    fcm_device_token: String,
    /// Prepended to Discord messages only. e.g. "[Main Account] "
    /// Empty string means no prefix (single-account or unknown account).
    account_prefix: String,
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
            account_prefix: String::new(),
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

    /// Set the account label used to prefix Discord notifications.
    /// Call this after `from_db` when the session's `account_label` is known.
    /// `None` → no prefix (single account or label deleted).
    pub fn with_account_label(mut self, label: Option<&str>) -> Self {
        self.account_prefix = match label {
            Some(l) if !l.is_empty() => format!("[{l}] "),
            _ => String::new(),
        };
        self
    }

    /// Override the Discord webhook with a per-account URL. When `Some(url)`
    /// is given (and non-empty), the global Discord webhook loaded from
    /// `notification_configs` is replaced — letting each Upbit account post
    /// to its own channel. `None` or empty → keep the global webhook
    /// (fallback). Telegram/FCM are unaffected: only Discord routes split.
    pub fn with_account_discord_webhook(mut self, url: Option<&str>) -> Self {
        if let Some(u) = url.map(str::trim).filter(|s| !s.is_empty()) {
            self.discord = Some(DiscordClient::new(u.to_string()));
        }
        self
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
            let discord_msg = if self.account_prefix.is_empty() {
                message.to_string()
            } else {
                format!("{}{}", self.account_prefix, message)
            };
            let _ = discord.send(&discord_msg).await;
        }
        if let Some(telegram) = &self.telegram {
            let _ = telegram.send(message).await;
        }
    }

    /// Send a Discord embed if Discord is configured + plain text fallback to
    /// Telegram/FCM. Lets us use Discord's structured format (color/title/
    /// fields) without giving up the simpler channels — each gets the
    /// representation that fits its medium.
    async fn send_split(&self, mut embed: serde_json::Value, plain_text: &str) {
        if let Some(fcm) = &self.fcm {
            let _ = fcm
                .send(&self.fcm_device_token, "BTC Trader", plain_text, "high")
                .await;
        }
        if let Some(discord) = &self.discord {
            // Prepend account prefix to embed title for Discord only.
            if !self.account_prefix.is_empty() {
                if let Some(title) = embed.get("title").and_then(|v| v.as_str()) {
                    let prefixed = format!("{}{}", self.account_prefix, title);
                    embed["title"] = serde_json::Value::String(prefixed);
                }
            }
            let payload = json!({ "embeds": [embed] });
            let _ = discord.send_payload(&payload).await;
        }
        if let Some(telegram) = &self.telegram {
            let _ = telegram.send(plain_text).await;
        }
    }

    /// Rich trade notification — Discord embed mirroring the legacy
    /// `SendTradeWithBalanceNotificationAsync` layout. fields:
    ///   - 코인 / 매수가|매도가 / 수량 / (sell only) 손익 — 모두 inline
    ///   - 시간 (KST) — block
    ///   - (있으면) 보유 KRW / 보유 코인 / 총 평가 — inline
    ///   - (있으면) 세션 라벨 / 세션 시작 / 세션 수익 — inline
    ///   - (있으면) note (사유) — block
    ///
    /// `late=true` 면 title 앞에 ⏱ 추가 + 색을 어둡게 (cycle 동기 fill 과
    /// 시각 구분).
    pub async fn notify_trade_embed(
        &self,
        side: &str,
        market: &str,
        price: f64,
        volume: f64,
        pnl_pct: Option<f64>,
        ctx: &TradeContext<'_>,
        late: bool,
    ) {
        let is_buy = side == "buy";
        // Legacy colors: 0x00FF00 (green) / 0xFF0000 (red). Late cycle is
        // dimmed ~60% to differentiate from synchronous fills.
        let color: u32 = if late {
            if is_buy { 0x009933 } else { 0x993333 }
        } else if is_buy { 0x00FF00 } else { 0xFF0000 };
        let title_emoji = match (is_buy, late) {
            (true, false) => "📈",
            (true, true) => "⏱📈",
            (false, false) => "📉",
            (false, true) => "⏱📉",
        };
        let title = format!("{} {} 체결", title_emoji, if is_buy { "매수" } else { "매도" });
        let kst = (chrono::Utc::now() + chrono::Duration::hours(9))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // 다중 사용자 시청 환경 — 디스코드 embed는 절대값(가격/수량/잔고) 대신
        // 총평가 대비 체결금액 비율만 노출. Plain text(텔레그램/FCM)는 그대로.
        let trade_pct = ctx.total_value_krw
            .filter(|t| *t > 0.0)
            .map(|t| (price * volume) / t * 100.0);
        let trade_pct_label = if is_buy { "매수 비율" } else { "매도 비율" };

        let mut fields: Vec<serde_json::Value> = vec![
            json!({"name": "코인", "value": format!("`{}`", market), "inline": true}),
        ];
        if let Some(p) = trade_pct {
            fields.push(json!({
                "name": trade_pct_label,
                "value": format!("`{:.2}%`", p),
                "inline": true,
            }));
        }
        if !is_buy {
            if let Some(p) = pnl_pct {
                let sign = if p >= 0.0 { "+" } else { "" };
                fields.push(json!({
                    "name": "손익",
                    "value": format!("`{}{:.2}%`", sign, p),
                    "inline": true,
                }));
            }
        }
        fields.push(json!({
            "name": "시간 (KST)",
            "value": format!("`{}`", kst),
            "inline": false,
        }));

        // Session block — 라벨/수익 %만 노출 (시작 자산 절대값은 숨김)
        if let Some(label) = ctx.session_label {
            fields.push(json!({"name": "📊 세션", "value": format!("`{}`", label), "inline": true}));
        }
        if let Some(p) = ctx.session_pnl_pct {
            let sign = if p >= 0.0 { "+" } else { "" };
            fields.push(json!({
                "name": "📊 세션 수익",
                "value": format!("`{}{:.2}%`", sign, p),
                "inline": true,
            }));
        }
        if let Some(n) = ctx.note {
            if !n.is_empty() {
                fields.push(json!({"name": "메모", "value": n, "inline": false}));
            }
        }

        let embed = json!({
            "title": title,
            "color": color,
            "fields": fields,
        });

        // Plain text fallback for Telegram/FCM.
        let plain = build_plain_trade(side, market, price, volume, pnl_pct, ctx, late, &kst);
        self.send_split(embed, &plain).await;
    }

    /// Rich ready-signal notification — buy=주황(0xFFA500), sell=토마토(0xFF6347)
    /// per legacy color scheme. Embed includes coin / current price / time.
    pub async fn notify_ready_embed(
        &self,
        market: &str,
        side: &str,
        target_price: f64,
        session_label: Option<&str>,
    ) {
        let is_buy = side == "buy";
        let title = if is_buy { "⚠️ 매수 대기 (BUY READY)" } else { "⚠️ 매도 대기 (SELL READY)" };
        let color: u32 = if is_buy { 0xFFA500 } else { 0xFF6347 };
        let kst = (chrono::Utc::now() + chrono::Duration::hours(9))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let mut fields: Vec<serde_json::Value> = vec![
            json!({"name": "코인", "value": format!("`{}`", market), "inline": true}),
            json!({"name": "현재가", "value": format!("`{}` KRW", fmt_int(target_price)), "inline": true}),
        ];
        if let Some(s) = session_label {
            fields.push(json!({"name": "세션", "value": format!("`{}`", s), "inline": true}));
        }
        fields.push(json!({
            "name": "시간 (KST)",
            "value": format!("`{}`", kst),
            "inline": false,
        }));

        let embed = json!({
            "title": title,
            "color": color,
            "fields": fields,
        });

        let plain = format!(
            "{} {} ({} 약 {} KRW){}",
            title, market, if is_buy { "매수가" } else { "매도가" },
            fmt_int(target_price),
            session_label.map(|s| format!(", 세션: {}", s)).unwrap_or_default(),
        );
        self.send_split(embed, &plain).await;
    }

    /// Limit 주문이 호가창에 등록되었으나 미체결인 경우. 매수=📥/매도=📤.
    pub async fn notify_order_registered_embed(
        &self,
        market: &str,
        side: &str,
        target_price: f64,
        session_label: Option<&str>,
    ) {
        let is_buy = side == "buy";
        let title = if is_buy { "📥 매수 주문 등록 (LIMIT WAIT)" } else { "📤 매도 주문 등록 (LIMIT WAIT)" };
        let color: u32 = if is_buy { 0x4FC3F7 } else { 0xFF8A65 }; // light blue / soft orange
        let kst = (chrono::Utc::now() + chrono::Duration::hours(9))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let mut fields: Vec<serde_json::Value> = vec![
            json!({"name": "코인", "value": format!("`{}`", market), "inline": true}),
            json!({"name": "지정가", "value": format!("`{}` KRW", fmt_int(target_price)), "inline": true}),
            json!({"name": "상태", "value": "`wait`", "inline": true}),
        ];
        if let Some(s) = session_label {
            fields.push(json!({"name": "세션", "value": format!("`{}`", s), "inline": true}));
        }
        fields.push(json!({
            "name": "시간 (KST)",
            "value": format!("`{}`", kst),
            "inline": false,
        }));

        let embed = json!({
            "title": title,
            "color": color,
            "fields": fields,
        });
        let plain = format!(
            "{} {} 지정가 {} KRW (wait){}",
            title, market, fmt_int(target_price),
            session_label.map(|s| format!(", 세션: {}", s)).unwrap_or_default(),
        );
        self.send_split(embed, &plain).await;
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────

fn fmt_int(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn build_plain_trade(
    side: &str,
    market: &str,
    price: f64,
    volume: f64,
    pnl_pct: Option<f64>,
    ctx: &TradeContext<'_>,
    late: bool,
    kst: &str,
) -> String {
    let icon = match (side == "buy", late) {
        (true, false) => "🟢",
        (true, true) => "⏱🟢",
        (false, false) => "🔴",
        (false, true) => "⏱🔴",
    };
    let head = if side == "buy" {
        format!("{} {} 매수: {} KRW × {:.6}", icon, market, fmt_int(price), volume)
    } else {
        format!(
            "{} {} 매도: {} KRW × {:.6} (P/L: {:+.2}%)",
            icon, market, fmt_int(price), volume,
            pnl_pct.unwrap_or(0.0),
        )
    };
    let mut lines = vec![head, format!("⏰ {} KST", kst)];
    if let Some(s) = ctx.session_label {
        lines.push(format!("📊 세션: {}", s));
    }
    if let Some(total) = ctx.total_value_krw {
        lines.push(format!("💰 총 평가: {} KRW", fmt_int(total)));
    }
    if let Some(p) = ctx.session_pnl_pct {
        lines.push(format!("📊 세션 수익: {:+.2}%", p));
    }
    lines.join("\n")
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
