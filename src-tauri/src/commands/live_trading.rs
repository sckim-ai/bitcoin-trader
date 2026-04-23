use crate::db::live_repo;
use crate::models::live::{LiveSession, LiveTrade, Preset};
use crate::models::trading::TradingParameters;
use crate::state::AppState;
use chrono::Utc;
use serde::Deserialize;
use tauri::State;

// ─── Presets ───

#[derive(Deserialize)]
pub struct SavePresetArgs {
    pub name: String,
    pub strategy_key: String,
    /// Market short form ("ETH" or "BTC"), used for default merging + record.
    pub market: String,
    pub timeframe: Option<String>,
    pub since_ts: Option<String>,
    pub until_ts: Option<String>,
    /// Partial parameter overrides (flat map of name → number). Missing fields
    /// use `TradingParameters::default_for_market` values.
    pub partial_params: serde_json::Value,
    pub source: Option<String>,
    pub source_run_id: Option<i64>,
}

#[tauri::command]
pub fn save_preset(
    args: SavePresetArgs,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    // 1. Start from market defaults.
    let base = TradingParameters::default_for_market(&args.market);
    let mut merged: serde_json::Value = serde_json::to_value(&base).map_err(|e| e.to_string())?;

    // 2. Overlay partial params. Unknown keys are silently ignored — the
    //    caller's flat `params` map may include entries that don't map to a
    //    TradingParameters field (e.g., UI-only controls). Serde roundtrip
    //    validates the final shape.
    if let (Some(obj), Some(overrides)) = (merged.as_object_mut(), args.partial_params.as_object()) {
        for (k, v) in overrides {
            if obj.contains_key(k) {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    // 3. Validate by round-tripping back to TradingParameters.
    let _validated: TradingParameters = serde_json::from_value(merged.clone())
        .map_err(|e| format!("parameter validation failed: {e}"))?;
    let params_json = serde_json::to_string(&merged).map_err(|e| e.to_string())?;

    // 4. Persist.
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::insert_preset(
        &conn,
        1,
        &args.name,
        &args.strategy_key,
        &params_json,
        args.source.as_deref().unwrap_or("manual"),
        args.source_run_id,
        Some(&args.market),
        args.timeframe.as_deref(),
        args.since_ts.as_deref(),
        args.until_ts.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_presets(state: State<'_, AppState>) -> Result<Vec<Preset>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::list_presets(&conn, 1).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_preset(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::delete_preset(&conn, id).map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Sessions ───

#[derive(Deserialize)]
pub struct CreateSessionArgs {
    pub label: String,
    pub preset_id: i64,
    pub market: String,
    pub initial_capital: f64,
    pub start_offset_days: Option<u32>,
}

#[tauri::command]
pub fn create_session(
    args: CreateSessionArgs,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let _preset = live_repo::get_preset(&conn, args.preset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("preset {} not found", args.preset_id))?;

    let start_ts = match args.start_offset_days {
        Some(days) => (Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339(),
        None => Utc::now().to_rfc3339(),
    };

    live_repo::insert_session(
        &conn,
        1,
        &args.label,
        args.preset_id,
        &args.market,
        "paper",
        args.initial_capital,
        &start_ts,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> Result<Vec<LiveSession>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::list_sessions(&conn, 1).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_session(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::set_session_status(&conn, id, "running").map_err(|e| e.to_string())?;
    drop(conn);
    state.paper_session_ids.lock().map_err(|e| e.to_string())?.insert(id, ());
    Ok(())
}

#[tauri::command]
pub fn stop_session(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::set_session_status(&conn, id, "stopped").map_err(|e| e.to_string())?;
    drop(conn);
    state.paper_session_ids.lock().map_err(|e| e.to_string())?.remove(&id);
    Ok(())
}

#[tauri::command]
pub fn delete_session(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::delete_session(&conn, id).map_err(|e| e.to_string())?;
    drop(conn);
    state.paper_session_ids.lock().map_err(|e| e.to_string())?.remove(&id);
    Ok(())
}

#[tauri::command]
pub fn list_session_trades(session_id: i64, state: State<'_, AppState>) -> Result<Vec<LiveTrade>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::list_trades(&conn, session_id).map_err(|e| e.to_string())
}
