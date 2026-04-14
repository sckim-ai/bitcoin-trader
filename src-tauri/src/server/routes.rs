use crate::auth::{password, session};
use crate::core::indicators;
use crate::migration::csv_import;
use crate::models::market::MarketData;
use crate::models::trading::TradingParameters;
use crate::core::optimizer::{get_parameter, set_parameter};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

type ApiError = (StatusCode, String);

fn internal_err(msg: String) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, msg)
}

// --- DTOs ---

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginRes {
    pub token: String,
    pub user: UserDto,
}

#[derive(Serialize)]
pub struct UserDto {
    pub id: i64,
    pub username: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct CandlesQuery {
    pub market: String,
    pub timeframe: String,
}

#[derive(Deserialize)]
pub struct MarketQuery {
    pub market: String,
}

#[derive(Deserialize)]
pub struct SimReq {
    pub strategy_key: String,
    pub market: String,
    pub timeframe: String,
    pub params: HashMap<String, f64>,
}

// --- Routes ---

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/auth/login", post(login_handler))
        .route("/api/market/candles", get(candles_handler))
        .route("/api/market/price", get(price_handler))
        .route("/api/trading/position", get(position_handler))
        .route("/api/simulation/run", post(simulation_handler))
        .route("/api/strategies", get(strategies_handler))
}

// --- Handlers ---

async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginReq>,
) -> Result<Json<LoginRes>, ApiError> {
    let conn = state.db.lock().map_err(|e| internal_err(e.to_string()))?;

    let result: Result<(i64, String, String), rusqlite::Error> = conn.query_row(
        "SELECT id, password_hash, role FROM users WHERE username = ?1",
        [&body.username],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );
    let (id, hash, role) = result
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    if !password::verify_password(&body.password, &hash).map_err(|e| internal_err(e))? {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    let token = session::create_session(&conn, id).map_err(|e| internal_err(e))?;

    Ok(Json(LoginRes {
        token,
        user: UserDto {
            id,
            username: body.username,
            role,
        },
    }))
}

async fn candles_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CandlesQuery>,
) -> Result<Json<Vec<crate::models::market::Candle>>, ApiError> {
    let conn = state.db.lock().map_err(|e| internal_err(e.to_string()))?;
    let candles = csv_import::load_candles(&conn, &q.market, &q.timeframe)
        .map_err(|e| internal_err(e.to_string()))?;
    Ok(Json(candles))
}

async fn price_handler(
    Query(q): Query<MarketQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client = crate::api::upbit::UpbitClient::new(
        std::env::var("UPBIT_ACCESS_KEY").unwrap_or_default(),
        std::env::var("UPBIT_SECRET_KEY").unwrap_or_default(),
    );
    let price = client
        .get_current_price(&q.market)
        .await
        .map_err(|e| internal_err(e.to_string()))?;
    Ok(Json(serde_json::json!({ "price": price })))
}

async fn position_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<MarketQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // For now use user_id=1 (auth will be enforced per-route when needed)
    let conn = state.db.lock().map_err(|e| internal_err(e.to_string()))?;
    let result: Result<serde_json::Value, rusqlite::Error> = conn.query_row(
        "SELECT status, COALESCE(buy_price, 0), COALESCE(buy_volume, 0) FROM positions WHERE market = ?1 AND user_id = 1",
        rusqlite::params![q.market],
        |row| {
            let status: String = row.get(0)?;
            let buy_price: f64 = row.get(1)?;
            let buy_volume: f64 = row.get(2)?;
            Ok(serde_json::json!({
                "status": status,
                "buy_price": buy_price,
                "buy_volume": buy_volume,
                "pnl_pct": 0.0
            }))
        },
    );

    match result {
        Ok(info) => Ok(Json(info)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Json(serde_json::json!({
            "status": "idle", "buy_price": 0.0, "buy_volume": 0.0, "pnl_pct": 0.0
        }))),
        Err(e) => Err(internal_err(e.to_string())),
    }
}

async fn simulation_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SimReq>,
) -> Result<Json<crate::models::trading::SimulationResult>, ApiError> {
    let strategy = state
        .registry
        .get(&body.strategy_key)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Strategy '{}' not found", body.strategy_key)))?;

    let conn = state.db.lock().map_err(|e| internal_err(e.to_string()))?;
    let candles = csv_import::load_candles(&conn, &body.market, &body.timeframe)
        .map_err(|e| internal_err(e.to_string()))?;
    drop(conn);

    let indicator_sets = indicators::calculate_all(&candles);
    let data: Vec<MarketData> = candles
        .into_iter()
        .zip(indicator_sets)
        .map(|(candle, ind)| MarketData { candle, indicators: ind })
        .collect();

    let mut trading_params = TradingParameters::default();
    for (name, value) in &body.params {
        set_parameter(&mut trading_params, name, *value);
    }
    let _ = get_parameter(&trading_params, "fee_rate");

    let result = strategy.run_simulation(&data, &trading_params);
    Ok(Json(result))
}

async fn strategies_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<serde_json::Value>> {
    let strategies: Vec<_> = state
        .registry
        .list()
        .into_iter()
        .map(|(key, name)| serde_json::json!({ "key": key, "name": name }))
        .collect();
    Json(strategies)
}
