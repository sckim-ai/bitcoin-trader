//! Upbit API key management via OS-native secret store.
//!
//! Uses the `keyring` crate which delegates to:
//!   - Windows: Credential Manager
//!   - macOS:   Keychain
//!   - Linux:   Secret Service (gnome-keyring / KWallet)
//!
//! Service name: "bitcoin-trader". User entries (legacy single-account):
//!   - "upbit_access_key"
//!   - "upbit_secret_key"
//!
//! Multi-account entries (account_id-scoped):
//!   - "upbit_access_key_{account_id}"
//!   - "upbit_secret_key_{account_id}"
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
#[deprecated(note = "Multi-account migration: use load_upbit_keys_for(account_id) instead. Removed in next release.")]
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
#[deprecated(note = "Multi-account migration: use load_upbit_keys_for(account_id) instead. Removed in next release.")]
#[allow(deprecated)]
pub fn load_upbit_keys() -> (Option<String>, Option<String>, &'static str) {
    let (a, s, src, _, _) = load_upbit_keys_full();
    (a, s, src)
}

/// Build a UpbitClient using whichever source has keys. Errors when neither
/// keyring nor env has a complete pair — caller should prompt the user to
/// configure keys in Settings.
#[deprecated(note = "Multi-account migration: use upbit_client_for(account_id) instead. Removed in next release.")]
#[allow(deprecated)]
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

// ── Multi-account (account_id-scoped) API ────────────────────────────────────

fn access_user(account_id: i64) -> String {
    format!("upbit_access_key_{account_id}")
}
fn secret_user(account_id: i64) -> String {
    format!("upbit_secret_key_{account_id}")
}

/// account_id 기반 키 조회. keyring만 참조 (env fallback 없음 — 마이그레이션 1회에서만 env를 봄).
/// 둘 다 있으면 `Ok((access, secret))`, 하나라도 빠지면 `Err`.
pub fn load_upbit_keys_for(account_id: i64) -> Result<(String, String), String> {
    let access = keyring::Entry::new(SERVICE, &access_user(account_id))
        .map_err(|e| format!("keyring open access({account_id}): {e}"))?
        .get_password()
        .map_err(|e| format!("keyring read access({account_id}): {e}"))?;
    let secret = keyring::Entry::new(SERVICE, &secret_user(account_id))
        .map_err(|e| format!("keyring open secret({account_id}): {e}"))?
        .get_password()
        .map_err(|e| format!("keyring read secret({account_id}): {e}"))?;
    Ok((access, secret))
}

/// account_id로 UpbitClient 생성.
pub fn upbit_client_for(account_id: i64) -> Result<UpbitClient, String> {
    let (a, s) = load_upbit_keys_for(account_id)?;
    Ok(UpbitClient::new(a, s))
}

/// 키링에 access/secret 키가 모두 있는지 boolean으로만 반환. UI list 응답 enrich용.
pub fn has_keys_for(account_id: i64) -> (bool, bool) {
    let has_access = keyring::Entry::new(SERVICE, &access_user(account_id))
        .and_then(|e| e.get_password())
        .is_ok();
    let has_secret = keyring::Entry::new(SERVICE, &secret_user(account_id))
        .and_then(|e| e.get_password())
        .is_ok();
    (has_access, has_secret)
}

/// account_id별로 keyring에 키 저장. add_upbit_account 흐름의 Phase 1에서 사용.
/// 둘 다 성공해야 Ok. 하나라도 실패하면 잔존 항목을 정리한 뒤 Err.
pub fn save_keys_for(account_id: i64, access: &str, secret: &str) -> Result<(), String> {
    if access.trim().is_empty() || secret.trim().is_empty() {
        return Err("access/secret must not be empty".into());
    }
    let a_entry = keyring::Entry::new(SERVICE, &access_user(account_id))
        .map_err(|e| format!("keyring open access: {e}"))?;
    let s_entry = keyring::Entry::new(SERVICE, &secret_user(account_id))
        .map_err(|e| format!("keyring open secret: {e}"))?;
    a_entry
        .set_password(access)
        .map_err(|e| format!("keyring write access: {e}"))?;
    if let Err(e) = s_entry.set_password(secret) {
        let _ = a_entry.delete_credential();
        return Err(format!("keyring write secret: {e}"));
    }
    Ok(())
}

/// account_id별로 keyring에서 키 삭제. delete_upbit_account 흐름에서 사용.
pub fn delete_keys_for(account_id: i64) {
    let _ = keyring::Entry::new(SERVICE, &access_user(account_id))
        .and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new(SERVICE, &secret_user(account_id))
        .and_then(|e| e.delete_credential());
}

// ── Legacy single-account API (deprecated) ────────────────────────────────────

#[deprecated(note = "Multi-account migration: use upbit_accounts::* family (Task 6/7) instead. Removed in next release.")]
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

#[allow(deprecated)]
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

#[deprecated(note = "Multi-account migration: use upbit_accounts::* family (Task 6/7) instead. Removed in next release.")]
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
#[deprecated(note = "Multi-account migration: use upbit_accounts::* family (Task 6/7) instead. Removed in next release.")]
#[allow(deprecated)]
#[tauri::command]
pub async fn test_upbit_connection() -> Result<usize, String> {
    let client = upbit_client_or_err()?;
    let balances = client
        .get_all_balances()
        .await
        .map_err(|e| format!("Upbit API rejected the keys: {e}"))?;
    Ok(balances.len())
}
