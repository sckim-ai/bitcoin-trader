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
    /// Baseline metrics from the simulation that produced this preset.
    /// Optional so older callers (without simulation context) still work.
    pub baseline_return: Option<f64>,
    pub baseline_trades: Option<i32>,
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
        args.baseline_return,
        args.baseline_trades,
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
    /// Optional per-session BUY cap in KRW. None / omitted → use full balance.
    #[serde(default)]
    pub max_order_krw: Option<f64>,
    /// paper 세션 Discord 알림 토글. real 세션은 항상 알림이 가므로 무관.
    /// 기본값 false — paper N개 노이즈 방지.
    #[serde(default)]
    pub notify_discord: bool,
}

/// Normalize a preset's since_ts ("YYYY-MM-DD" or RFC3339) to an RFC3339
/// UTC timestamp. Returns `None` if the input can't be parsed.
fn normalize_since(raw: &str) -> Option<String> {
    // Already RFC3339?
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc).to_rfc3339());
    }
    // YYYY-MM-DD?
    if let Ok(date) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let ts = date.and_hms_opt(0, 0, 0)?.and_utc();
        return Some(ts.to_rfc3339());
    }
    None
}

#[tauri::command]
pub fn create_session(
    args: CreateSessionArgs,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    // 세션 생성은 계정과 무관하게 자유롭게. 항상 paper로 시작.
    // 계정 연결은 paper → real 승급(toggle_session_mode) 시점에만 강제된다.
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let preset = live_repo::get_preset(&conn, args.preset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("preset {} not found", args.preset_id))?;

    // Session's simulation window always anchors on the preset's backtest
    // start. Falls back to "now" if the preset has no since_ts recorded.
    let start_ts = preset
        .since_ts
        .as_deref()
        .and_then(normalize_since)
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    let id = live_repo::insert_session(
        &conn,
        1,
        &args.label,
        args.preset_id,
        &args.market,
        "paper",
        args.initial_capital,
        &start_ts,
        None,
    )
    .map_err(|e| e.to_string())?;

    if let Some(cap) = args.max_order_krw {
        if cap > 0.0 {
            live_repo::set_session_max_order_krw(&conn, id, Some(cap))
                .map_err(|e| e.to_string())?;
        }
    }
    if args.notify_discord {
        live_repo::set_session_notify_discord(&conn, id, true)
            .map_err(|e| e.to_string())?;
    }
    Ok(id)
}

#[tauri::command]
pub fn set_session_order_cap(
    id: i64,
    max_order_krw: Option<f64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // Treat 0 / negative as "clear cap". Frontend passes None to clear too.
    let normalized = max_order_krw.filter(|&v| v > 0.0);
    live_repo::set_session_max_order_krw(&conn, id, normalized)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// paper 세션의 Discord 알림 토글. real 세션은 항상 알림이 가므로 호출이 무해(no-op 같은 효과).
/// 016 도입 boolean 토글 — 새 UI는 `set_session_notify_account_ids` 사용.
#[tauri::command]
pub fn set_session_notify_discord(
    id: i64,
    value: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::set_session_notify_discord(&conn, id, value).map_err(|e| e.to_string())?;
    Ok(())
}

/// Paper 세션의 알림 fan-out 계정 목록을 갱신. 빈 array면 알림 off.
/// Real 세션에 대해 호출돼도 DB는 갱신되지만, 알림 정책상 효과 없음.
#[tauri::command]
pub fn set_session_notify_account_ids(
    id: i64,
    account_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::set_session_notify_account_ids(&conn, id, &account_ids)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> Result<Vec<LiveSession>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::list_sessions(&conn, 1).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_session(
    id: i64,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;

    // Flip status + load session/preset for an immediate first cycle.
    let (session, preset) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        live_repo::set_session_status(&conn, id, "running").map_err(|e| e.to_string())?;
        let s = live_repo::get_session(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("session {id} not found"))?;
        let p = live_repo::get_preset(&conn, s.preset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("preset {} not found", s.preset_id))?;
        (s, p)
    };
    state.paper_session_ids.lock().map_err(|e| e.to_string())?.insert(id, ());

    // Kick off one cycle now so the user sees current position/signal
    // without waiting for the next hourly boundary. The scheduler continues
    // running on the hour for subsequent updates.
    let handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        // Dedicated DB connection — avoids contending with the main mutex
        // while we call Upbit + run simulation.
        let conn = match crate::db::schema::initialize(&crate::db::paths::local_db_path()) {
            Ok(c) => std::sync::Arc::new(std::sync::Mutex::new(c)),
            Err(e) => {
                let _ = handle.emit("session:log", serde_json::json!({
                    "session_id": id,
                    "level": "ERROR",
                    "message": format!("immediate-cycle DB init: {e}"),
                }));
                return;
            }
        };
        let registry = crate::strategies::StrategyRegistry::new();

        match crate::services::session_engine::run_session_cycle(
            &conn, &session, &preset, &registry,
        ).await {
            Ok(out) => { let _ = handle.emit("session:update", &out); }
            Err(e) => {
                let _ = handle.emit("session:log", serde_json::json!({
                    "session_id": id,
                    "level": "ERROR",
                    "message": format!("immediate cycle: {e}"),
                }));
            }
        }
    });

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

// ─── Mode toggle (paper ↔ real) — Phase 4A.2 ───
//
// API key check on promotion: a real session is useless without keys, and
// silent failure on first cycle is the worst UX. We surface the missing-key
// case at the toggle moment with a clear message pointing to Settings.
// Note: multi-real=1 invariant is enforced by the per-account partial unique
// index (Task 10). count_real_sessions has been removed.

#[derive(serde::Deserialize)]
pub struct ToggleSessionModeArgs {
    pub id: i64,
    pub mode: String, // "paper" or "real"
    /// real 승급 시 계정 지정. None이면 세션에 이미 연결된 계정을 사용한다.
    /// paper로 되돌릴 때는 무시 (기존 계정 연결을 보존).
    #[serde(default)]
    pub upbit_account_id: Option<i64>,
}

#[tauri::command]
pub fn toggle_session_mode(
    args: ToggleSessionModeArgs,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mode = args.mode.as_str();
    if mode != "paper" && mode != "real" {
        return Err(format!("invalid mode: {mode} (expected 'paper' or 'real')"));
    }

    let conn = state.db.lock().map_err(|e| e.to_string())?;

    if mode == "real" {
        // real 승급 경로: 계정을 새로 받았거나 세션에 이미 연결된 계정을 사용한다.
        let session = live_repo::get_session(&conn, args.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("session {} not found", args.id))?;
        let account_id = args
            .upbit_account_id
            .or(session.upbit_account_id)
            .ok_or_else(|| "Upbit 계정을 선택하세요.".to_string())?;
        let acc = crate::db::upbit_accounts_repo::get_account(&conn, account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "계정이 삭제되었거나 존재하지 않습니다.".to_string())?;
        if !acc.enabled {
            return Err("계정이 비활성화 상태입니다.".into());
        }
        let (ha, hs) = crate::commands::upbit_keys::has_keys_for(account_id);
        if !ha || !hs {
            return Err("계정 API 키가 설정되지 않았습니다.".into());
        }
        // mode + upbit_account_id를 한 UPDATE로 원자적으로 적용.
        // 부분 유니크 인덱스(idx_session_account_running_real)가 같은 계정의
        // 두 번째 running real을 차단하므로 SQLite UNIQUE 위반을
        // 친절한 한글 메시지로 변환한다.
        live_repo::set_session_mode_real(&conn, args.id, account_id)
            .map_err(|e| {
                let s = e.to_string().to_lowercase();
                if s.contains("unique") || s.contains("constraint") {
                    format!("계정 [{}]에 이미 실행 중인 real 세션이 있습니다.", acc.label)
                } else {
                    e.to_string()
                }
            })?;
    } else {
        // paper로 되돌리기: 계정 연결은 보존, mode만 변경.
        live_repo::set_session_mode(&conn, args.id, mode).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// All wait-state pending orders across sessions. Used by the LiveTrading
/// page's "pending" widget — normally 0 rows during steady operation, but
/// useful for spotting stuck orders during outages or re-peg failures.
#[derive(serde::Serialize)]
pub struct PendingOrderRow {
    pub uuid: String,
    pub session_id: i64,
    pub side: String,
    pub market: String,
    pub ord_type: String,
    pub target_price: Option<f64>,
    pub requested: f64,
    pub placed_at: String,
    pub last_checked: Option<String>,
}

#[tauri::command]
pub fn list_pending_orders(state: State<'_, AppState>) -> Result<Vec<PendingOrderRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let rows = live_repo::list_pending_wait(&conn).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|p| PendingOrderRow {
        uuid: p.uuid,
        session_id: p.session_id,
        side: p.side,
        market: p.market,
        ord_type: p.ord_type,
        target_price: p.target_price,
        requested: p.requested,
        placed_at: p.placed_at,
        last_checked: p.last_checked,
    }).collect())
}

/// Stop every running real session immediately. Returns the affected ids.
/// Mode is preserved (still 'real') — the user explicitly chose those, and
/// silent demote on emergency would be surprising. To revert mode, use
/// `toggle_session_mode` afterward.
#[tauri::command]
pub fn emergency_stop_all_real(state: State<'_, AppState>) -> Result<Vec<i64>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let ids = live_repo::stop_all_real_sessions(&conn).map_err(|e| e.to_string())?;
    drop(conn);
    let mut session_set = state.paper_session_ids.lock().map_err(|e| e.to_string())?;
    for id in &ids {
        session_set.remove(id);
    }
    Ok(ids)
}

#[tauri::command]
pub fn list_session_trades(session_id: i64, state: State<'_, AppState>) -> Result<Vec<LiveTrade>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    live_repo::list_trades(&conn, session_id).map_err(|e| e.to_string())
}

/// Returns the per-candle signal_log persisted by the most recent
/// `session_engine` cycle. Empty array if the session has not cycled yet.
#[tauri::command]
pub fn get_session_signal_log(
    session_id: i64,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let json = live_repo::get_session_signal_log(&conn, session_id).map_err(|e| e.to_string())?;
    match json {
        Some(s) if !s.is_empty() => serde_json::from_str(&s).map_err(|e| e.to_string()),
        _ => Ok(serde_json::Value::Array(vec![])),
    }
}

