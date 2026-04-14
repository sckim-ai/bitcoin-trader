import { create } from "zustand";
import type { Candle } from "../types";
import { getCandles } from "../lib/api";

interface MarketDataState {
  candles: Candle[];
  market: string;
  timeframe: string;
  loading: boolean;
  error: string | null;
  setMarket: (market: string) => void;
  setTimeframe: (timeframe: string) => void;
  loadCandles: () => Promise<void>;
}

export const useMarketDataStore = create<MarketDataState>((set, get) => ({
  candles: [],
  market: "BTC",
  timeframe: "hour",
  loading: false,
  error: null,

  setMarket: (market) => set({ market }),
  setTimeframe: (timeframe) => set({ timeframe }),

  loadCandles: async () => {
    const { market, timeframe } = get();
    set({ loading: true, error: null });
    try {
      const candles = await getCandles(market, timeframe);
      set({ candles, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },
}));
