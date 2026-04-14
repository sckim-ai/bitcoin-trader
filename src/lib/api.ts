import type {
  Candle,
  MarketData,
  SimulationResult,
  StrategyInfo,
  ParetoSolution,
  PositionInfo,
} from "../types";

const isTauri = "__TAURI__" in window;
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
  timeframe: string
): Promise<Candle[]> {
  if (isTauri) return tauriInvoke("get_candles", { market, timeframe });
  return httpGet("/api/market/candles", { market, timeframe });
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
  params: Record<string, number>
): Promise<SimulationResult> {
  if (isTauri) return tauriInvoke("run_simulation", { strategyKey, market, timeframe, params });
  return httpPost("/api/simulation/run", {
    strategy_key: strategyKey,
    market,
    timeframe,
    params,
  });
}

export async function listStrategies(): Promise<StrategyInfo[]> {
  if (isTauri) return tauriInvoke("list_strategies");
  return httpGet("/api/strategies");
}

export async function startOptimization(
  strategyKey: string,
  market: string,
  timeframe: string,
  config: Record<string, unknown>
): Promise<ParetoSolution[]> {
  if (isTauri) return tauriInvoke("start_optimization", { strategyKey, market, timeframe, config });
  throw new Error("Optimization is only available in desktop mode");
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
