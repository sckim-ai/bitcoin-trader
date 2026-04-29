//! Upbit API key management via OS-native secret store.
//!
//! Uses the `keyring` crate which delegates to:
//!   - Windows: Credential Manager
//!   - macOS:   Keychain
//!   - Linux:   Secret Service (gnome-keyring / KWallet)
//!
//! Service name: "bitcoin-trader". User entries:
//!   - "upbit_access_key"
//!   - "upbit_secret_key"
//!
//! Falls back to UPBIT_ACCESS_KEY / UPBIT_SECRET_KEY env vars only when keyring
//! has no entry — keeps existing dev workflows working without forcing a
//! migration. See `load_upbit_keys()` in this module for the resolution order.

use crate::api::upbit::UpbitClient;
use serde::Serialize;

const SERVICE: &str = "bitcoin-trader";
const ACCESS_USER: &str = "upbit_access_key";
const SECRET_USER: &str = "upbit_secret_key";

#[derive(Serialize)]
pub struct UpbitKeyStatus {
    /// True when both keys are present (in keyring or env). Booleans only —
    /// never expose the actual key values to the frontend.
    pub has_access: bool,
    pub has_secret: bool,
    /// "keyring" / "env" / "none" — tells the UI where the active keys come
    /// from so the user knows whether to expect a Settings save or an env var.
    pub source: &'static str,
}

/// Resolution order: keyring first, env var as fallback.
/// Returns (access_key, secret_key, source). source is "keyring" / "env" / "none".
pub fn load_upbit_keys() -> (Option<String>, Option<String>, &'static str) {
    // 1) Try keyring
    let kr_access = keyring::Entry::new(SERVICE, ACCESS_USER)
        .ok()
        .and_then(|e| e.get_password().ok());
    let kr_secret = keyring::Entry::new(SERVICE, SECRET_USER)
        .ok()
        .and_then(|e| e.get_password().ok());

    if kr_access.is_some() && kr_secret.is_some() {
        return (kr_access, kr_secret, "keyring");
    }

    // 2) Fallback to env
    let env_access = std::env::var("UPBIT_ACCESS_KEY").ok().filter(|s| !s.is_empty());
    let env_secret = std::env::var("UPBIT_SECRET_KEY").ok().filter(|s| !s.is_empty());
    if env_access.is_some() && env_secret.is_some() {
        return (env_access, env_secret, "env");
    }

    (None, None, "none")
}

/// Build a UpbitClient using whichever source has keys. Errors when neither
/// keyring nor env has a complete pair — caller should prompt the user to
/// configure keys in Settings.
pub fn upbit_client_or_err() -> Result<UpbitClient, String> {
    let (access, secret, source) = load_upbit_keys();
    match (access, secret) {
        (Some(a), Some(s)) => Ok(UpbitClient::new(a, s)),
        _ => Err(format!(
            "Upbit API keys not configured (source: {}). Set them in Settings.",
            source
        )),
    }
}

#[tauri::command]
pub fn save_upbit_keys(access_key: String, secret_key: String) -> Result<(), String> {
    if access_key.trim().is_empty() || secret_key.trim().is_empty() {
        return Err("access_key and secret_key must not be empty".into());
    }
    keyring::Entry::new(SERVICE, ACCESS_USER)
        .map_err(|e| format!("keyring open access: {e}"))?
        .set_password(access_key.trim())
        .map_err(|e| format!("keyring write access: {e}"))?;
    keyring::Entry::new(SERVICE, SECRET_USER)
        .map_err(|e| format!("keyring open secret: {e}"))?
        .set_password(secret_key.trim())
        .map_err(|e| format!("keyring write secret: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn get_upbit_key_status() -> UpbitKeyStatus {
    let (access, secret, source) = load_upbit_keys();
    UpbitKeyStatus {
        has_access: access.is_some(),
        has_secret: secret.is_some(),
        source,
    }
}

#[tauri::command]
pub fn clear_upbit_keys() -> Result<(), String> {
    // delete_credential() returns NoEntry error if nothing is stored — treat
    // that as success so the button is idempotent.
    let _ = keyring::Entry::new(SERVICE, ACCESS_USER)
        .map_err(|e| format!("keyring open access: {e}"))?
        .delete_credential();
    let _ = keyring::Entry::new(SERVICE, SECRET_USER)
        .map_err(|e| format!("keyring open secret: {e}"))?
        .delete_credential();
    Ok(())
}

/// Validate the keys by hitting Upbit's authenticated /v1/accounts endpoint
/// (via `get_all_balances`). Returns the number of currencies the account
/// holds — a non-zero or zero value both prove the keys work; only an Err
/// proves they don't.
#[tauri::command]
pub async fn test_upbit_connection() -> Result<usize, String> {
    let client = upbit_client_or_err()?;
    let balances = client
        .get_all_balances()
        .await
        .map_err(|e| format!("Upbit API rejected the keys: {e}"))?;
    Ok(balances.len())
}
