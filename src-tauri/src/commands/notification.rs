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
