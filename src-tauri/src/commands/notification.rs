use crate::auth::session;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn save_notification_config(
    token: String,
    channel: String,
    config: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let user_id = session::validate_session(&conn, &token)?
        .ok_or("Not authenticated")?;

    conn.execute(
        "INSERT INTO notification_configs (user_id, channel, config, enabled)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id, channel) DO UPDATE SET config = ?3, enabled = ?4",
        rusqlite::params![user_id, channel, config, enabled as i64],
    )
    .map_err(|e| format!("Failed to save notification config: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn test_notification(
    token: String,
    channel: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mgr = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let user_id = session::validate_session(&conn, &token)?
            .ok_or("Not authenticated")?;
        crate::notifications::manager::NotificationManager::from_db(&conn, user_id)
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
