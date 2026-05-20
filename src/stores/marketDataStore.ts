import { create } from "zustand";
import type { Candle } from "../types";
import { getCandles, updateMarketData } from "../lib/api";

interface MarketDataState {
  candles: Candle[];
  market: string;
  timeframe: string;
  since: string;
  loading: boolean;
  error: string | null;
  setMarket: (market: string) => void;
  setTimeframe: (timeframe: string) => void;
  setSince: (since: string) => void;
  loadCandles: () => Promise<void>;
  refreshCandles: () => Promise<void>;
}

function oneYearAgoIso(): string {
  const d = new Date();
  d.setFullYear(d.getFullYear() - 1);
  return d.toISOString().slice(0, 10);
}

export const useMarketDataStore = create<MarketDataState>((set, get) => ({
  candles: [],
  market: "BTC",
  timeframe: "hour",
  since: oneYearAgoIso(),
  loading: false,
  error: null,

  setMarket: (market) => set({ market }),
  setTimeframe: (timeframe) => set({ timeframe }),
  setSince: (since) => set({ since }),

  loadCandles: async () => {
    const { market, timeframe, since } = get();
    set({ loading: true, error: null });
    try {
      let candles = await getCandles(market, timeframe, undefined, since);

      // DB가 비어있으면 Upbit API에서 자동 fetch (백엔드가 SINCE 상수까지 백필)
      if (candles.length === 0) {
        await updateMarketData(market, timeframe);
        candles = await getCandles(market, timeframe, undefined, since);
      }

      set({ candles, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  // Re-pull candles from DB only (no Upbit fetch). Used by periodic refresh
  // so background-updater UPSERTs (corrected high/low) reach the chart without UI flash.
  refreshCandles: async () => {
    const { market, timeframe, since } = get();
    try {
      const candles = await getCandles(market, timeframe, undefined, since);
      if (candles.length > 0) set({ candles });
    } catch { /* silent */ }
  },
}));
