import { create } from "zustand";
import type { StrategyInfo, SimulationResult } from "../types";
import {
  listStrategies,
  runSimulation as apiRunSimulation,
} from "../lib/api";

interface SimulationState {
  strategies: StrategyInfo[];
  selectedStrategy: string;
  result: SimulationResult | null;
  loading: boolean;
  error: string | null;
  setSelectedStrategy: (key: string) => void;
  fetchStrategies: () => Promise<void>;
  runSimulation: (market: string, timeframe: string) => Promise<void>;
}

export const useSimulationStore = create<SimulationState>((set, get) => ({
  strategies: [],
  selectedStrategy: "V0",
  result: null,
  loading: false,
  error: null,

  setSelectedStrategy: (key) => set({ selectedStrategy: key }),

  fetchStrategies: async () => {
    try {
      const strategies = await listStrategies();
      set({ strategies });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  runSimulation: async (market, timeframe) => {
    const { selectedStrategy } = get();
    set({ loading: true, error: null });
    try {
      const result = await apiRunSimulation(
        selectedStrategy,
        market,
        timeframe,
        {}
      );
      set({ result, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },
}));
