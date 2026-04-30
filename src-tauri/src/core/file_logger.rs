//! File-backed logging for live cycle events. Writes are append-only, one
//! file per UTC day, located alongside the SQLite DB so users find logs
//! and DB in the same directory.
//!
//! Why a custom logger instead of `tracing` / `log`:
//!   - The volume is tiny (≤ ~50 lines per real cycle, one cycle per hour)
//!   - We want logs to coexist with `eprintln!` (stderr) so the dev console
//!     still shows everything in real time
//!   - No cross-crate sharing or log levels needed yet
//!
//! Macro:
//!   live_log!("[real cycle] session={} sim={}", session.id, sim);
//! → writes to stderr AND to logs/live_<YYYY-MM-DD>.log with a timestamp
//! prefix. Failures to write the file are silent (logs are best-effort —
//! we never want logging to break the trade loop).

use chrono::Utc;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Resolve the logs directory once. Mirrors the local DB path (LOCALAPPDATA
/// on Windows) and creates `logs/` if missing. Returns None if even directory
/// creation fails — caller should silently no-op.
fn log_dir() -> Option<&'static PathBuf> {
    if let Some(p) = LOG_DIR.get() {
        return Some(p);
    }
    let base = dirs_next::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let dir = base.join("bitcoin-trader").join("logs");
    if create_dir_all(&dir).is_err() {
        return None;
    }
    let _ = LOG_DIR.set(dir);
    LOG_DIR.get()
}

/// Append a formatted line to today's log file. Best-effort: any IO error
/// is swallowed so the trading loop never fails because of a write.
pub fn write_line(msg: &str) {
    let Some(dir) = log_dir() else { return; };
    let now = Utc::now();
    let file_name = format!("live_{}.log", now.format("%Y-%m-%d"));
    let path = dir.join(file_name);
    let stamp = now.format("%H:%M:%S%.3fZ");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {}", stamp, msg);
    }
}

/// Write to stderr AND append to today's log file. Drop-in replacement for
/// `eprintln!` — same `format!`-style args. Use for any line that should
/// survive past the dev console for post-mortem analysis.
#[macro_export]
macro_rules! live_log {
    ($($arg:tt)*) => {{
        let __msg = format!($($arg)*);
        eprintln!("{}", __msg);
        $crate::core::file_logger::write_line(&__msg);
    }};
}
