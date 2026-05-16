// Mirror of Rust types from src-tauri/src/models/

export interface Candle {
  timestamp: string;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface IndicatorSet {
  sma_10: number;
  sma_25: number;
  sma_60: number;
  rsi: number;
  macd: number;
  macd_signal: number;
  macd_histogram: number;
  bollinger_upper: number;
  bollinger_middle: number;
  bollinger_lower: number;
  atr: number;
  adx: number;
  di_plus: number;
  di_minus: number;
  stoch_k: number;
  stoch_d: number;
  psy_hour: number;
  psy_day: number;
}

export interface MarketData {
  candle: Candle;
  indicators: IndicatorSet;
}

export interface TradeRecord {
  buy_index: number;
  sell_index: number;
  buy_price: number;
  sell_price: number;
  pnl_pct: number;
  hold_bars: number;
  buy_signal: string;
  sell_signal: string;
  buy_timestamp: string;
  sell_timestamp: string;
}

export interface DataRange {
  market: string;
  timeframe: string;
  count: number;
  min_timestamp?: string;
  max_timestamp?: string;
}

export interface SignalEvent {
  index: number;
  timestamp: string;
  signal_type: string;
  price: number;
  position: number;
}

export interface SimulationResult {
  total_return: number;
  market_return: number;
  max_drawdown: number;
  total_trades: number;
  win_rate: number;
  profit_factor: number;
  avg_trade_return: number;
  max_consecutive_losses: number;
  buy_signals: number;
  sell_signals: number;
  last_position: number;
  last_buy_price: number;
  last_set_volume: number;
  last_signal_type: string;
  last_hold_bars: number;
  last_entry_rsi: number;
  last_highest_since_buy: number;
  trades: TradeRecord[];
  signal_log: SignalEvent[];
  sharpe_ratio: number;
  sortino_ratio: number;
  calmar_ratio: number;
  annual_return: number;
}

export interface ParameterRange {
  name: string;
  min: number;
  max: number;
  step: number;
}

export interface StrategyInfo {
  key: string;
  name: string;
  ranges: ParameterRange[];
  defaults: Record<string, number>;
}

export interface GenerationResult {
  generation: number;
  best_return: number;
  best_win_rate: number;
  front_size: number;
}

export interface ParetoSolution {
  objectives: number[];
  parameters: Record<string, number>;
  /// Full metric snapshot — includes metrics not selected as NSGA-II
  /// objectives so the Solutions table can render every column. Backend
  /// returns `{}` for rows persisted before migration 004.
  metrics?: Record<string, number>;
  rank: number;
  crowding_distance: number;
}

export interface PositionInfo {
  status: string;
  buy_price: number;
  buy_volume: number;
  pnl_pct: number;
}

export interface AutoTradeStatus {
  running: boolean;
  market: string;
  strategy: string;
  last_signal: string;
  last_check: string;
}

export interface AutoTradeLog {
  timestamp: string;
  level: string;
  message: string;
}

export interface AutoTradeEvent {
  side: string;
  market: string;
  price: number;
  volume: number;
  pnl: number | null;
  signal: string;
  strategy: string;
}

export interface UpdateResult {
  market: string;
  timeframe: string;
  new_candles: number;
}

// ─── Live Trading (Phase 1) ───

export interface Preset {
  id: number;
  user_id: number;
  name: string;
  strategy_key: string;
  params_json: string;
  source: string;
  source_run_id: number | null;
  market: string | null;
  timeframe: string | null;
  since_ts: string | null;
  until_ts: string | null;
  /** total_return % at the time the preset was saved (static baseline). */
  baseline_return: number | null;
  /** total_trades at the time the preset was saved. */
  baseline_trades: number | null;
  created_at: string;
}

export interface SavePresetArgs {
  name: string;
  strategy_key: string;
  market: string;
  timeframe?: string;
  since_ts?: string;
  until_ts?: string;
  partial_params: Record<string, number>;
  source?: string;
  source_run_id?: number;
  baseline_return?: number;
  baseline_trades?: number;
}

export interface UpbitAccount {
  id: number;
  user_id: number;
  label: string;
  enabled: boolean;
  created_at: string;
  has_access_key: boolean;
  has_secret_key: boolean;
  has_running_session: boolean;
}

export interface LiveSession {
  id: number;
  user_id: number;
  label: string;
  preset_id: number;
  market: string;
  mode: "paper" | "real";
  status: "running" | "stopped";
  initial_capital: number;
  start_ts: string;
  real_started_at: string | null;
  last_cycle_ts: string | null;
  last_signal: string | null;
  current_position: "idle" | "holding";
  current_buy_price: number | null;
  current_buy_volume: number | null;
  current_equity: number | null;
  /** Cumulative return % from real_started_at onward (closed trades only). */
  live_return: number;
  /** Daily realized loss % threshold — auto-stop when today's loss exceeds. */
  max_daily_loss_pct: number;
  /** Daily real-sell count threshold — auto-stop on reach. */
  max_daily_trades: number;
  /** Per-session BUY cap in KRW. null = no cap (full balance). */
  max_order_krw: number | null;
  upbit_account_id: number | null;
  account_label: string | null;
  created_at: string;
}

export interface LiveTrade {
  id: number;
  session_id: number;
  ts: string;
  side: "buy" | "sell";
  price: number;
  volume: number;
  fee: number;
  signal: string;
  pnl: number | null;
  pnl_pct: number | null;
  is_real: boolean;
}

export interface CreateSessionArgs {
  label: string;
  preset_id: number;
  market: string;
  initial_capital: number;
  /** Optional per-session BUY cap in KRW. Null/omitted = no cap. */
  max_order_krw?: number | null;
  upbit_account_id?: number | null;
}

export interface SessionCycleOutput {
  session_id: number;
  new_completed_trades: number;
  latest_signal: string;
  current_position: string;
  current_equity: number;
}

export interface TickData {
  market: string;
  price: number;
  change_pct: number;
  volume_24h: number;
  ts_ms: number;
}
