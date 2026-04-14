use crate::migration::csv_import;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct MigrationResult {
    pub hour_records: usize,
    pub day_records: usize,
    pub week_records: usize,
}

#[tauri::command]
pub fn migrate_from_csv(
    state: State<'_, AppState>,
    csv_dir: String,
) -> Result<MigrationResult, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let dir = Path::new(&csv_dir);

    let hour_records = import_if_exists(&conn, &dir.join("merged_data_hour.csv"), "KRW-BTC", "hour")?;
    let day_records = import_if_exists(&conn, &dir.join("merged_data_day.csv"), "KRW-BTC", "day")?;
    let week_records = import_if_exists(&conn, &dir.join("merged_data_week.csv"), "KRW-BTC", "week")?;

    Ok(MigrationResult {
        hour_records,
        day_records,
        week_records,
    })
}

fn import_if_exists(
    conn: &rusqlite::Connection,
    path: &Path,
    market: &str,
    timeframe: &str,
) -> Result<usize, String> {
    if !path.exists() {
        return Ok(0);
    }
    csv_import::import_csv(conn, path, market, timeframe).map_err(|e| e.to_string())
}
