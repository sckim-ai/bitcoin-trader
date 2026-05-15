//! File-backed logging for live cycle events. Writes are append-only, one
//! file per KST day, located alongside the SQLite DB so users find logs
//! and DB in the same directory.
//!
//! Why a custom logger instead of `tracing` / `log`:
//!   - The volume is tiny (≤ ~50 lines per real cycle, one cycle per hour)
//!   - We want logs to coexist with stderr so the dev console
//!     still shows everything in real time
//!   - No cross-crate sharing or log levels needed yet
//!
//! Macro:
//!   live_log!("[real cycle] session={} sim={}", session.id, sim);
//! → writes to stderr AND to logs/live_<YYYY-MM-DD>.log with the SAME
//! KST timestamp prefix (`HH:MM:SS.mmm KST `). Failures to write the file
//! are silent (logs are best-effort — we never want logging to break the
//! trade loop).

use chrono::{Duration, Utc};
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::sync::broadcast;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Live-log broadcast channel for the UI stream. `None` until the Tauri
/// setup phase calls `init_log_broadcast`. Capacity 256 — older lines drop
/// when the receiver lags. Drop is fine: the file is the durable record.
static LOG_TX: OnceLock<broadcast::Sender<String>> = OnceLock::new();

/// Initialise the live-log broadcast channel and return a receiver. Call
/// once during Tauri setup; subsequent calls are no-ops on the sender side
/// but still return a fresh receiver subscribed to the original channel.
pub fn init_log_broadcast() -> broadcast::Receiver<String> {
    if let Some(tx) = LOG_TX.get() {
        return tx.subscribe();
    }
    let (tx, rx) = broadcast::channel(256);
    let _ = LOG_TX.set(tx);
    rx
}

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

/// Build the KST timestamp prefix shared by stderr and file output. Single
/// source so the same wallclock value lands in both sinks. KST is the
/// project's user-facing timezone (Manual + UI display all KST).
fn kst_stamp() -> String {
    let kst = Utc::now() + Duration::hours(9);
    kst.format("%H:%M:%S%.3f KST").to_string()
}

/// Write to stderr AND append to today's log file with a single shared KST
/// timestamp prefix. Best-effort file write. Drop-in replacement for
/// `eprintln!` — same `format!`-style args via the `live_log!` macro.
pub fn write_with_stamp(msg: &str) {
    let stamp = kst_stamp();
    let line = format!("{} {}", stamp, msg);
    eprintln!("{}", line);

    let Some(dir) = log_dir() else { return; };
    // Filename uses KST date so ymd matches Asia/Seoul midnight rollovers,
    // matching how the user thinks about "today's log".
    let kst = Utc::now() + Duration::hours(9);
    let file_name = format!("live_{}.log", kst.format("%Y-%m-%d"));
    let path = dir.join(file_name);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}", line);
    }

    // Best-effort UI broadcast. Errors (no receivers, channel closed) are
    // intentionally swallowed — the file is the durable record.
    if let Some(tx) = LOG_TX.get() {
        let _ = tx.send(line);
    }
}

/// Write to stderr AND append to today's log file. Drop-in replacement for
/// `eprintln!` — same `format!`-style args. Use for any line that should
/// survive past the dev console for post-mortem analysis.
#[macro_export]
macro_rules! live_log {
    ($($arg:tt)*) => {{
        let __msg = format!($($arg)*);
        $crate::core::file_logger::write_with_stamp(&__msg);
    }};
}
