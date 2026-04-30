//! Notification configuration commands. Aligned with the rest of the desktop
//! app's single-user model — `user_id = 1` (default admin) is used directly
//! instead of validating a session token. The previous token-required path
//! caused a "Not authenticated" error any time the local session expired
//! (24 hours) or when the user simply hadn't gone through the login screen,
//! while every other command (sessions / presets / upbit keys / live trading)
//! happily used user_id=1 hardcoded. Aligning here removes the inconsistency.

use crate::state::AppState;
use tauri::State;

const DEFAULT_USER_ID: i64 = 1;

#[tauri::command]
pub fn save_notification_config(
    channel: String,
    config: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO notification_configs (user_id, channel, config, enabled)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id, channel) DO UPDATE SET config = ?3, enabled = ?4",
        rusqlite::params![DEFAULT_USER_ID, channel, config, enabled as i64],
    )
    .map_err(|e| format!("Failed to save notification config: {e}"))?;

    Ok(())
}

/// Send the 5 trade-notification variants in sequence so the user can verify
/// each format renders correctly in their channel (Discord/Telegram/FCM).
/// Goes through the regular NotificationManager (so enabled=0 channels are
/// skipped — explicit "this is the runtime path" test, not the credential
/// validation that `test_notification` does).
///
/// Variants emitted:
///   1. 매수 대기      (notify_ready buy)
///   2. 매도 대기      (notify_ready sell)
///   3. 매수 즉시 체결 (notify_trade_full buy, late=false)
///   4. 매도 즉시 체결 (notify_trade_full sell, late=false, +1.71% pnl)
///   5. 매수 주문 등록 (notify_order_registered buy, limit wait)
///   6. 매수 늦은 체결 (notify_trade_full buy, late=true)
#[tauri::command]
pub async fn test_trade_notifications(state: State<'_, AppState>) -> Result<String, String> {
    let mgr = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::notifications::manager::NotificationManager::from_db(&conn, DEFAULT_USER_ID)
    };

    let market = "KRW-ETH";
    let session = Some("Long_V3.1_1288% (TEST)");
    let price = 3_400_000.0;
    let qty = 0.00294118;

    // 1) buy ready
    mgr.notify_ready(market, "buy", price, session).await;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // 2) sell ready
    mgr.notify_ready(market, "sell", price + 50_000.0, session).await;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // 3) buy executed (immediate)
    mgr.notify_trade_full(
        "buy", market, price, qty, None,
        Some("TEST · 1 chunks done / 0 wait"),
        false,
    ).await;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // 4) sell executed (immediate, with profit)
    mgr.notify_trade_full(
        "sell", market, price + 58_140.0, qty, Some(1.71),
        Some("TEST · 1 chunks done / 0 wait"),
        false,
    ).await;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // 5) limit-buy registered, not yet filled
    mgr.notify_order_registered(market, "buy", price - 20_000.0, session).await;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // 6) late fill (tracker found a wait order completed across cycles)
    mgr.notify_trade_full(
        "buy", market, price, qty, None,
        Some("TEST · late fill, Long_V3.1 (placed 2026-04-30T10:00:00Z)"),
        true,
    ).await;

    Ok("6개 메시지 전송 완료 — Discord/Telegram/FCM 채널을 확인하세요.".to_string())
}

/// Test a notification channel — independent of `enabled` flag. The intent
/// is "does this URL/token actually work?", which is the question users
/// have right after entering credentials. The legacy implementation went
/// through NotificationManager.from_db which silently skipped channels
/// where enabled=0, leaving "Save → Test" silent failures.
#[tauri::command]
pub async fn test_notification(
    channel: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use crate::notifications::{discord::DiscordClient, fcm::FcmClient, telegram::TelegramClient};

    // Pull the raw config row, ignoring `enabled`.
    let config_json = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let row: Option<String> = conn
            .query_row(
                "SELECT config FROM notification_configs WHERE user_id = ?1 AND channel = ?2",
                rusqlite::params![DEFAULT_USER_ID, channel],
                |r| r.get(0),
            )
            .ok();
        row
    };
    let config_json = config_json.ok_or_else(|| {
        format!("No saved config for channel '{}'. Save it first.", channel)
    })?;
    let config: serde_json::Value = serde_json::from_str(&config_json)
        .map_err(|e| format!("Stored config is malformed: {e}"))?;

    let test_msg = "BTC Trader 테스트 알림입니다.";

    match channel.as_str() {
        "discord" => {
            let url = config["webhook_url"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Discord webhook_url is empty")?;
            DiscordClient::new(url.to_string())
                .send(test_msg)
                .await
                .map_err(|e| format!("Discord send failed: {e}"))?;
            Ok("Discord 전송 완료 — 채널을 확인하세요.".to_string())
        }
        "telegram" => {
            let token = config["bot_token"].as_str().filter(|s| !s.is_empty())
                .ok_or("Telegram bot_token is empty")?;
            let chat_id = config["chat_id"].as_str().filter(|s| !s.is_empty())
                .ok_or("Telegram chat_id is empty")?;
            TelegramClient::new(token.to_string(), chat_id.to_string())
                .send(test_msg)
                .await
                .map_err(|e| format!("Telegram send failed: {e}"))?;
            Ok("Telegram 전송 완료 — 채팅을 확인하세요.".to_string())
        }
        "fcm" => {
            let key = config["server_key"].as_str().filter(|s| !s.is_empty())
                .ok_or("FCM server_key is empty")?;
            let token = config["device_token"].as_str().filter(|s| !s.is_empty())
                .ok_or("FCM device_token is empty")?;
            FcmClient::new(key.to_string())
                .send(token, "BTC Trader", test_msg, "high")
                .await
                .map_err(|e| format!("FCM send failed: {e}"))?;
            Ok("FCM 전송 완료 — 디바이스를 확인하세요.".to_string())
        }
        _ => Err(format!("Unknown channel: {channel}")),
    }
}
