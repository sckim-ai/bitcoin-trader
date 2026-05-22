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
    /// Total return % measured during the simulation that produced this preset.
    /// None if the preset predates baseline tracking (migration 009).
    pub baseline_return: Option<f64>,
    /// Number of completed trades during that same simulation.
    pub baseline_trades: Option<i32>,
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
    /// Cumulative return % from real_started_at onward (closed trades only).
    /// Distinct from preset.baseline_return: this is the live track record,
    /// not the static backtest result.
    pub live_return: f64,
    /// Auto-stop threshold: if today's realized loss% drops below this,
    /// the session is moved to status='stopped'. Negative number (e.g. -10.0).
    pub max_daily_loss_pct: f64,
    /// Auto-stop threshold: if today's is_real=1 sell count reaches this,
    /// the session is stopped. Defends against runaway loops.
    pub max_daily_trades: i32,
    /// Per-session BUY order cap in KRW. None → use the full KRW balance
    /// (current behaviour). When set, every BUY is `min(krw_balance, cap)`
    /// before the 0.9995 fee buffer. Sells are always full balance.
    pub max_order_krw: Option<f64>,
    pub created_at: String,
    /// 이 세션이 실주문을 보낼 Upbit 계정. NULL은 마이그레이션 직전의
    /// 옛 세션에서만 발생하고, 신규 세션은 백엔드가 NOT NULL을 강제한다.
    #[serde(default)]
    pub upbit_account_id: Option<i64>,
    /// JOIN으로 가져오는 표시용 라벨. NULL이면 "[삭제됨]"으로 UI가 표시.
    #[serde(default)]
    pub account_label: Option<String>,
    /// 이 계정 전용 Discord webhook (JOIN으로 가져옴).
    /// NULL이면 `notification_configs`의 글로벌 webhook을 fallback으로 사용.
    /// 알림 전송 경로에서만 사용되며 UI에는 노출하지 않음.
    #[serde(default)]
    pub account_discord_webhook: Option<String>,
    /// (Deprecated, kept for backward-compat) — 016의 boolean 토글. 새 로직은
    /// `notify_account_ids`만 참조한다. 폐기 예정.
    #[serde(default)]
    pub notify_discord: bool,
    /// Paper 세션 알림을 발송할 Upbit 계정 ID 목록. 각 계정의 `discord_webhook_url`
    /// 으로 N번 fan-out 된다. 빈 array = 알림 off.
    /// Real 세션은 이 필드를 참조하지 않음 (자기 계정 webhook만 사용).
    #[serde(default)]
    pub notify_account_ids: Vec<i64>,
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
