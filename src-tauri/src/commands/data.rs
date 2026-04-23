use crate::api::upbit::UpbitClient;
use crate::core::indicators;
use crate::migration::csv_import;
use crate::models::market::{Candle, MarketData};
use crate::state::AppState;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

const SINCE: &str = "2020-01-01T00:00:00";

#[tauri::command]
pub fn load_csv_data(
    csv_path: String,
    market: String,
    timeframe: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let path = std::path::Path::new(&csv_path);
    csv_import::import_csv(&conn, path, &market, &timeframe).map_err(|e| e.to_string())
}

/// One-shot backfill: populate `day_psy` for every hour row in the DB
/// whose value is currently NULL. Run once after upgrading to schema v3.
#[tauri::command]
pub fn backfill_day_psy(
    state: State<'_, AppState>,
) -> Result<Vec<(String, usize)>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let markets: Vec<String> = conn
        .prepare("SELECT DISTINCT market FROM market_data WHERE timeframe = 'hour'")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for m in markets {
        let n = crate::core::day_psy_store::refresh_day_psy(&conn, &m)
            .map_err(|e| e.to_string())?;
        out.push((m, n));
    }
    Ok(out)
}

#[tauri::command]
pub fn get_candles(
    market: String,
    timeframe: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<Candle>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    csv_import::load_candles(&conn, &market, &timeframe, limit).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct DataRange {
    pub market: String,
    pub timeframe: String,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_timestamp: Option<String>,
}

#[tauri::command]
pub fn get_data_range(
    market: String,
    timeframe: String,
    state: State<'_, AppState>,
) -> Result<DataRange, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let row: (i64, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT COUNT(*), MIN(timestamp), MAX(timestamp)
             FROM market_data
             WHERE market = ?1 AND timeframe = ?2",
            rusqlite::params![market, timeframe],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok(DataRange {
        market,
        timeframe,
        count: row.0 as usize,
        min_timestamp: row.1,
        max_timestamp: row.2,
    })
}

#[tauri::command]
pub fn get_market_data(
    market: String,
    timeframe: String,
    state: State<'_, AppState>,
) -> Result<Vec<MarketData>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if timeframe == "hour" {
        return crate::core::day_psy_store::load_market_data(&conn, &market, None, None)
            .map_err(|e| e.to_string());
    }
    let candles =
        csv_import::load_candles(&conn, &market, &timeframe, None).map_err(|e| e.to_string())?;
    let indicator_sets = indicators::calculate_all(&candles);
    Ok(candles
        .into_iter()
        .zip(indicator_sets)
        .map(|(candle, ind)| MarketData {
            candle,
            indicators: ind,
        })
        .collect())
}

// ─── Data auto-update commands ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    pub market: String,
    pub timeframe: String,
    pub new_candles: usize,
}

/// Convert Upbit market name (KRW-BTC) to DB market name (BTC).
/// DB uses short names (BTC, ETH) to match CSV import convention.
fn to_db_market(api_market: &str) -> String {
    api_market
        .strip_prefix("KRW-")
        .unwrap_or(api_market)
        .to_string()
}

/// Convert DB market name (BTC) to Upbit API market name (KRW-BTC).
fn to_api_market(db_market: &str) -> String {
    if db_market.starts_with("KRW-") {
        db_market.to_string()
    } else {
        format!("KRW-{}", db_market)
    }
}

/// Fetch candles from Upbit, paginating backwards from `start_to` (None = latest)
/// until reaching `since` date or running out of data.
async fn fetch_candles_back(
    client: &UpbitClient,
    api_market: &str,
    interval: &str,
    since: &str,
    start_to: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut all = Vec::new();
    let mut to_cursor = start_to;

    loop {
        let raw = client
            .get_candles_before(api_market, interval, 200, to_cursor.as_deref())
            .await
            .map_err(|e| e.to_string())?;

        if raw.is_empty() {
            break;
        }

        let oldest_ts = raw
            .last()
            .and_then(|v| v.get("candle_date_time_utc"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        all.extend(raw.iter().cloned());

        if oldest_ts.as_str() <= since {
            break;
        }
        if raw.len() < 200 {
            break;
        }
        // Avoid infinite loop if Upbit returns same cursor batch
        if Some(&oldest_ts) == to_cursor.as_ref() {
            break;
        }

        to_cursor = Some(oldest_ts);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    Ok(all)
}

/// Sync one (market, timeframe) pair: fetch latest 200 + back-fill any historical gap.
/// Returns total inserted candle count.
pub async fn sync_market(
    db: &Mutex<Connection>,
    db_market: &str,
    timeframe: &str,
    api_market: &str,
    interval: &str,
) -> Result<usize, String> {
    let client = create_client()?;

    // Read current DB state (release lock before any await)
    let (count, oldest): (i64, Option<String>) = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT COUNT(*), MIN(timestamp) FROM market_data WHERE market = ?1 AND timeframe = ?2",
            rusqlite::params![db_market, timeframe],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?
    };

    let mut total = 0;

    // 1. Forward sync: latest 200 candles (covers gap from latest_in_db to now)
    let latest = client
        .get_candles(api_market, interval, 200)
        .await
        .map_err(|e| e.to_string())?;
    {
        let conn = db.lock().map_err(|e| e.to_string())?;
        total += insert_candles_to_db(&conn, db_market, timeframe, &latest).map_err(|e| e.to_string())?;
    }

    // 2. Back-fill: if DB empty OR oldest still later than SINCE, paginate backwards.
    // Strip trailing 'Z' for lexical compare since Upbit returns "...UTC" without Z.
    let needs_backfill = match &oldest {
        None => true,
        Some(ts) => ts.trim_end_matches('Z') > SINCE,
    };

    if needs_backfill {
        // Start cursor: oldest in DB (so we fetch strictly before it). If DB empty, start from now.
        let start_to = oldest.as_ref().map(|t| t.trim_end_matches('Z').to_string());
        let backfill = fetch_candles_back(&client, api_market, interval, SINCE, start_to).await?;
        if !backfill.is_empty() {
            let conn = db.lock().map_err(|e| e.to_string())?;
            total += insert_candles_to_db(&conn, db_market, timeframe, &backfill).map_err(|e| e.to_string())?;
        }
    }

    // After any ingest, try to populate day_psy on hour rows where it's still NULL.
    // Safe regardless of which timeframe was just synced: if day candles aren't present yet,
    // the function is a no-op for dates with missing day data.
    {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let _ = crate::core::day_psy_store::refresh_day_psy(&conn, db_market)
            .map_err(|e| e.to_string())?;
    }

    let _ = count; // count unused now, kept for potential logging
    Ok(total)
}

fn timeframe_to_interval(timeframe: &str) -> &'static str {
    match timeframe {
        "hour" => "60",
        "day" => "day",
        "week" => "week",
        _ => "60",
    }
}

#[tauri::command]
pub async fn update_market_data(
    market: String,
    timeframe: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let api_market = to_api_market(&market);
    let db_market = to_db_market(&market);
    let interval = timeframe_to_interval(&timeframe);
    sync_market(&state.db, &db_market, &timeframe, &api_market, interval).await
}

#[tauri::command]
pub async fn auto_update_all_markets(
    state: State<'_, AppState>,
) -> Result<Vec<UpdateResult>, String> {
    let api_markets = ["KRW-BTC", "KRW-ETH"];
    let timeframes = [("hour", "60"), ("day", "day"), ("week", "week")];

    let mut results = Vec::new();
    for api_market in &api_markets {
        let db_market = to_db_market(api_market);
        for (tf_name, interval) in &timeframes {
            let count = sync_market(&state.db, &db_market, tf_name, api_market, interval)
                .await
                .unwrap_or(0);
            results.push(UpdateResult {
                market: db_market.clone(),
                timeframe: tf_name.to_string(),
                new_candles: count,
            });
        }
    }
    Ok(results)
}

/// Insert Upbit API candle data into market_data table. Returns inserted count.
fn insert_candles_to_db(
    conn: &rusqlite::Connection,
    market: &str,
    timeframe: &str,
    raw: &[serde_json::Value],
) -> Result<usize, Box<dyn std::error::Error>> {
    let tx = conn.unchecked_transaction()?;
    let mut count = 0;
    {
        // UPSERT: in-progress 캔들은 매 fetch마다 high/low/close/volume이 갱신되므로
        // OR IGNORE 대신 ON CONFLICT DO UPDATE 사용. 닫힌 캔들은 값이 동일해서 사실상 no-op.
        let mut stmt = tx.prepare(
            "INSERT INTO market_data (market, timeframe, timestamp, open, high, low, close, volume)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(market, timeframe, timestamp) DO UPDATE SET
                open = excluded.open,
                high = excluded.high,
                low = excluded.low,
                close = excluded.close,
                volume = excluded.volume",
        )?;
        for v in raw {
            let ts = match v.get("candle_date_time_utc").and_then(|t| t.as_str()) {
                Some(t) => format!("{}Z", t.trim_end_matches('Z')),
                None => continue,
            };
            let open = v.get("opening_price").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let high = v.get("high_price").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let low = v.get("low_price").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let close = v.get("trade_price").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let volume = v.get("candle_acc_trade_volume").and_then(|x| x.as_f64()).unwrap_or(0.0);
            if close == 0.0 { continue; }

            count += stmt.execute(rusqlite::params![market, timeframe, ts, open, high, low, close, volume])?;
        }
    }
    tx.commit()?;
    Ok(count)
}

/// Create an UpbitClient for public API calls (candle data).
/// API keys are optional — uses empty strings if not set, since candle endpoints don't require auth.
fn create_client() -> Result<UpbitClient, String> {
    let access_key = std::env::var("UPBIT_ACCESS_KEY").unwrap_or_default();
    let secret_key = std::env::var("UPBIT_SECRET_KEY").unwrap_or_default();
    Ok(UpbitClient::new(access_key, secret_key))
}
