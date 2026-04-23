import { create } from "zustand";
import type { Candle } from "../types";
import { getCandles, updateMarketData } from "../lib/api";

interface MarketDataState {
  candles: Candle[];
  market: string;
  timeframe: string;
  limit: number;
  loading: boolean;
  error: string | null;
  setMarket: (market: string) => void;
  setTimeframe: (timeframe: string) => void;
  setLimit: (limit: number) => void;
  loadCandles: () => Promise<void>;
  refreshCandles: () => Promise<void>;
}

export const useMarketDataStore = create<MarketDataState>((set, get) => ({
  candles: [],
  market: "BTC",
  timeframe: "hour",
  limit: 500,
  loading: false,
  error: null,

  setMarket: (market) => set({ market }),
  setTimeframe: (timeframe) => set({ timeframe }),
  setLimit: (limit) => set({ limit }),

  loadCandles: async () => {
    const { market, timeframe, limit } = get();
    set({ loading: true, error: null });
    try {
      let candles = await getCandles(market, timeframe, limit);

      // DB가 비어있으면 Upbit API에서 자동 fetch
      if (candles.length === 0) {
        await updateMarketData(market, timeframe);
        candles = await getCandles(market, timeframe, limit);
      }

      set({ candles, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  // Re-pull candles from DB only (no Upbit fetch). Used by periodic refresh
  // so background-updater UPSERTs (corrected high/low) reach the chart without UI flash.
  refreshCandles: async () => {
    const { market, timeframe, limit } = get();
    try {
      const candles = await getCandles(market, timeframe, limit);
      if (candles.length > 0) set({ candles });
    } catch { /* silent */ }
  },
}));
