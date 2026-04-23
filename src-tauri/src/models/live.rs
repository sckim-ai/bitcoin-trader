use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub strategy_key: String,
    pub params_json: String,
    pub source: String,
    pub source_run_id: Option<i64>,
    /// Market where the preset was calibrated (e.g., "ETH", "BTC").
    pub market: Option<String>,
    /// Timeframe used when saving (e.g., "hour", "day", "week").
    pub timeframe: Option<String>,
    /// Backtest window start (YYYY-MM-DD).
    pub since_ts: Option<String>,
    /// Backtest window end (YYYY-MM-DD).
    pub until_ts: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionMode {
    #[serde(rename = "paper")]
    Paper,
    #[serde(rename = "real")]
    Real,
}

impl SessionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionMode::Paper => "paper",
            SessionMode::Real => "real",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "paper" => Some(SessionMode::Paper),
            "real" => Some(SessionMode::Real),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "stopped")]
    Stopped,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Running => "running",
            SessionStatus::Stopped => "stopped",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "running" => Some(SessionStatus::Running),
            "stopped" => Some(SessionStatus::Stopped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSession {
    pub id: i64,
    pub user_id: i64,
    pub label: String,
    pub preset_id: i64,
    pub market: String,
    pub mode: String,
    pub status: String,
    pub initial_capital: f64,
    pub start_ts: String,
    pub real_started_at: Option<String>,
    pub last_cycle_ts: Option<String>,
    pub last_signal: Option<String>,
    pub current_position: String,
    pub current_buy_price: Option<f64>,
    pub current_buy_volume: Option<f64>,
    pub current_equity: Option<f64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveTrade {
    pub id: i64,
    pub session_id: i64,
    pub ts: String,
    pub side: String,
    pub price: f64,
    pub volume: f64,
    pub fee: f64,
    pub signal: String,
    pub pnl: Option<f64>,
    pub pnl_pct: Option<f64>,
    pub is_real: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveEquityPoint {
    pub session_id: i64,
    pub ts: String,
    pub equity: f64,
    pub position: String,
}
