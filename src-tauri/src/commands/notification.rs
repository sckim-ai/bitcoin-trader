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

#[tauri::command]
pub async fn test_notification(
    channel: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mgr = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::notifications::manager::NotificationManager::from_db(&conn, DEFAULT_USER_ID)
    }; // conn dropped here, before any await

    let test_msg = "BTC Trader 테스트 알림입니다.";

    match channel.as_str() {
        "fcm" | "discord" | "telegram" | "all" => {
            mgr.notify_alert(test_msg).await;
            Ok(format!("테스트 알림 전송 완료 ({})", channel))
        }
        _ => Err(format!("Unknown channel: {channel}")),
    }
}
