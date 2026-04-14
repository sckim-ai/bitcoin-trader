use crate::models::market::Candle;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;

/// Import a CSV file into the market_data table.
/// CSV must have headers: timestamp, open, high, low, close, volume
/// Returns the number of rows inserted (duplicates are skipped via OR IGNORE).
pub fn import_csv(
    conn: &Connection,
    csv_path: &Path,
    market: &str,
    timeframe: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut reader = csv::Reader::from_path(csv_path)?;
    let mut count = 0;

    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO market_data (market, timeframe, timestamp, open, high, low, close, volume)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;

        for result in reader.records() {
            let record = result?;
            // Expect columns: timestamp, open, high, low, close, volume
            if record.len() < 6 {
                continue;
            }
            let timestamp = &record[0];
            let open: f64 = record[1].parse()?;
            let high: f64 = record[2].parse()?;
            let low: f64 = record[3].parse()?;
            let close: f64 = record[4].parse()?;
            let volume: f64 = record[5].parse()?;

            let inserted = stmt.execute(params![market, timeframe, timestamp, open, high, low, close, volume])?;
            count += inserted;
        }
    }
    tx.commit()?;

    Ok(count)
}

/// Load candles from the market_data table for a given market and timeframe,
/// ordered by timestamp ascending.
pub fn load_candles(
    conn: &Connection,
    market: &str,
    timeframe: &str,
) -> Result<Vec<Candle>, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT timestamp, open, high, low, close, volume
         FROM market_data
         WHERE market = ?1 AND timeframe = ?2
         ORDER BY timestamp ASC",
    )?;

    let candles = stmt
        .query_map(params![market, timeframe], |row| {
            let ts_str: String = row.get(0)?;
            let open: f64 = row.get(1)?;
            let high: f64 = row.get(2)?;
            let low: f64 = row.get(3)?;
            let close: f64 = row.get(4)?;
            let volume: f64 = row.get(5)?;
            Ok((ts_str, open, high, low, close, volume))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut result = Vec::with_capacity(candles.len());
    for (ts_str, open, high, low, close, volume) in candles {
        let timestamp: DateTime<Utc> = ts_str
            .parse()
            .unwrap_or_else(|_| DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap().into());
        result.push(Candle {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use std::io::Write;

    fn setup_db() -> (Connection, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = schema::initialize(&db_path).unwrap();
        (conn, dir)
    }

    #[test]
    fn test_import_and_load() {
        let (conn, dir) = setup_db();

        // Create test CSV
        let csv_path = dir.path().join("test.csv");
        {
            let mut f = std::fs::File::create(&csv_path).unwrap();
            writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
            writeln!(f, "2024-01-01T00:00:00Z,100.0,105.0,95.0,102.0,1000.0").unwrap();
            writeln!(f, "2024-01-01T01:00:00Z,102.0,108.0,99.0,106.0,1500.0").unwrap();
        }

        let count = import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
        assert_eq!(count, 2);

        let candles = load_candles(&conn, "BTC", "hour").unwrap();
        assert_eq!(candles.len(), 2);
        assert!((candles[0].close - 102.0).abs() < 1e-10);
        assert!((candles[1].volume - 1500.0).abs() < 1e-10);
    }

    #[test]
    fn test_import_duplicate_skipped() {
        let (conn, dir) = setup_db();

        let csv_path = dir.path().join("test.csv");
        {
            let mut f = std::fs::File::create(&csv_path).unwrap();
            writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
            writeln!(f, "2024-01-01T00:00:00Z,100.0,105.0,95.0,102.0,1000.0").unwrap();
        }

        let count1 = import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
        assert_eq!(count1, 1);

        // Import again — duplicate should be skipped
        let count2 = import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
        assert_eq!(count2, 0);

        let candles = load_candles(&conn, "BTC", "hour").unwrap();
        assert_eq!(candles.len(), 1);
    }
}
