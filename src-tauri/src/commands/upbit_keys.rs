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
    /// Surface keyring / env failures so the UI can show why the read failed
    /// instead of silently displaying "Not configured" after a successful save.
    /// None when read succeeded.
    pub access_error: Option<String>,
    pub secret_error: Option<String>,
}

fn read_keyring(user: &str) -> (Option<String>, Option<String>) {
    match keyring::Entry::new(SERVICE, user) {
        Err(e) => (None, Some(format!("Entry::new({SERVICE}, {user}): {e}"))),
        Ok(entry) => match entry.get_password() {
            Ok(s) => (Some(s), None),
            Err(keyring::Error::NoEntry) => (None, None), // expected when not yet saved
            Err(e) => (None, Some(format!("get_password({user}): {e}"))),
        },
    }
}

/// Resolution order: keyring first, env var as fallback.
/// Returns (access_key, secret_key, source, access_error, secret_error).
pub fn load_upbit_keys_full()
    -> (Option<String>, Option<String>, &'static str, Option<String>, Option<String>)
{
    // 1) Try keyring
    let (kr_access, ae) = read_keyring(ACCESS_USER);
    let (kr_secret, se) = read_keyring(SECRET_USER);

    if kr_access.is_some() && kr_secret.is_some() {
        return (kr_access, kr_secret, "keyring", None, None);
    }

    // 2) Fallback to env
    let env_access = std::env::var("UPBIT_ACCESS_KEY").ok().filter(|s| !s.is_empty());
    let env_secret = std::env::var("UPBIT_SECRET_KEY").ok().filter(|s| !s.is_empty());
    if env_access.is_some() && env_secret.is_some() {
        return (env_access, env_secret, "env", ae, se);
    }

    // Partial keyring (e.g. only access saved) → return whichever side we have
    // but mark source as "none" so the UI prompts for completion.
    (kr_access, kr_secret, "none", ae, se)
}

/// Backwards-compatible 3-tuple wrapper for callers that don't need the
/// per-side error info (e.g. UpbitClient construction sites).
pub fn load_upbit_keys() -> (Option<String>, Option<String>, &'static str) {
    let (a, s, src, _, _) = load_upbit_keys_full();
    (a, s, src)
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
    let access_trim = access_key.trim().to_string();
    let secret_trim = secret_key.trim().to_string();

    keyring::Entry::new(SERVICE, ACCESS_USER)
        .map_err(|e| format!("keyring open access: {e}"))?
        .set_password(&access_trim)
        .map_err(|e| format!("keyring write access: {e}"))?;
    keyring::Entry::new(SERVICE, SECRET_USER)
        .map_err(|e| format!("keyring open secret: {e}"))?
        .set_password(&secret_trim)
        .map_err(|e| format!("keyring write secret: {e}"))?;

    // Read-back verification: some platforms (or sandboxed builds) report
    // write success but a fresh Entry instance fails to read the value back.
    // Catching that here gives the user a precise error instead of a silent
    // "Not configured" after the save toast.
    let (a, ae) = read_keyring(ACCESS_USER);
    let (s, se) = read_keyring(SECRET_USER);
    if a.as_deref() != Some(&access_trim) {
        return Err(format!(
            "Saved access_key but read-back failed (got {} bytes; err: {:?})",
            a.as_deref().map(str::len).unwrap_or(0), ae
        ));
    }
    if s.as_deref() != Some(&secret_trim) {
        return Err(format!(
            "Saved secret_key but read-back failed (got {} bytes; err: {:?})",
            s.as_deref().map(str::len).unwrap_or(0), se
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn get_upbit_key_status() -> UpbitKeyStatus {
    let (access, secret, source, access_error, secret_error) = load_upbit_keys_full();
    if let Some(ref e) = access_error { eprintln!("[upbit_keys] access read error: {e}"); }
    if let Some(ref e) = secret_error { eprintln!("[upbit_keys] secret read error: {e}"); }
    UpbitKeyStatus {
        has_access: access.is_some(),
        has_secret: secret.is_some(),
        source,
        access_error,
        secret_error,
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
