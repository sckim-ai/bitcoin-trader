//! Centralised filesystem path for the SQLite file so background tasks
//! (optimizer workers, market updater) can open their own `Connection`
//! without reaching back into the Tauri AppHandle or duplicating the
//! path-resolution logic.

use std::path::PathBuf;

/// Absolute path to `bitcoin_trader.db` under the user's local data dir.
/// Mirrors the logic used in `lib.rs::dirs_db_path` but available to any
/// module (no Tauri feature gating).
pub fn local_db_path() -> PathBuf {
    let mut path = dirs_next::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    path.push("bitcoin-trader");
    let _ = std::fs::create_dir_all(&path);
    path.push("bitcoin_trader.db");
    path
}
