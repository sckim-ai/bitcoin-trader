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
  fee_adjusted_return: number;
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
