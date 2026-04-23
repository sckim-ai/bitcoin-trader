use serde::{Deserialize, Serialize};

/// Trading parameters for the V3 RegimeAdaptive strategy — the only live
/// strategy in this project. Fields mirror the legacy
/// `TradingConfig_V3_RegimeAdaptive_20260401_140149.json` verbatim so
/// simulation results reproduce the baseline (~278% on clean day_psy pipeline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingParameters {
    // RSI-interpolated buy parameters
    pub v3_urgent_buy_volume_lo: f64,
    pub v3_urgent_buy_volume_hi: f64,
    pub v3_urgent_buy_volume_pow: f64,
    pub v3_buy_volume_lo: f64,
    pub v3_buy_volume_hi: f64,
    pub v3_buy_volume_pow: f64,
    pub v3_buy_price_drop_lo: f64,
    pub v3_buy_price_drop_hi: f64,
    pub v3_buy_price_drop_pow: f64,
    pub v3_buy_decay_lo: f64,
    pub v3_buy_decay_hi: f64,
    pub v3_buy_decay_pow: f64,
    pub v3_buy_psy_lo: f64,
    pub v3_buy_psy_hi: f64,
    pub v3_buy_psy_pow: f64,
    pub v3_buy_wait_lo: f64,
    pub v3_buy_wait_hi: f64,
    pub v3_buy_wait_pow: f64,

    // RSI-interpolated sell parameters
    pub v3_sell_stop_loss_lo: f64,
    pub v3_sell_stop_loss_hi: f64,
    pub v3_sell_stop_loss_pow: f64,
    pub v3_sell_profit_lo: f64,
    pub v3_sell_profit_hi: f64,
    pub v3_sell_profit_pow: f64,
    pub v3_sell_volume_lo: f64,
    pub v3_sell_volume_hi: f64,
    pub v3_sell_volume_pow: f64,
    pub v3_sell_decay_lo: f64,
    pub v3_sell_decay_hi: f64,
    pub v3_sell_decay_pow: f64,
    pub v3_sell_fixed_sl_lo: f64,
    pub v3_sell_fixed_sl_hi: f64,
    pub v3_sell_fixed_sl_pow: f64,
    pub v3_sell_max_hold_lo: f64,
    pub v3_sell_max_hold_hi: f64,
    pub v3_sell_max_hold_pow: f64,

    // Misc
    pub v3_fee_rate: f64,
    pub v3_min_hold_bars: i32,
    pub v3_volume_lookback: i32,

    // V5 dual-PSY thresholds (EnhancedAdaptive overlays V3 with PsyHour+PsyDay
    // dual confirmation). V5 does NOT use v3_buy_psy_* — these 12 fields
    // replace that single-PSY filter. Defaults mirror legacy
    // NetTradingEngine.cs:254-279.
    #[serde(default = "default_v5_buy_psy_hour_lo")] pub v5_buy_psy_hour_lo: f64,
    #[serde(default = "default_v5_buy_psy_hour_hi")] pub v5_buy_psy_hour_hi: f64,
    #[serde(default = "default_v5_pow")] pub v5_buy_psy_hour_pow: f64,
    #[serde(default = "default_v5_buy_psy_day_lo")] pub v5_buy_psy_day_lo: f64,
    #[serde(default = "default_v5_buy_psy_day_hi")] pub v5_buy_psy_day_hi: f64,
    #[serde(default = "default_v5_pow")] pub v5_buy_psy_day_pow: f64,
    #[serde(default = "default_v5_sell_psy_hour_lo")] pub v5_sell_psy_hour_lo: f64,
    #[serde(default = "default_v5_sell_psy_hour_hi")] pub v5_sell_psy_hour_hi: f64,
    #[serde(default = "default_v5_pow")] pub v5_sell_psy_hour_pow: f64,
    #[serde(default = "default_v5_sell_psy_day_lo")] pub v5_sell_psy_day_lo: f64,
    #[serde(default = "default_v5_sell_psy_day_hi")] pub v5_sell_psy_day_hi: f64,
    #[serde(default = "default_v5_pow")] pub v5_sell_psy_day_pow: f64,

    // ─── V3.1 RegimeAdaptive (trade-value based) ───
    // 신호 기준: vol*close (KRW 거래대금). V3의 BTC 거래량 기반에서 전환.
    // V3의 모든 v3_* 필드와 동형이지만 스케일이 다름 (KRW 단위).
    #[serde(default = "d_v31_ubv_lo")] pub v31_urgent_buy_tv_lo: f64,
    #[serde(default = "d_v31_ubv_hi")] pub v31_urgent_buy_tv_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_urgent_buy_tv_pow: f64,
    #[serde(default = "d_v31_bv_lo")]  pub v31_buy_tv_lo: f64,
    #[serde(default = "d_v31_bv_hi")]  pub v31_buy_tv_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_buy_tv_pow: f64,
    #[serde(default = "d_v31_bpd_lo")] pub v31_buy_price_drop_lo: f64,
    #[serde(default = "d_v31_bpd_hi")] pub v31_buy_price_drop_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_buy_price_drop_pow: f64,
    #[serde(default = "d_v31_bdc_lo")] pub v31_buy_decay_lo: f64,
    #[serde(default = "d_v31_bdc_hi")] pub v31_buy_decay_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_buy_decay_pow: f64,
    #[serde(default = "d_v31_bps_lo")] pub v31_buy_psy_lo: f64,
    #[serde(default = "d_v31_bps_hi")] pub v31_buy_psy_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_buy_psy_pow: f64,
    #[serde(default = "d_v31_bw_lo")]  pub v31_buy_wait_lo: f64,
    #[serde(default = "d_v31_bw_hi")]  pub v31_buy_wait_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_buy_wait_pow: f64,

    #[serde(default = "d_v31_ssl_lo")] pub v31_sell_stop_loss_lo: f64,
    #[serde(default = "d_v31_ssl_hi")] pub v31_sell_stop_loss_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_sell_stop_loss_pow: f64,
    #[serde(default = "d_v31_spf_lo")] pub v31_sell_profit_lo: f64,
    #[serde(default = "d_v31_spf_hi")] pub v31_sell_profit_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_sell_profit_pow: f64,
    #[serde(default = "d_v31_sv_lo")]  pub v31_sell_tv_lo: f64,
    #[serde(default = "d_v31_sv_hi")]  pub v31_sell_tv_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_sell_tv_pow: f64,
    #[serde(default = "d_v31_sdc_lo")] pub v31_sell_decay_lo: f64,
    #[serde(default = "d_v31_sdc_hi")] pub v31_sell_decay_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_sell_decay_pow: f64,
    #[serde(default = "d_v31_sfx_lo")] pub v31_sell_fixed_sl_lo: f64,
    #[serde(default = "d_v31_sfx_hi")] pub v31_sell_fixed_sl_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_sell_fixed_sl_pow: f64,
    #[serde(default = "d_v31_smh_lo")] pub v31_sell_max_hold_lo: f64,
    #[serde(default = "d_v31_smh_hi")] pub v31_sell_max_hold_hi: f64,
    #[serde(default = "d_v31_pow")]    pub v31_sell_max_hold_pow: f64,

    #[serde(default = "d_v31_fee")]            pub v31_fee_rate: f64,
    #[serde(default = "d_v31_min_hold")]       pub v31_min_hold_bars: i32,
    #[serde(default = "d_v31_lookback")]       pub v31_volume_lookback: i32,

    // 신규 파라미터화된 4개 상수 (V3에서 하드코딩이었음)
    #[serde(default = "d_v31_cutoff_mult")]        pub v31_cutoff_tv_mult: f64,
    #[serde(default = "d_v31_urg_sell_mult")]      pub v31_urgent_sell_tv_mult: f64,
    #[serde(default = "d_v31_sell_ready_rise")]    pub v31_sell_ready_price_rise: f64,
    #[serde(default = "d_v31_sell_wait_max")]      pub v31_sell_wait_max: i32,
}

fn default_v5_buy_psy_hour_lo() -> f64 { 0.05 }
fn default_v5_buy_psy_hour_hi() -> f64 { -0.15 }
fn default_v5_buy_psy_day_lo() -> f64 { 0.15 }
fn default_v5_buy_psy_day_hi() -> f64 { -0.20 }
fn default_v5_sell_psy_hour_lo() -> f64 { -0.05 }
fn default_v5_sell_psy_hour_hi() -> f64 { 0.15 }
fn default_v5_sell_psy_day_lo() -> f64 { -0.10 }
fn default_v5_sell_psy_day_hi() -> f64 { 0.20 }
fn default_v5_pow() -> f64 { 1.0 }

// V3.1 defaults — ETH/hour trade value (KRW) 스케일 기준.
// V3 BTC volume 기본값에 대략 4_000_000 (ETH 가격) 을 곱한 수준으로 시작.
fn d_v31_ubv_lo() -> f64 { 8.4e10 }   // 21000 BTC-vol * 4M
fn d_v31_ubv_hi() -> f64 { 3.0e11 }   // 75000 * 4M
fn d_v31_bv_lo()  -> f64 { 2.0e10 }
fn d_v31_bv_hi()  -> f64 { 7.0e10 }
fn d_v31_bpd_lo() -> f64 { 1.045 }
fn d_v31_bpd_hi() -> f64 { 1.025 }
fn d_v31_bdc_lo() -> f64 { 0.09 }
fn d_v31_bdc_hi() -> f64 { 0.077 }
fn d_v31_bps_lo() -> f64 { 0.14 }
fn d_v31_bps_hi() -> f64 { -0.24 }
fn d_v31_bw_lo()  -> f64 { 492.0 }
fn d_v31_bw_hi()  -> f64 { 336.0 }
fn d_v31_ssl_lo() -> f64 { 0.85 }
fn d_v31_ssl_hi() -> f64 { 0.82 }
fn d_v31_spf_lo() -> f64 { 1.145 }
fn d_v31_spf_hi() -> f64 { 1.09 }
fn d_v31_sv_lo()  -> f64 { 8.0e9 }
fn d_v31_sv_hi()  -> f64 { 1.26e11 }
fn d_v31_sdc_lo() -> f64 { 0.116 }
fn d_v31_sdc_hi() -> f64 { 0.079 }
fn d_v31_sfx_lo() -> f64 { 0.08 }
fn d_v31_sfx_hi() -> f64 { 0.08 }
fn d_v31_smh_lo() -> f64 { 672.0 }
fn d_v31_smh_hi() -> f64 { 816.0 }
fn d_v31_pow()    -> f64 { 2.0 }
fn d_v31_fee()    -> f64 { 0.0005 }
fn d_v31_min_hold() -> i32 { 21 }
fn d_v31_lookback() -> i32 { 35 }
fn d_v31_cutoff_mult()     -> f64 { 1.0 }
fn d_v31_urg_sell_mult()   -> f64 { 2.0 }
fn d_v31_sell_ready_rise() -> f64 { 1.0 }
fn d_v31_sell_wait_max()   -> i32 { 168 }

impl TradingParameters {
    /// Market-aware defaults. Volume thresholds are calibrated against ETH/hour
    /// (legacy base). For BTC we scale by the ETH→BTC average-volume ratio
    /// (≈1/13) so the defaults stay in a reasonable operating range even
    /// though re-optimization for BTC is expected.
    pub fn default_for_market(market: &str) -> Self {
        let mut p = Self::default();
        const ETH_TO_BTC: f64 = 1.0 / 13.0;
        if market == "BTC" {
            p.v3_urgent_buy_volume_lo *= ETH_TO_BTC;
            p.v3_urgent_buy_volume_hi *= ETH_TO_BTC;
            p.v3_buy_volume_lo *= ETH_TO_BTC;
            p.v3_buy_volume_hi *= ETH_TO_BTC;
            p.v3_sell_volume_lo *= ETH_TO_BTC;
            p.v3_sell_volume_hi *= ETH_TO_BTC;
        }
        p
    }
}

impl Default for TradingParameters {
    fn default() -> Self {
        // Verbatim from legacy TradingConfig_V3_RegimeAdaptive_20260401_140149.json.
        Self {
            v3_urgent_buy_volume_lo: 21000.0,
            v3_urgent_buy_volume_hi: 75000.0,
            v3_urgent_buy_volume_pow: 1.3,
            v3_buy_volume_lo: 5000.0,
            v3_buy_volume_hi: 17500.0,
            v3_buy_volume_pow: 3.5,
            v3_buy_price_drop_lo: 1.045,
            v3_buy_price_drop_hi: 1.025,
            v3_buy_price_drop_pow: 2.7,
            v3_buy_decay_lo: 0.09,
            v3_buy_decay_hi: 0.077,
            v3_buy_decay_pow: 2.6,
            v3_buy_psy_lo: 0.14,
            v3_buy_psy_hi: -0.24,
            v3_buy_psy_pow: 2.4,
            v3_buy_wait_lo: 492.0,
            v3_buy_wait_hi: 336.0,
            v3_buy_wait_pow: 3.4,

            v3_sell_stop_loss_lo: 0.85,
            v3_sell_stop_loss_hi: 0.82,
            v3_sell_stop_loss_pow: 2.0,
            v3_sell_profit_lo: 1.145,
            v3_sell_profit_hi: 1.09,
            v3_sell_profit_pow: 2.0,
            v3_sell_volume_lo: 2000.0,
            v3_sell_volume_hi: 31500.0,
            v3_sell_volume_pow: 3.2,
            v3_sell_decay_lo: 0.116,
            v3_sell_decay_hi: 0.079,
            v3_sell_decay_pow: 3.3,
            v3_sell_fixed_sl_lo: 0.08,
            v3_sell_fixed_sl_hi: 0.08,
            v3_sell_fixed_sl_pow: 2.6,
            v3_sell_max_hold_lo: 672.0,
            v3_sell_max_hold_hi: 816.0,
            v3_sell_max_hold_pow: 1.9,

            v3_fee_rate: 0.0005,
            v3_min_hold_bars: 21,
            v3_volume_lookback: 35,

            v5_buy_psy_hour_lo: default_v5_buy_psy_hour_lo(),
            v5_buy_psy_hour_hi: default_v5_buy_psy_hour_hi(),
            v5_buy_psy_hour_pow: default_v5_pow(),
            v5_buy_psy_day_lo: default_v5_buy_psy_day_lo(),
            v5_buy_psy_day_hi: default_v5_buy_psy_day_hi(),
            v5_buy_psy_day_pow: default_v5_pow(),
            v5_sell_psy_hour_lo: default_v5_sell_psy_hour_lo(),
            v5_sell_psy_hour_hi: default_v5_sell_psy_hour_hi(),
            v5_sell_psy_hour_pow: default_v5_pow(),
            v5_sell_psy_day_lo: default_v5_sell_psy_day_lo(),
            v5_sell_psy_day_hi: default_v5_sell_psy_day_hi(),
            v5_sell_psy_day_pow: default_v5_pow(),

            v31_urgent_buy_tv_lo: d_v31_ubv_lo(),
            v31_urgent_buy_tv_hi: d_v31_ubv_hi(),
            v31_urgent_buy_tv_pow: d_v31_pow(),
            v31_buy_tv_lo: d_v31_bv_lo(),
            v31_buy_tv_hi: d_v31_bv_hi(),
            v31_buy_tv_pow: d_v31_pow(),
            v31_buy_price_drop_lo: d_v31_bpd_lo(),
            v31_buy_price_drop_hi: d_v31_bpd_hi(),
            v31_buy_price_drop_pow: d_v31_pow(),
            v31_buy_decay_lo: d_v31_bdc_lo(),
            v31_buy_decay_hi: d_v31_bdc_hi(),
            v31_buy_decay_pow: d_v31_pow(),
            v31_buy_psy_lo: d_v31_bps_lo(),
            v31_buy_psy_hi: d_v31_bps_hi(),
            v31_buy_psy_pow: d_v31_pow(),
            v31_buy_wait_lo: d_v31_bw_lo(),
            v31_buy_wait_hi: d_v31_bw_hi(),
            v31_buy_wait_pow: d_v31_pow(),
            v31_sell_stop_loss_lo: d_v31_ssl_lo(),
            v31_sell_stop_loss_hi: d_v31_ssl_hi(),
            v31_sell_stop_loss_pow: d_v31_pow(),
            v31_sell_profit_lo: d_v31_spf_lo(),
            v31_sell_profit_hi: d_v31_spf_hi(),
            v31_sell_profit_pow: d_v31_pow(),
            v31_sell_tv_lo: d_v31_sv_lo(),
            v31_sell_tv_hi: d_v31_sv_hi(),
            v31_sell_tv_pow: d_v31_pow(),
            v31_sell_decay_lo: d_v31_sdc_lo(),
            v31_sell_decay_hi: d_v31_sdc_hi(),
            v31_sell_decay_pow: d_v31_pow(),
            v31_sell_fixed_sl_lo: d_v31_sfx_lo(),
            v31_sell_fixed_sl_hi: d_v31_sfx_hi(),
            v31_sell_fixed_sl_pow: d_v31_pow(),
            v31_sell_max_hold_lo: d_v31_smh_lo(),
            v31_sell_max_hold_hi: d_v31_smh_hi(),
            v31_sell_max_hold_pow: d_v31_pow(),
            v31_fee_rate: d_v31_fee(),
            v31_min_hold_bars: d_v31_min_hold(),
            v31_volume_lookback: d_v31_lookback(),
            v31_cutoff_tv_mult: d_v31_cutoff_mult(),
            v31_urgent_sell_tv_mult: d_v31_urg_sell_mult(),
            v31_sell_ready_price_rise: d_v31_sell_ready_rise(),
            v31_sell_wait_max: d_v31_sell_wait_max(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub buy_index: usize,
    pub sell_index: usize,
    pub buy_price: f64,
    pub sell_price: f64,
    pub pnl_pct: f64,
    pub hold_bars: i32,
    pub buy_signal: String,
    pub sell_signal: String,
    #[serde(default)]
    pub buy_timestamp: String,
    #[serde(default)]
    pub sell_timestamp: String,
}

/// Per-bar signal state transition — ported from legacy `DetermineSignalType`.
/// Emitted whenever the signal kind changes, so the sequence is compact yet
/// complete for the timeline UI (ready → buy ready → buy → hold → sell ready → sell).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEvent {
    pub index: usize,
    pub timestamp: String,
    pub signal_type: String,
    pub price: f64,
    pub position: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub total_return: f64,
    pub market_return: f64,
    pub max_drawdown: f64,
    pub total_trades: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub avg_trade_return: f64,
    pub max_consecutive_losses: usize,
    pub fee_adjusted_return: f64,
    pub buy_signals: usize,
    pub sell_signals: usize,
    pub last_position: i32,
    pub last_buy_price: f64,
    pub last_set_volume: f64,
    pub last_signal_type: String,
    pub last_hold_bars: i32,
    pub last_entry_rsi: f64,
    pub last_highest_since_buy: f64,
    pub trades: Vec<TradeRecord>,
    #[serde(default)]
    pub signal_log: Vec<SignalEvent>,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,
    pub annual_return: f64,
}

impl Default for SimulationResult {
    fn default() -> Self {
        Self {
            total_return: 0.0,
            market_return: 0.0,
            max_drawdown: 0.0,
            total_trades: 0,
            win_rate: 0.0,
            profit_factor: 0.0,
            avg_trade_return: 0.0,
            max_consecutive_losses: 0,
            fee_adjusted_return: 0.0,
            buy_signals: 0,
            sell_signals: 0,
            last_position: 0,
            last_buy_price: 0.0,
            last_set_volume: 0.0,
            last_signal_type: String::new(),
            last_hold_bars: 0,
            last_entry_rsi: 0.0,
            last_highest_since_buy: 0.0,
            trades: Vec::new(),
            signal_log: Vec::new(),
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
            calmar_ratio: 0.0,
            annual_return: 0.0,
        }
    }
}
