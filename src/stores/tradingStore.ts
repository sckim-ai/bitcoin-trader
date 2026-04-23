import { create } from "zustand";
import type { AutoTradeStatus, PositionInfo } from "../types";
import {
  getCurrentPrice,
  getBalance,
  getPosition,
  startAutoTrading as apiStartAutoTrading,
  stopAutoTrading as apiStopAutoTrading,
  getAutoTradingStatus as apiGetAutoTradingStatus,
} from "../lib/api";

interface TradingState {
  currentPrice: number;
  priceChange: number;
  balanceKrw: number;
  balanceCoin: number;
  position: PositionInfo | null;
  isMonitoring: boolean;
  isAutoTrading: boolean;
  autoTradingStatus: AutoTradeStatus | null;
  logs: string[];
  monitorInterval: ReturnType<typeof setInterval> | null;
  // actions
  fetchPrice: (market: string) => Promise<void>;
  fetchBalance: () => Promise<void>;
  fetchPosition: (market: string) => Promise<void>;
  startMonitoring: (market: string) => void;
  stopMonitoring: () => void;
  startAutoTrading: (market: string, strategyKey: string) => Promise<void>;
  stopAutoTrading: () => Promise<void>;
  fetchAutoTradingStatus: () => Promise<void>;
  addLog: (msg: string) => void;
}

export const useTradingStore = create<TradingState>((set, get) => ({
  currentPrice: 0,
  priceChange: 0,
  balanceKrw: 0,
  balanceCoin: 0,
  position: null,
  isMonitoring: false,
  isAutoTrading: false,
  autoTradingStatus: null,
  logs: [],
  monitorInterval: null,

  fetchPrice: async (market: string) => {
    try {
      const prevPrice = get().currentPrice;
      const price = await getCurrentPrice(market);
      const change = prevPrice > 0 ? ((price - prevPrice) / prevPrice) * 100 : 0;
      set({ currentPrice: price, priceChange: change });
    } catch (e) {
      get().addLog(`[ERROR] Price fetch: ${e}`);
    }
  },

  fetchBalance: async () => {
    try {
      const krw = await getBalance("KRW");
      const btc = await getBalance("BTC");
      set({ balanceKrw: krw, balanceCoin: btc });
    } catch (e) {
      get().addLog(`[ERROR] Balance fetch: ${e}`);
    }
  },

  fetchPosition: async (market: string) => {
    try {
      const pos = await getPosition(market);
      set({ position: pos });
    } catch (e) {
      get().addLog(`[ERROR] Position fetch: ${e}`);
    }
  },

  startMonitoring: (market: string) => {
    const { isMonitoring } = get();
    if (isMonitoring) return;

    get().addLog("[INFO] Monitoring started");
    const interval = setInterval(() => {
      get().fetchPrice(market);
    }, 5000);

    set({ isMonitoring: true, monitorInterval: interval });
    get().fetchPrice(market);
    get().fetchBalance();
    get().fetchPosition(market);
  },

  stopMonitoring: () => {
    const { monitorInterval } = get();
    if (monitorInterval) {
      clearInterval(monitorInterval);
    }
    set({ isMonitoring: false, monitorInterval: null });
    get().addLog("[INFO] Monitoring stopped");
  },

  startAutoTrading: async (market: string, strategyKey: string) => {
    try {
      const msg = await apiStartAutoTrading(market, strategyKey);
      set({ isAutoTrading: true });
      get().addLog(`[SUCCESS] ${msg}`);
      // Also start monitoring for price display
      if (!get().isMonitoring) {
        get().startMonitoring(market);
      }
    } catch (e) {
      get().addLog(`[ERROR] Auto-trading start failed: ${e}`);
    }
  },

  stopAutoTrading: async () => {
    try {
      const msg = await apiStopAutoTrading();
      set({ isAutoTrading: false, autoTradingStatus: null });
      get().addLog(`[SUCCESS] ${msg}`);
    } catch (e) {
      get().addLog(`[ERROR] Auto-trading stop failed: ${e}`);
    }
  },

  fetchAutoTradingStatus: async () => {
    try {
      const status = await apiGetAutoTradingStatus();
      set({ isAutoTrading: status.running, autoTradingStatus: status });
    } catch {
      // ignore
    }
  },

  addLog: (msg: string) => {
    const timestamp = new Date().toLocaleTimeString();
    set((state) => ({
      logs: [`[${timestamp}] ${msg}`, ...state.logs].slice(0, 100),
    }));
  },
}));
