use crate::auth::session;
use crate::state::AppState;
use axum::{
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Auth middleware: extracts Bearer token, validates session, injects UserId.
/// Uses State extractor (requires state on router).
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer(&req)?;

    let conn = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user_id = session::validate_session(&conn, token)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    drop(conn);

    req.extensions_mut().insert(UserId(user_id));
    Ok(next.run(req).await)
}

/// Stateless version: gets AppState from request extensions (set by axum's with_state).
pub async fn auth_middleware_stateless(
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer(&req)?;

    // Get state from extensions — axum puts it there when Router has with_state
    let state = req
        .extensions()
        .get::<Arc<AppState>>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let conn = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user_id = session::validate_session(&conn, token)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    drop(conn);

    req.extensions_mut().insert(UserId(user_id));
    Ok(next.run(req).await)
}

fn extract_bearer<'a>(req: &'a Request<axum::body::Body>) -> Result<&'a str, StatusCode> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)
}

#[derive(Debug, Clone, Copy)]
pub struct UserId(pub i64);
