import { invoke } from "@tauri-apps/api/core";
import type {
  Candle,
  MarketData,
  SimulationResult,
  StrategyInfo,
  ParetoSolution,
  PositionInfo,
} from "../types";

export async function loadCsvData(
  csvPath: string,
  market: string,
  timeframe: string
): Promise<number> {
  return invoke("load_csv_data", {
    csvPath,
    market,
    timeframe,
  });
}

export async function getCandles(
  market: string,
  timeframe: string
): Promise<Candle[]> {
  return invoke("get_candles", { market, timeframe });
}

export async function getMarketData(
  market: string,
  timeframe: string
): Promise<MarketData[]> {
  return invoke("get_market_data", { market, timeframe });
}

export async function runSimulation(
  strategyKey: string,
  market: string,
  timeframe: string,
  params: Record<string, number>
): Promise<SimulationResult> {
  return invoke("run_simulation", {
    strategyKey,
    market,
    timeframe,
    params,
  });
}

export async function listStrategies(): Promise<StrategyInfo[]> {
  return invoke("list_strategies");
}

export async function startOptimization(
  strategyKey: string,
  market: string,
  timeframe: string,
  config: Record<string, unknown>
): Promise<ParetoSolution[]> {
  return invoke("start_optimization", {
    strategyKey,
    market,
    timeframe,
    config,
  });
}

export async function getCurrentPrice(market: string): Promise<number> {
  return invoke("get_current_price", { market });
}

export async function getBalance(currency: string): Promise<number> {
  return invoke("get_balance", { currency });
}

export async function manualBuy(
  market: string,
  volume: number,
  price: number
): Promise<string> {
  return invoke("manual_buy", { market, volume, price });
}

export async function manualSell(
  market: string,
  volume: number,
  price: number
): Promise<string> {
  return invoke("manual_sell", { market, volume, price });
}

export async function getPosition(market: string): Promise<PositionInfo> {
  return invoke("get_position", { market });
}
