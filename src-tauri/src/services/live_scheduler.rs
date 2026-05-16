use crate::api::upbit::UpbitClient;
use crate::db::live_repo;
use crate::services::session_engine;
use crate::strategies::StrategyRegistry;
use chrono::{Timelike, Utc};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn seconds_until_next_hour() -> u64 {
    let now = Utc::now();
    let secs_into_hour = now.minute() * 60 + now.second();
    let remaining = 3600u64.saturating_sub(secs_into_hour as u64);
    if remaining < 10 { remaining + 3600 } else { remaining }
}

pub fn create_public_client() -> UpbitClient {
    // Public endpoints (ticker price, candles) work without credentials.
    // Authenticated callers use upbit_client_for(account_id) instead.
    UpbitClient::new(String::new(), String::new())
}

/// Run the live scheduler forever. Wakes at every hour boundary, iterates all
/// `running` sessions, and executes one cycle per session.
#[cfg(feature = "tauri-app")]
pub async fn run_loop(
    app_handle: tauri::AppHandle,
    db: Arc<Mutex<Connection>>,
    cancel: Arc<AtomicBool>,
) {
    use tauri::Emitter;

    let registry = StrategyRegistry::new();
    // run_session_cycle now reads candles directly from DB; no Upbit client
    // needed here. (`create_public_client` is still pub for other callers.)

    loop {
        if cancel.load(Ordering::Relaxed) { break; }
        let wait = seconds_until_next_hour();
        for _ in 0..wait {
            if cancel.load(Ordering::Relaxed) { return; }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // 1. Snapshot running sessions.
        let sessions = {
            let conn = match db.lock() {
                Ok(c) => c,
                Err(e) => { eprintln!("live-scheduler db lock: {e}"); continue; }
            };
            match live_repo::list_running_sessions(&conn) {
                Ok(v) => v,
                Err(e) => { eprintln!("list_running_sessions: {e}"); continue; }
            }
        };

        for session in sessions {
            let preset = {
                let conn = db.lock().unwrap();
                match live_repo::get_preset(&conn, session.preset_id) {
                    Ok(Some(p)) => p,
                    _ => {
                        eprintln!("preset {} not found for session {}", session.preset_id, session.id);
                        continue;
                    }
                }
            };

            match session_engine::run_session_cycle(&db, &session, &preset, &registry).await {
                Ok(out) => {
                    let _ = app_handle.emit("session:update", &out);
                }
                Err(e) => {
                    eprintln!("session {} cycle error: {e}", session.id);
                    let _ = app_handle.emit("session:log", serde_json::json!({
                        "session_id": session.id,
                        "level": "ERROR",
                        "message": format!("cycle error: {e}"),
                    }));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seconds_until_next_hour_range() {
        let s = seconds_until_next_hour();
        assert!(s >= 10);
        assert!(s <= 7200);
    }
}
