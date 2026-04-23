import { create } from "zustand";
import type { ParetoSolution } from "../types";
import type { OptimizationRunSummary, OptimizationGenerationView } from "../lib/api";

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
  /// Lazily populated per-generation cache. When the user loads a past run
  /// we only fetch the latest generation — other generations are inserted
  /// here on demand as the scrub slider visits them.
  genHistory: GenerationEvent[];
  selectedGen: number | null;
  selectedSolution: ParetoSolution | null;
  error: string | null;
  /// True MAX(generation) for the currently loaded run. Drives the scrub
  /// slider's upper bound. `null` during a fresh live run — the in-flight
  /// `progress.total_generations` (the configured target) is used instead.
  maxGeneration: number | null;
  /// Generation currently being lazy-fetched (slider scrub). Gates the
  /// Pareto chart's placeholder and prevents duplicate in-flight requests.
  scrubLoading: boolean;

  // Recent Runs — cached in the store so tab re-entry is instant. The page
  // still kicks off a background refresh, but users see the last known
  // list immediately instead of an empty table.
  runs: OptimizationRunSummary[];
  runsLoading: boolean;
  /// Which run's full generation history is currently being fetched. Drives
  /// the inline spinner on the Recent Runs row — a run with ~50k saved
  /// solutions can take several seconds to deserialize + IPC-transfer.
  loadingRunId: number | null;

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
  /// Seed the view with the latest generation of a past run. Called once by
  /// the Recent Runs "Load" button; further generations are filled in by
  /// `upsertGeneration` as the slider scrubs.
  loadRunLatest: (runId: number, view: OptimizationGenerationView) => void;
  /// Add (or replace) a single generation in the cache. Keeps genHistory
  /// sorted by generation ascending so consumers can assume order.
  upsertGeneration: (runId: number, view: OptimizationGenerationView) => void;
  setScrubLoading: (b: boolean) => void;

  setRuns: (runs: OptimizationRunSummary[]) => void;
  setRunsLoading: (b: boolean) => void;
  setLoadingRunId: (id: number | null) => void;
}

function toEvent(runId: number, view: OptimizationGenerationView): GenerationEvent {
  const bestReturn = view.solutions.reduce(
    (acc, s) => Math.max(acc, s.metrics?.total_return ?? s.objectives[0] ?? 0),
    -Infinity
  );
  const bestWinRate = view.solutions.reduce(
    (acc, s) => Math.max(acc, s.metrics?.win_rate ?? s.objectives[1] ?? 0),
    -Infinity
  );
  return {
    run_id: runId,
    generation: view.generation,
    total_generations: view.max_generation,
    best_return: Number.isFinite(bestReturn) ? bestReturn : 0,
    best_win_rate: Number.isFinite(bestWinRate) ? bestWinRate : 0,
    front_size: view.solutions.length,
    front: view.solutions,
  };
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
  maxGeneration: null,
  scrubLoading: false,

  runs: [],
  runsLoading: false,
  loadingRunId: null,

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
      maxGeneration: null,
      scrubLoading: false,
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
      maxGeneration: null,
      scrubLoading: false,
    }),
  setRuns: (runs) => set({ runs }),
  setRunsLoading: (b) => set({ runsLoading: b }),
  setLoadingRunId: (id) => set({ loadingRunId: id }),
  setScrubLoading: (b) => set({ scrubLoading: b }),

  loadRunLatest: (runId, view) => {
    if (view.solutions.length === 0) return;
    const event = toEvent(runId, view);
    set({
      currentRunId: runId,
      progress: event,
      genHistory: [event],
      selectedGen: event.generation,
      selectedSolution: null,
      running: false,
      maxGeneration: view.max_generation,
      scrubLoading: false,
    });
  },

  upsertGeneration: (runId, view) => {
    const event = toEvent(runId, view);
    set((state) => {
      // Late-arriving fetch after the user switched runs — drop silently
      // so we don't pollute the new run's cache.
      if (state.currentRunId !== runId) return {};
      const filtered = state.genHistory.filter((e) => e.generation !== event.generation);
      filtered.push(event);
      filtered.sort((a, b) => a.generation - b.generation);
      return {
        genHistory: filtered,
        // Promote to progress only if this is the currently selected gen —
        // otherwise a background scrub fetch shouldn't steal the active view.
        progress: state.selectedGen === event.generation ? event : state.progress,
        scrubLoading: false,
      };
    });
  },
}));
