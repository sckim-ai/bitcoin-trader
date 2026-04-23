//! Persistent day-PSY pipeline.
//!
//! Design (matches the clean-semantic plan, separate from legacy hybrid):
//! * Each hour bar owns a `day_psy` column — computed once at ingest, never
//!   recomputed. Write-once guarantees deterministic back-test results across
//!   rebuilds of the DB from the same Upbit snapshot.
//! * `day_psy(bar at KST date D)` = closed day-PSY(D-1) with `calc_psy`
//!   period=10 applied over the day-candle close series.
//! * Warmup (first 10 day candles) leaves `day_psy = NULL`. Strategies
//!   interpret NULL → NaN at load time and rely on `NaN < threshold == false`
//!   to skip those bars without adding explicit branches.
//!
//! Call `refresh_day_psy(conn, market)` after ingesting new candles for
//! that market (either timeframe). Idempotent: only NULL rows get
//! populated so the function is safe to call repeatedly.

use crate::core::indicators;
use crate::migration::csv_import;
use chrono::Duration;
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

/// Populate `day_psy` on every hour-bar in `market_data` that is currently
/// NULL. Returns the number of rows updated. Missing day-candle history for
/// a given KST date leaves the corresponding hour row untouched — callers
/// can re-run once the day candle arrives.
pub fn refresh_day_psy(
    conn: &Connection,
    market: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    // 1. Build the KST-date → closed day-PSY map from stored day candles.
    let day_candles = csv_import::load_candles(conn, market, "day", None)?;
    if day_candles.is_empty() {
        return Ok(0);
    }
    let psy_map = indicators::build_day_psy_map(&day_candles);

    // 2. For each hour row with NULL day_psy, compute the previous KST date
    //    and look up the day-PSY. Skip warmup (0.0) or missing entries.
    let mut stmt = conn.prepare(
        "SELECT id, timestamp FROM market_data
         WHERE market = ?1 AND timeframe = 'hour' AND day_psy IS NULL",
    )?;
    let rows: Vec<(i64, String)> = stmt
        .query_map(params![market], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let tx = conn.unchecked_transaction()?;
    let mut update_stmt = tx.prepare(
        "UPDATE market_data SET day_psy = ?1 WHERE id = ?2",
    )?;
    let mut updated = 0usize;
    for (id, ts_str) in rows {
        let ts: chrono::DateTime<chrono::Utc> = match ts_str.parse() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let prev_kst_date = (ts + Duration::hours(9)).date_naive() - Duration::days(1);
        if let Some(&psy) = psy_map.get(&prev_kst_date) {
            if psy != 0.0 {
                update_stmt.execute(params![psy, id])?;
                updated += 1;
            }
        }
    }
    drop(update_stmt);
    tx.commit()?;

    Ok(updated)
}

/// Load `(Vec<Candle>, Vec<Option<f64>>)` side-by-side: candles as usual
/// and the persisted `day_psy` column aligned by index. NULL in DB becomes
/// `None`; downstream code converts to `f64::NAN` when building indicators.
pub fn load_hour_with_day_psy(
    conn: &Connection,
    market: &str,
    since: Option<&str>,
    until: Option<&str>,
) -> Result<(Vec<crate::models::market::Candle>, Vec<Option<f64>>), Box<dyn std::error::Error>> {
    let mut where_clauses = String::from("market = ?1 AND timeframe = 'hour'");
    let mut idx = 2usize;
    let since_owned = since.map(str::to_string);
    let until_owned = until.map(str::to_string);
    if since_owned.is_some() {
        where_clauses.push_str(&format!(" AND timestamp >= ?{}", idx));
        idx += 1;
    }
    if until_owned.is_some() {
        where_clauses.push_str(&format!(" AND timestamp <= ?{}", idx));
    }
    let sql = format!(
        "SELECT timestamp, open, high, low, close, volume, day_psy
         FROM market_data WHERE {} ORDER BY timestamp ASC",
        where_clauses
    );
    let mut stmt = conn.prepare(&sql)?;

    let mut bound_args: Vec<&dyn rusqlite::ToSql> = vec![&market];
    if let Some(s) = since_owned.as_ref() { bound_args.push(s); }
    if let Some(u) = until_owned.as_ref() { bound_args.push(u); }

    let mut candles = Vec::new();
    let mut day_psy_values = Vec::new();
    let rows = stmt.query_map(rusqlite::params_from_iter(bound_args), |row| {
        let ts_str: String = row.get(0)?;
        let open: f64 = row.get(1)?;
        let high: f64 = row.get(2)?;
        let low: f64 = row.get(3)?;
        let close: f64 = row.get(4)?;
        let volume: f64 = row.get(5)?;
        let day_psy: Option<f64> = row.get(6)?;
        Ok((ts_str, open, high, low, close, volume, day_psy))
    })?;
    for row in rows {
        let (ts_str, open, high, low, close, volume, day_psy) = row?;
        let timestamp: chrono::DateTime<chrono::Utc> = ts_str
            .parse()
            .unwrap_or_else(|_| chrono::DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap().into());
        candles.push(crate::models::market::Candle {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
        });
        day_psy_values.push(day_psy);
    }
    Ok((candles, day_psy_values))
}

/// Build a `MarketData` vector from DB rows: loads hour candles + persisted
/// day_psy, then computes the rest of the indicator set on the hour series.
/// NULL day_psy → `f64::NAN` so strategy buy-conditions (`psy_day < T`)
/// naturally skip warmup bars.
pub fn load_market_data(
    conn: &Connection,
    market: &str,
    since: Option<&str>,
    until: Option<&str>,
) -> Result<Vec<crate::models::market::MarketData>, Box<dyn std::error::Error>> {
    let (candles, day_psy_values) = load_hour_with_day_psy(conn, market, since, until)?;
    if candles.is_empty() {
        return Ok(Vec::new());
    }
    let psy_map: BTreeMap<chrono::NaiveDate, f64> = candles
        .iter()
        .zip(day_psy_values.iter())
        .filter_map(|(c, v)| {
            v.map(|val| {
                let prev = (c.timestamp + Duration::hours(9)).date_naive() - Duration::days(1);
                (prev, val)
            })
        })
        .collect();
    let indicator_sets = indicators::calculate_all_with_day_psy(&candles, Some(&psy_map));
    Ok(candles
        .into_iter()
        .zip(indicator_sets)
        .map(|(candle, indicators)| crate::models::market::MarketData { candle, indicators })
        .collect())
}
