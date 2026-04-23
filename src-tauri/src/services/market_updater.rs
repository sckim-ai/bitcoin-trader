use crate::commands::data::sync_market;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

const MARKETS: &[(&str, &str)] = &[("KRW-BTC", "BTC"), ("KRW-ETH", "ETH")];
const TIMEFRAMES: &[(&str, &str)] = &[("hour", "60"), ("day", "day"), ("week", "week")];

/// Background loop: syncs market data periodically.
/// First pass runs immediately on startup; subsequent passes every 60 seconds.
pub async fn run_loop(db: Arc<Mutex<Connection>>) {
    loop {
        for (api_market, db_market) in MARKETS {
            for (tf_name, interval) in TIMEFRAMES {
                let _ = sync_market(&db, db_market, tf_name, api_market, interval).await;
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}
