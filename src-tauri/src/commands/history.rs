//! Trading-history queries — backs the /history page.
//!
//! All queries scope to `is_real = 1` so paper noise never bleeds into the
//! historical record. live_trades has no separate index on (is_real, ts) but
//! the table is small enough (~thousands of rows) that a full scan is fine.
//!
//! Three commands:
//!   - list_real_trades:         per-trade rows + filters (session/range)
//!   - real_pnl_summary:         day/week/month aggregations
//!   - export_real_trades_csv:   pure CSV string for the frontend to save

use crate::models::live::LiveTrade;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Deserialize)]
pub struct HistoryFilter {
    /// Optional session filter. None = all sessions.
    pub session_id: Option<i64>,
    /// Inclusive lower bound, RFC3339 (or YYYY-MM-DD prefix). None = no bound.
    pub since: Option<String>,
    /// Exclusive upper bound. None = no bound.
    pub until: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DailyBucket {
    /// "YYYY-MM-DD" in UTC.
    pub date: String,
    pub trade_count: i64,
    pub realized_pnl: f64,
    pub avg_pnl_pct: f64,
}

#[tauri::command]
pub fn list_real_trades(
    filter: HistoryFilter,
    state: State<'_, AppState>,
) -> Result<Vec<LiveTrade>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Build WHERE clause dynamically. We use named placeholders to keep the
    // SQL readable; missing filters default to permissive bounds.
    let mut sql = String::from(
        "SELECT id, session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct, is_real
         FROM live_trades WHERE is_real = 1",
    );
    let mut bind: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(sid) = filter.session_id {
        sql.push_str(" AND session_id = ?");
        bind.push(rusqlite::types::Value::Integer(sid));
    }
    if let Some(s) = filter.since.as_deref() {
        sql.push_str(" AND ts >= ?");
        bind.push(rusqlite::types::Value::Text(s.to_string()));
    }
    if let Some(u) = filter.until.as_deref() {
        sql.push_str(" AND ts < ?");
        bind.push(rusqlite::types::Value::Text(u.to_string()));
    }
    sql.push_str(" ORDER BY ts DESC, id DESC LIMIT 5000");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bind.iter()), |row| {
            Ok(LiveTrade {
                id: row.get(0)?,
                session_id: row.get(1)?,
                ts: row.get(2)?,
                side: row.get(3)?,
                price: row.get(4)?,
                volume: row.get(5)?,
                fee: row.get(6)?,
                signal: row.get(7)?,
                pnl: row.get(8)?,
                pnl_pct: row.get(9)?,
                is_real: row.get::<_, i64>(10)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn real_pnl_summary(
    filter: HistoryFilter,
    state: State<'_, AppState>,
) -> Result<Vec<DailyBucket>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let mut sql = String::from(
        "SELECT substr(ts, 1, 10) AS day,
                COUNT(*) AS n,
                COALESCE(SUM(pnl), 0) AS pnl_sum,
                COALESCE(AVG(pnl_pct), 0) AS avg_pct
         FROM live_trades
         WHERE is_real = 1 AND side = 'sell'",
    );
    let mut bind: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(sid) = filter.session_id {
        sql.push_str(" AND session_id = ?");
        bind.push(rusqlite::types::Value::Integer(sid));
    }
    if let Some(s) = filter.since.as_deref() {
        sql.push_str(" AND ts >= ?");
        bind.push(rusqlite::types::Value::Text(s.to_string()));
    }
    if let Some(u) = filter.until.as_deref() {
        sql.push_str(" AND ts < ?");
        bind.push(rusqlite::types::Value::Text(u.to_string()));
    }
    sql.push_str(" GROUP BY day ORDER BY day DESC LIMIT 365");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bind.iter()), |row| {
            Ok(DailyBucket {
                date: row.get(0)?,
                trade_count: row.get(1)?,
                realized_pnl: row.get(2)?,
                avg_pnl_pct: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_real_trades_csv(
    filter: HistoryFilter,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let trades = list_real_trades(filter, state)?;
    let mut out = String::with_capacity(trades.len() * 80 + 4);
    // UTF-8 BOM so Korean Excel reads the file as UTF-8 and doesn't mangle
    // timestamp/signal/Korean text. Other tools (Notepad, Google Sheets,
    // pandas) ignore the BOM transparently.
    out.push('\u{FEFF}');
    out.push_str("id,session_id,ts,side,signal,price,volume,fee,pnl,pnl_pct\n");
    for t in trades {
        // Numeric formatting:
        //   - price/fee: integer KRW
        //   - volume: 8 decimals (Upbit precision)
        //   - pnl: integer KRW
        //   - pnl_pct: 4 decimals (basis-point granularity)
        let pnl = t.pnl.map(|v| format!("{:.0}", v)).unwrap_or_default();
        let pnl_pct = t.pnl_pct.map(|v| format!("{:.4}", v)).unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{:.0},{:.8},{:.0},{},{}\n",
            t.id, t.session_id, t.ts, t.side, t.signal,
            t.price, t.volume, t.fee, pnl, pnl_pct,
        ));
    }
    Ok(out)
}
