use crate::auth::{password, session};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

#[tauri::command]
pub fn login(
    username: String,
    password_input: String,
    state: State<'_, AppState>,
) -> Result<LoginResponse, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let (id, hash, role, created_at) = conn
        .query_row(
            "SELECT id, password_hash, role, created_at FROM users WHERE username = ?1",
            [&username],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|_| "Invalid username or password".to_string())?;

    if !password::verify_password(&password_input, &hash)? {
        return Err("Invalid username or password".to_string());
    }

    let token = session::create_session(&conn, id)?;

    Ok(LoginResponse {
        token,
        user: UserInfo {
            id,
            username,
            role,
            created_at,
        },
    })
}

#[tauri::command]
pub fn register(
    token: String,
    username: String,
    password_input: String,
    role: String,
    state: State<'_, AppState>,
) -> Result<UserInfo, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Only admin can register new users
    let caller_id = session::validate_session(&conn, &token)?
        .ok_or("Not authenticated")?;
    let caller_role: String = conn
        .query_row("SELECT role FROM users WHERE id = ?1", [caller_id], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    if caller_role != "admin" {
        return Err("Only admin can register users".to_string());
    }

    let hash = password::hash_password(&password_input)?;
    conn.execute(
        "INSERT INTO users (username, password_hash, role) VALUES (?1, ?2, ?3)",
        rusqlite::params![username, hash, role],
    )
    .map_err(|e| format!("Failed to create user: {e}"))?;

    let id = conn.last_insert_rowid();
    let created_at: String = conn
        .query_row("SELECT created_at FROM users WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;

    Ok(UserInfo {
        id,
        username,
        role,
        created_at,
    })
}

#[tauri::command]
pub fn logout(token: String, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    session::delete_session(&conn, &token)
}

#[tauri::command]
pub fn list_users(token: String, state: State<'_, AppState>) -> Result<Vec<UserInfo>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let caller_id = session::validate_session(&conn, &token)?
        .ok_or("Not authenticated")?;
    let caller_role: String = conn
        .query_row("SELECT role FROM users WHERE id = ?1", [caller_id], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    if caller_role != "admin" {
        return Err("Only admin can list users".to_string());
    }

    let mut stmt = conn
        .prepare("SELECT id, username, role, created_at FROM users ORDER BY id")
        .map_err(|e| e.to_string())?;

    let users = stmt
        .query_map([], |row| {
            Ok(UserInfo {
                id: row.get(0)?,
                username: row.get(1)?,
                role: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(users)
}

#[tauri::command]
pub fn delete_user(
    token: String,
    user_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let caller_id = session::validate_session(&conn, &token)?
        .ok_or("Not authenticated")?;
    let caller_role: String = conn
        .query_row("SELECT role FROM users WHERE id = ?1", [caller_id], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    if caller_role != "admin" {
        return Err("Only admin can delete users".to_string());
    }
    if caller_id == user_id {
        return Err("Cannot delete yourself".to_string());
    }

    conn.execute("DELETE FROM sessions WHERE user_id = ?1", [user_id])
        .map_err(|e| e.to_string())?;
    let affected = conn
        .execute("DELETE FROM users WHERE id = ?1", [user_id])
        .map_err(|e| e.to_string())?;

    if affected == 0 {
        return Err("User not found".to_string());
    }
    Ok(())
}
