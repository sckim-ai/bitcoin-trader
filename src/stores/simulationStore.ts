import { create } from "zustand";
import type { DataRange, StrategyInfo, SimulationResult } from "../types";
import {
  getDataRange,
  listStrategies,
  runSimulation as apiRunSimulation,
} from "../lib/api";

interface SimulationState {
  strategies: StrategyInfo[];
  selectedStrategy: string;
  market: string;
  timeframe: string;
  since: string;
  until: string;
  dataRange: DataRange | null;
  params: Record<string, number>;
  /// When truthy, the next `hydrateForMarket` call preserves `params` (and
  /// the provided market/timeframe/since/until) instead of overwriting them
  /// with strategy defaults. Used by `applyOptimizedParams` so optimizer
  /// results aren't clobbered when the SimulationPage re-mounts.
  paramsPinned: boolean;
  result: SimulationResult | null;
  loading: boolean;
  error: string | null;
  setSelectedStrategy: (key: string) => Promise<void>;
  setMarket: (market: string) => Promise<void>;
  setTimeframe: (timeframe: string) => Promise<void>;
  setSince: (since: string) => void;
  setUntil: (until: string) => void;
  setParam: (name: string, value: number) => void;
  resetParams: () => void;
  fetchStrategies: () => Promise<void>;
  refreshDataRange: () => Promise<void>;
  runSimulation: () => Promise<void>;
  /// Transfer a solution from the optimizer into the simulation context.
  /// Sets strategy / market / timeframe / date range and flips `paramsPinned`
  /// so the SimulationPage's first `fetchStrategies` on mount does not
  /// replace the optimized params with the strategy defaults.
  applyOptimizedParams: (payload: {
    strategy: string;
    market: string;
    timeframe: string;
    since: string;
    until: string;
    params: Record<string, number>;
  }) => void;
}

function defaultsFor(strategies: StrategyInfo[], key: string): Record<string, number> {
  const s = strategies.find((s) => s.key === key);
  return s ? { ...s.defaults } : {};
}

// Some strategies were calibrated against a specific market (legacy C#
// project was ETH-only for V3). Selecting such a strategy auto-switches
// the simulation context so the default parameters match the legacy
// TradingConfig JSON verbatim, making "278% on clean pipeline" reproducible
// with zero manual tweaking.
const STRATEGY_LEGACY_CONTEXT: Record<
  string,
  { market: string; timeframe: string; since: string; until: string }
> = {
  V3: {
    market: "ETH",
    timeframe: "hour",
    since: "2025-01-01",
    until: "2026-03-31",
  },
};

function toIso(date: string, endOfDay = false): string | null {
  if (!date) return null;
  return endOfDay ? `${date}T23:59:59Z` : `${date}T00:00:00Z`;
}

// Re-load strategies (market-aware defaults) + data range whenever market or
// timeframe changes. The param panel is refilled with the new defaults and
// the since/until pickers are snapped to the full data range so the "Data:"
// label and the Since/Until fields stay in sync.
async function hydrateForMarket(set: (p: Partial<SimulationState>) => void, get: () => SimulationState) {
  const { market, timeframe, selectedStrategy, paramsPinned, params, since, until } = get();
  try {
    const [strategies, dataRange] = await Promise.all([
      listStrategies(market),
      getDataRange(market, timeframe).catch(() => null),
    ]);
    const chosen = strategies.find((s) => s.key === selectedStrategy)
      ? selectedStrategy
      : strategies[0]?.key ?? selectedStrategy;
    const minDate = dataRange?.min_timestamp?.slice(0, 10) ?? "";
    const maxDate = dataRange?.max_timestamp?.slice(0, 10) ?? "";
    // If params were just pinned by `applyOptimizedParams`, honour them for
    // this hydrate and clear the flag. Otherwise refresh from strategy defaults.
    const legacy = STRATEGY_LEGACY_CONTEXT[chosen];
    if (paramsPinned) {
      set({
        strategies,
        selectedStrategy: chosen,
        params, // keep the caller-supplied params intact
        dataRange,
        since,
        until,
        paramsPinned: false,
      });
      return;
    }
    set({
      strategies,
      selectedStrategy: chosen,
      params: defaultsFor(strategies, chosen),
      dataRange,
      since: legacy ? legacy.since : minDate,
      until: legacy ? legacy.until : maxDate,
    });
  } catch (e) {
    set({ error: String(e) });
  }
}

export const useSimulationStore = create<SimulationState>((set, get) => ({
  strategies: [],
  selectedStrategy: "V3",
  market: "ETH",
  timeframe: "hour",
  since: "",
  until: "",
  dataRange: null,
  params: {},
  paramsPinned: false,
  result: null,
  loading: false,
  error: null,

  setSelectedStrategy: async (key) => {
    const legacy = STRATEGY_LEGACY_CONTEXT[key];
    if (legacy) {
      // Switching into a strategy with a calibrated legacy context: align
      // market/timeframe so backend `default_for_market` matches the legacy
      // JSON exactly. `hydrateForMarket` pins since/until to the legacy
      // window automatically when the strategy has a context entry.
      set({
        selectedStrategy: key,
        market: legacy.market,
        timeframe: legacy.timeframe,
        result: null,
      });
      await hydrateForMarket(set, get);
      return;
    }
    const { strategies } = get();
    set({ selectedStrategy: key, params: defaultsFor(strategies, key), result: null });
  },

  setMarket: async (market) => {
    set({ market, result: null });
    await hydrateForMarket(set, get);
  },

  setTimeframe: async (timeframe) => {
    set({ timeframe, result: null });
    await hydrateForMarket(set, get);
  },

  setSince: (since) => set({ since }),
  setUntil: (until) => set({ until }),

  setParam: (name, value) =>
    set((state) => ({ params: { ...state.params, [name]: value } })),

  resetParams: () => {
    const { strategies, selectedStrategy } = get();
    set({ params: defaultsFor(strategies, selectedStrategy) });
  },

  fetchStrategies: async () => {
    await hydrateForMarket(set, get);
  },

  refreshDataRange: async () => {
    const { market, timeframe } = get();
    try {
      const dataRange = await getDataRange(market, timeframe);
      set({ dataRange });
    } catch { /* silent */ }
  },

  runSimulation: async () => {
    const { selectedStrategy, market, timeframe, since, until, params } = get();
    set({ loading: true, error: null });
    try {
      const result = await apiRunSimulation(
        selectedStrategy,
        market,
        timeframe,
        params,
        toIso(since, false),
        toIso(until, true)
      );
      set({ result, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  applyOptimizedParams: ({ strategy, market, timeframe, since, until, params }) => {
    set({
      selectedStrategy: strategy,
      market,
      timeframe,
      since,
      until,
      params: { ...params },
      paramsPinned: true,
      result: null,
      error: null,
    });
  },
}));
