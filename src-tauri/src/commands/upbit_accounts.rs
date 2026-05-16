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
        upbit_accounts_repo::insert_account(&conn, 1, &label).map_err(|e| {
            if e.to_string().to_lowercase().contains("unique") {
                "같은 이름의 계정이 이미 있습니다.".to_string()
            } else {
                e.to_string()
            }
        })?
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
