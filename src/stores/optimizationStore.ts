import { create } from "zustand";
import type { ParetoSolution } from "../types";
import type { OptimizationRunSummary } from "../lib/api";

export interface GenerationEvent {
  run_id: number;
  generation: number;
  total_generations: number;
  best_return: number;
  best_win_rate: number;
  front_size: number;
  front: ParetoSolution[];
}

export interface CompletionEvent {
  run_id: number;
  status: string;
  generations_run: number;
  final_front_size: number;
  elapsed_ms: number;
  error?: string | null;
}

interface OptimizationState {
  // Config inputs (persisted so tab switches keep user's tuning)
  selectedStrategy: string;
  market: string;
  timeframe: string;
  since: string;
  until: string;
  populationSize: number;
  generations: number;
  crossoverRate: number;
  mutationRate: number;
  selectedObjectives: string[];
  minWinRate: number;
  minTrades: number;
  minReturn: number;

  // Runtime
  running: boolean;
  currentRunId: number | null;
  progress: GenerationEvent | null;
  genHistory: GenerationEvent[];
  selectedGen: number | null;
  selectedSolution: ParetoSolution | null;
  error: string | null;

  // Recent Runs — cached in the store so tab re-entry is instant. The page
  // still kicks off a background refresh, but users see the last known
  // list immediately instead of an empty table.
  runs: OptimizationRunSummary[];
  runsLoading: boolean;

  // Patches & events
  patchConfig: (patch: Partial<OptimizationState>) => void;
  setSelectedObjectives: (o: string[]) => void;
  onGenerationEvent: (ev: GenerationEvent) => void;
  onCompletionEvent: (ev: CompletionEvent) => void;
  startRun: (runId: number) => void;
  setRunning: (b: boolean) => void;
  setSelectedGen: (g: number | null) => void;
  setSelectedSolution: (s: ParetoSolution | null) => void;
  setError: (e: string | null) => void;
  resetRun: () => void;
  /// Rebuild genHistory when a past run is loaded from the Recent Runs panel.
  loadGenHistory: (runId: number, snapshots: { generation: number; front: ParetoSolution[] }[]) => void;

  setRuns: (runs: OptimizationRunSummary[]) => void;
  setRunsLoading: (b: boolean) => void;
}

export const useOptimizationStore = create<OptimizationState>((set) => ({
  // Sensible defaults matching the legacy V3 context
  selectedStrategy: "V3",
  market: "ETH",
  timeframe: "hour",
  since: "2025-01-01",
  until: "2026-03-31",
  populationSize: 500,
  generations: 500,
  crossoverRate: 0.5,
  mutationRate: 0.5,
  selectedObjectives: ["total_return", "win_rate", "total_trades"],
  minWinRate: 50,
  minTrades: 20,
  minReturn: 0,

  running: false,
  currentRunId: null,
  progress: null,
  genHistory: [],
  selectedGen: null,
  selectedSolution: null,
  error: null,

  runs: [],
  runsLoading: false,

  patchConfig: (patch) => set(patch),
  setSelectedObjectives: (o) => set({ selectedObjectives: o }),
  onGenerationEvent: (ev) =>
    set((state) => ({
      progress: ev,
      genHistory: [...state.genHistory, ev],
      selectedGen: ev.generation,
    })),
  onCompletionEvent: (ev) =>
    set((state) => ({
      running: false,
      error: ev.status === "error" && ev.error ? ev.error : state.error,
    })),
  startRun: (runId) =>
    set({
      currentRunId: runId,
      running: true,
      progress: null,
      genHistory: [],
      selectedGen: null,
      selectedSolution: null,
      error: null,
    }),
  setRunning: (b) => set({ running: b }),
  setSelectedGen: (g) => set({ selectedGen: g }),
  setSelectedSolution: (s) => set({ selectedSolution: s }),
  setError: (e) => set({ error: e }),
  resetRun: () =>
    set({
      progress: null,
      genHistory: [],
      selectedGen: null,
      selectedSolution: null,
      currentRunId: null,
    }),
  setRuns: (runs) => set({ runs }),
  setRunsLoading: (b) => set({ runsLoading: b }),

  loadGenHistory: (runId, snapshots) => {
    if (snapshots.length === 0) return;
    const latest = snapshots[snapshots.length - 1];
    const genEvents: GenerationEvent[] = snapshots.map((snap) => {
      const bestReturn = snap.front.reduce(
        (acc, s) => Math.max(acc, s.metrics?.total_return ?? s.objectives[0] ?? 0),
        -Infinity
      );
      const bestWinRate = snap.front.reduce(
        (acc, s) => Math.max(acc, s.metrics?.win_rate ?? s.objectives[1] ?? 0),
        -Infinity
      );
      return {
        run_id: runId,
        generation: snap.generation,
        total_generations: latest.generation,
        best_return: Number.isFinite(bestReturn) ? bestReturn : 0,
        best_win_rate: Number.isFinite(bestWinRate) ? bestWinRate : 0,
        front_size: snap.front.length,
        front: snap.front,
      };
    });
    const lastEvent = genEvents[genEvents.length - 1];
    set({
      currentRunId: runId,
      progress: lastEvent,
      genHistory: genEvents,
      selectedGen: lastEvent.generation,
      selectedSolution: null,
      running: false,
    });
  },
}));
