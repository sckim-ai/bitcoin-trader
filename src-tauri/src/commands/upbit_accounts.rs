//! Multi-Upbit-account management commands.
//!
//! add 흐름은 §4.2.3 (spec) 보상 패턴:
//!   Phase 1 (동기) — DB row insert, lock 해제
//!   Phase 2 (동기) — keyring write
//!   Phase 3 (async) — Upbit 연결 테스트
//!   실패 시 keyring 삭제 + DB row 삭제 보상.
//! AppState.db가 std::sync::Mutex이므로 MutexGuard를 `.await` 너머로 못 가져간다.

use crate::commands::upbit_keys::{
    delete_keys_for, has_keys_for, save_keys_for, upbit_client_for,
};
use crate::db::upbit_accounts_repo;
use crate::models::upbit_account::UpbitAccount;
use crate::state::AppState;
use serde::Deserialize;
use tauri::State;

fn enrich_keyring(mut accounts: Vec<UpbitAccount>) -> Vec<UpbitAccount> {
    for a in accounts.iter_mut() {
        let (ha, hs) = has_keys_for(a.id);
        a.has_access_key = ha;
        a.has_secret_key = hs;
    }
    accounts
}

#[tauri::command]
pub fn list_upbit_accounts(state: State<'_, AppState>) -> Result<Vec<UpbitAccount>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let accounts = upbit_accounts_repo::list_accounts(&conn, 1).map_err(|e| e.to_string())?;
    drop(conn);
    Ok(enrich_keyring(accounts))
}

#[derive(Deserialize)]
pub struct AddAccountArgs {
    pub label: String,
    pub access_key: String,
    pub secret_key: String,
    /// 계정 전용 Discord webhook. 비워두면 글로벌 설정을 fallback으로 사용.
    #[serde(default)]
    pub discord_webhook_url: Option<String>,
}

#[tauri::command]
pub async fn add_upbit_account(
    args: AddAccountArgs,
    state: State<'_, AppState>,
) -> Result<UpbitAccount, String> {
    let label = args.label.trim().to_string();
    if label.is_empty() {
        return Err("라벨은 비어 있을 수 없습니다.".into());
    }
    if args.access_key.trim().is_empty() || args.secret_key.trim().is_empty() {
        return Err("access_key / secret_key는 비어 있을 수 없습니다.".into());
    }

    // Phase 1: DB row insert (짧은 sync 작업, lock 해제 후 await)
    let new_id = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let id = upbit_accounts_repo::insert_account(&conn, 1, &label).map_err(|e| {
            if e.to_string().to_lowercase().contains("unique") {
                "같은 이름의 계정이 이미 있습니다.".to_string()
            } else {
                e.to_string()
            }
        })?;
        if let Some(url) = args.discord_webhook_url.as_deref() {
            let _ = upbit_accounts_repo::set_discord_webhook(&conn, id, Some(url));
        }
        id
    };

    // Phase 2: keyring write
    let access = args.access_key.trim();
    let secret = args.secret_key.trim();
    if let Err(e) = save_keys_for(new_id, access, secret) {
        let conn = state.db.lock().map_err(|err| err.to_string())?;
        let _ = upbit_accounts_repo::delete_account(&conn, new_id);
        return Err(format!("키 저장 실패: {e}"));
    }

    // Phase 3: 연결 테스트 (async)
    let test_result = match upbit_client_for(new_id) {
        Ok(client) => client.get_all_balances().await.map_err(|e| e.to_string()),
        Err(e) => Err(e),
    };
    if let Err(e) = test_result {
        delete_keys_for(new_id);
        let conn = state.db.lock().map_err(|err| err.to_string())?;
        let _ = upbit_accounts_repo::delete_account(&conn, new_id);
        return Err(format!("Upbit 연결 실패 (롤백됨): {e}"));
    }

    // 최종 응답
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let acc = upbit_accounts_repo::get_account(&conn, new_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "방금 만든 계정을 다시 읽지 못했습니다".to_string())?;
    drop(conn);
    Ok(enrich_keyring(vec![acc]).into_iter().next().unwrap())
}

#[tauri::command]
pub async fn test_upbit_account_connection(id: i64) -> Result<usize, String> {
    let client = upbit_client_for(id)?;
    let balances = client
        .get_all_balances()
        .await
        .map_err(|e| format!("Upbit API rejected: {e}"))?;
    Ok(balances.len())
}

/// 계정별 Discord webhook 테스트 — 알림 라우팅 정책(계정 webhook → 글로벌 fallback)을
/// 그대로 거쳐서 단일 텍스트 메시지를 전송. 사용자가 "이 계정이 실제로 어디로 가는지"를
/// 확인할 수 있도록 결과에 "계정 전용 채널" / "글로벌 fallback" 라벨을 포함한다.
#[tauri::command]
pub async fn test_account_discord(
    account_id: i64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use crate::notifications::discord::DiscordClient;

    // Phase 1: 정보 수집 (lock 해제 후 await)
    let (webhook_url, label, used_fallback) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let account = upbit_accounts_repo::get_account(&conn, account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "계정을 찾을 수 없습니다.".to_string())?;

        let account_url = account
            .discord_webhook_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);

        match account_url {
            Some(u) => (u, account.label, false),
            None => {
                // 글로벌 fallback — notification_configs에서 enabled=1인 discord 행 사용.
                // enabled=0이면 라이브 알림도 안 나가므로 테스트도 같은 정책.
                let global = conn
                    .query_row(
                        "SELECT config FROM notification_configs
                         WHERE user_id = 1 AND channel = 'discord' AND enabled = 1",
                        [],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                    .and_then(|cfg| serde_json::from_str::<serde_json::Value>(&cfg).ok())
                    .and_then(|v| v["webhook_url"].as_str().map(String::from))
                    .filter(|s| !s.is_empty());

                match global {
                    Some(u) => (u, account.label, true),
                    None => {
                        return Err(
                            "이 계정에도, Settings 글로벌에도 Discord webhook이 설정되어 있지 않습니다."
                                .to_string(),
                        );
                    }
                }
            }
        }
    };

    let route = if used_fallback { "글로벌 fallback" } else { "계정 전용 채널" };
    let msg = format!("[{label}] BTC Trader Discord 테스트 — {route}");
    DiscordClient::new(webhook_url)
        .send(&msg)
        .await
        .map_err(|e| format!("Discord 전송 실패: {e}"))?;

    Ok(if used_fallback {
        format!("✓ '{label}' — 계정 webhook 없어 글로벌 채널로 전송됨")
    } else {
        format!("✓ '{label}' — 계정 전용 채널로 전송 완료")
    })
}

fn guard_no_running(conn: &rusqlite::Connection, account_id: i64) -> Result<(), String> {
    let n = upbit_accounts_repo::count_running_sessions(conn, account_id)
        .map_err(|e| e.to_string())?;
    if n > 0 {
        return Err("실행 중인 세션이 있습니다. 먼저 세션을 중지하세요.".into());
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct UpdateAccountArgs {
    pub id: i64,
    pub label: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    /// `None`(필드 부재) = 변경 없음.
    /// `Some("")` 또는 공백 문자열 = NULL로 저장(글로벌 fallback).
    /// `Some(url)` = 해당 URL로 교체.
    #[serde(default)]
    pub discord_webhook_url: Option<String>,
}

#[tauri::command]
pub async fn update_upbit_account(
    args: UpdateAccountArgs,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let id = args.id;

    // 라벨만 바꾸는 경우는 가드 없이 허용 (주문 경로 영향 없음).
    if let Some(label) = args.label.as_deref() {
        let trimmed = label.trim();
        if trimmed.is_empty() {
            return Err("라벨은 비어 있을 수 없습니다.".into());
        }
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        upbit_accounts_repo::update_label(&conn, id, trimmed).map_err(|e| {
            if e.to_string().to_lowercase().contains("unique") {
                "같은 이름의 계정이 이미 있습니다.".to_string()
            } else {
                e.to_string()
            }
        })?;
    }

    // webhook 변경은 running 세션 가드 없이 허용 (주문 경로 영향 없음).
    if let Some(url) = args.discord_webhook_url.as_deref() {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        upbit_accounts_repo::set_discord_webhook(&conn, id, Some(url))
            .map_err(|e| e.to_string())?;
    }

    // 키 변경은 running 세션 가드 + 새 키 검증.
    if args.access_key.is_some() || args.secret_key.is_some() {
        {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            guard_no_running(&conn, id)?;
        } // lock 해제 후 keyring/await

        let new_access = args
            .access_key
            .ok_or_else(|| "키 갱신 시 access_key/secret_key를 모두 보내세요.".to_string())?;
        let new_secret = args
            .secret_key
            .ok_or_else(|| "키 갱신 시 access_key/secret_key를 모두 보내세요.".to_string())?;
        save_keys_for(id, new_access.trim(), new_secret.trim())?;
        let client = upbit_client_for(id)?;
        if let Err(e) = client.get_all_balances().await {
            return Err(format!("새 키 저장됨, 그러나 Upbit 연결 실패: {e}"));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn set_upbit_account_enabled(
    id: i64,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if !enabled {
        guard_no_running(&conn, id)?;
    }
    upbit_accounts_repo::set_enabled(&conn, id, enabled).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_upbit_account(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    guard_no_running(&conn, id)?;
    upbit_accounts_repo::delete_account(&conn, id).map_err(|e| e.to_string())?;
    drop(conn);
    delete_keys_for(id);
    Ok(())
}
