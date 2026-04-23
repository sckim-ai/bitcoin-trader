import type {
  Candle,
  DataRange,
  MarketData,
  SimulationResult,
  StrategyInfo,
  ParetoSolution,
  PositionInfo,
  AutoTradeStatus,
  UpdateResult,
} from "../types";

const isTauri = "__TAURI_INTERNALS__" in window;
const API_BASE = "http://localhost:3741";

// --- Platform-aware invoke ---

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke(cmd, args);
}

async function httpGet<T>(path: string, params?: Record<string, string>): Promise<T> {
  const url = new URL(path, API_BASE);
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      url.searchParams.set(k, v);
    }
  }
  const token = localStorage.getItem("auth_token");
  const headers: Record<string, string> = {};
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const res = await fetch(url.toString(), { headers });
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);
  return res.json();
}

async function httpPost<T>(path: string, body: unknown): Promise<T> {
  const token = localStorage.getItem("auth_token");
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const res = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);
  return res.json();
}

// --- API Functions ---

export async function loadCsvData(
  csvPath: string,
  market: string,
  timeframe: string
): Promise<number> {
  if (isTauri) return tauriInvoke("load_csv_data", { csvPath, market, timeframe });
  // CSV loading is desktop-only
  throw new Error("CSV loading is only available in desktop mode");
}

export async function getCandles(
  market: string,
  timeframe: string,
  limit?: number
): Promise<Candle[]> {
  if (isTauri) return tauriInvoke("get_candles", { market, timeframe, limit });
  const params: Record<string, string> = { market, timeframe };
  if (limit) params.limit = String(limit);
  return httpGet("/api/market/candles", params);
}

export async function getMarketData(
  market: string,
  timeframe: string
): Promise<MarketData[]> {
  if (isTauri) return tauriInvoke("get_market_data", { market, timeframe });
  // PWA: build from candles (no server-side indicator endpoint yet)
  throw new Error("Market data with indicators is only available in desktop mode");
}

export async function runSimulation(
  strategyKey: string,
  market: string,
  timeframe: string,
  params: Record<string, number>,
  since?: string | null,
  until?: string | null
): Promise<SimulationResult> {
  if (isTauri)
    return tauriInvoke("run_simulation", {
      strategyKey,
      market,
      timeframe,
      params,
      since: since ?? null,
      until: until ?? null,
    });
  return httpPost("/api/simulation/run", {
    strategy_key: strategyKey,
    market,
    timeframe,
    params,
    since: since ?? null,
    until: until ?? null,
  });
}

export async function listStrategies(market?: string): Promise<StrategyInfo[]> {
  if (isTauri) return tauriInvoke("list_strategies", { market: market ?? null });
  const qs = market ? `?market=${encodeURIComponent(market)}` : "";
  return httpGet(`/api/strategies${qs}`);
}

export async function getDataRange(market: string, timeframe: string): Promise<DataRange> {
  if (isTauri) return tauriInvoke("get_data_range", { market, timeframe });
  return httpGet(
    `/api/market/range?market=${encodeURIComponent(market)}&timeframe=${encodeURIComponent(timeframe)}`
  );
}

// Start an optimization run. Returns `run_id` immediately; the actual work
// runs in the background and emits `opt:gen` and `opt:done` events that
// the UI listens to (see OptimizationPage).
export async function startOptimization(
  strategyKey: string,
  market: string,
  timeframe: string,
  config: Record<string, unknown>
): Promise<number> {
  if (isTauri) return tauriInvoke("start_optimization", { strategyKey, market, timeframe, config });
  throw new Error("Optimization is only available in desktop mode");
}

export async function cancelOptimization(): Promise<boolean> {
  if (isTauri) return tauriInvoke("cancel_optimization");
  throw new Error("Optimization is only available in desktop mode");
}

export async function getOptimizationStatus(): Promise<{ running: boolean; run_id?: number; last_generation?: number }> {
  if (isTauri) return tauriInvoke("get_optimization_status");
  throw new Error("Optimization is only available in desktop mode");
}

export interface OptimizationRunSummary {
  id: number;
  strategy_key: string;
  population_size: number;
  generations: number;
  objectives: string;
  status: string;
  started_at: string;
  completed_at: string | null;
  best_return: number | null;
}

export async function listOptimizationRuns(limit = 50): Promise<OptimizationRunSummary[]> {
  if (isTauri) return tauriInvoke("list_optimization_runs", { limit });
  throw new Error("Optimization is only available in desktop mode");
}

export async function getOptimizationRunResults(runId: number): Promise<ParetoSolution[]> {
  if (isTauri) return tauriInvoke("get_optimization_run_results", { runId });
  throw new Error("Optimization is only available in desktop mode");
}

export interface GenerationSnapshot {
  generation: number;
  front: ParetoSolution[];
}

export async function getOptimizationRunHistory(runId: number): Promise<GenerationSnapshot[]> {
  if (isTauri) return tauriInvoke("get_optimization_run_history", { runId });
  throw new Error("Optimization is only available in desktop mode");
}

export async function deleteOptimizationRun(runId: number): Promise<void> {
  if (isTauri) return tauriInvoke("delete_optimization_run", { runId });
  throw new Error("Optimization is only available in desktop mode");
}

// Tauri event subscriptions (Tauri-only; web bypasses via HTTP polling).
export async function onOptimizationEvent<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<T>(event, (e) => handler(e.payload));
  return unlisten;
}

export async function getCurrentPrice(market: string): Promise<number> {
  if (isTauri) return tauriInvoke("get_current_price", { market });
  const data = await httpGet<{ price: number }>("/api/market/price", { market });
  return data.price;
}

export async function getBalance(currency: string): Promise<number> {
  if (isTauri) return tauriInvoke("get_balance", { currency });
  throw new Error("Balance check is only available in desktop mode");
}

export async function manualBuy(
  market: string,
  volume: number,
  price: number
): Promise<string> {
  if (isTauri) return tauriInvoke("manual_buy", { market, volume, price });
  throw new Error("Manual trading is only available in desktop mode");
}

export async function manualSell(
  market: string,
  volume: number,
  price: number
): Promise<string> {
  if (isTauri) return tauriInvoke("manual_sell", { market, volume, price });
  throw new Error("Manual trading is only available in desktop mode");
}

export async function getPosition(market: string): Promise<PositionInfo> {
  if (isTauri) return tauriInvoke("get_position", { market });
  return httpGet("/api/trading/position", { market });
}

// --- Auth API ---

export interface LoginResponse {
  token: string;
  user: { id: number; username: string; role: string };
}

export interface UserInfo {
  id: number;
  username: string;
  role: string;
  created_at: string;
}

export async function login(
  username: string,
  password: string
): Promise<LoginResponse> {
  if (isTauri) return tauriInvoke("login", { username, passwordInput: password });
  return httpPost("/api/auth/login", { username, password });
}

export async function register(
  token: string,
  username: string,
  password: string,
  role: string
): Promise<UserInfo> {
  if (isTauri) return tauriInvoke("register", { token, username, passwordInput: password, role });
  throw new Error("User registration via PWA not yet implemented");
}

export async function logout(token: string): Promise<void> {
  if (isTauri) return tauriInvoke("logout", { token });
  // For PWA, just clear local storage
  localStorage.removeItem("auth_token");
}

export async function listUsers(token: string): Promise<UserInfo[]> {
  if (isTauri) return tauriInvoke("list_users", { token });
  throw new Error("User management via PWA not yet implemented");
}

export async function deleteUser(token: string, userId: number): Promise<void> {
  if (isTauri) return tauriInvoke("delete_user", { token, userId });
  throw new Error("User management via PWA not yet implemented");
}

// --- Notification API ---

export async function saveNotificationConfig(
  token: string,
  channel: string,
  config: string,
  enabled: boolean
): Promise<void> {
  if (isTauri) return tauriInvoke("save_notification_config", { token, channel, config, enabled });
  throw new Error("Notification config via PWA not yet implemented");
}

export async function testNotification(
  token: string,
  channel: string
): Promise<string> {
  if (isTauri) return tauriInvoke("test_notification", { token, channel });
  throw new Error("Notification test via PWA not yet implemented");
}

// --- Migration API ---

export interface MigrationResult {
  hour_records: number;
  day_records: number;
  week_records: number;
}

export async function migrateFromCsv(csvDir: string): Promise<MigrationResult> {
  if (isTauri) return tauriInvoke("migrate_from_csv", { csvDir });
  throw new Error("CSV migration is only available in desktop mode");
}

// --- Auto-trading API ---

export async function startAutoTrading(
  market: string,
  strategyKey: string
): Promise<string> {
  if (isTauri) return tauriInvoke("start_auto_trading", { market, strategyKey });
  throw new Error("Auto-trading is only available in desktop mode");
}

export async function stopAutoTrading(): Promise<string> {
  if (isTauri) return tauriInvoke("stop_auto_trading");
  throw new Error("Auto-trading is only available in desktop mode");
}

export async function getAutoTradingStatus(): Promise<AutoTradeStatus> {
  if (isTauri) return tauriInvoke("get_auto_trading_status");
  throw new Error("Auto-trading is only available in desktop mode");
}

// --- Data Update API ---

export async function updateMarketData(
  market: string,
  timeframe: string
): Promise<number> {
  if (isTauri) return tauriInvoke("update_market_data", { market, timeframe });
  throw new Error("Data update is only available in desktop mode");
}

export async function autoUpdateAllMarkets(): Promise<UpdateResult[]> {
  if (isTauri) return tauriInvoke("auto_update_all_markets");
  throw new Error("Data update is only available in desktop mode");
}
