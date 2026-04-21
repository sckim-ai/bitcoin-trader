import { useEffect, useMemo } from "react";
import {
  cancelOptimization,
  getOptimizationRunHistory,
  getOptimizationStatus,
  listOptimizationRuns,
  listStrategies,
  startOptimization,
  type OptimizationRunSummary,
} from "../lib/api";
import { useSimulationStore } from "../stores/simulationStore";
import { useOptimizationStore } from "../stores/optimizationStore";
import ParetoChart from "../components/charts/ParetoChart";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Input } from "../components/ui/Input";
import { NumberInput } from "../components/ui/NumberInput";
import { Select } from "../components/ui/Select";
import { Badge } from "../components/ui/Badge";
import { Download, History, Play, Rocket, Square, X } from "lucide-react";
import type { ParetoSolution, StrategyInfo } from "../types";
import { useNavigate } from "react-router-dom";
import { useState } from "react";

// Every metric the NSGA-II evaluator produces — shown as a column in the
// Solutions table regardless of which were selected as objectives. Keys
// must match the keys the backend writes into `Individual::metrics`.
const ALL_METRICS: { key: string; label: string; minimize?: boolean }[] = [
  { key: "total_return", label: "Total Return" },
  { key: "win_rate", label: "Win Rate" },
  { key: "profit_factor", label: "Profit Factor" },
  { key: "sharpe_ratio", label: "Sharpe Ratio" },
  { key: "sortino_ratio", label: "Sortino Ratio" },
  { key: "total_trades", label: "Total Trades" },
  { key: "max_drawdown", label: "Max Drawdown", minimize: true },
];

export default function OptimizationPage() {
  const navigate = useNavigate();
  const applyOptimizedParams = useSimulationStore((s) => s.applyOptimizedParams);

  const {
    selectedStrategy,
    market,
    timeframe,
    since,
    until,
    populationSize,
    generations,
    crossoverRate,
    mutationRate,
    selectedObjectives,
    minWinRate,
    minTrades,
    minReturn,
    running,
    progress,
    genHistory,
    selectedGen,
    selectedSolution,
    error,
    patchConfig,
    setSelectedObjectives,
    startRun,
    setRunning,
    setSelectedGen,
    setSelectedSolution,
    setError,
    loadGenHistory,
  } = useOptimizationStore();

  const [strategies, setStrategies] = useState<StrategyInfo[]>([]);
  const [runs, setRuns] = useState<OptimizationRunSummary[]>([]);

  useEffect(() => {
    listStrategies(market).then(setStrategies).catch(() => {});
    refreshRuns();
    getOptimizationStatus().then((st) => {
      if (st.running) setRunning(true);
    }).catch(() => {});
  }, [market, setRunning]);

  const refreshRuns = async () => {
    try {
      setRuns(await listOptimizationRuns(30));
    } catch {
      /* silent */
    }
  };

  // When a completion event arrives (running → false), refresh run list.
  useEffect(() => {
    if (!running) refreshRuns();
  }, [running]);

  const launchRun = async (continuePrevious: boolean) => {
    setError(null);
    try {
      const runId = await startOptimization(selectedStrategy, market, timeframe, {
        population_size: populationSize,
        generations,
        crossover_rate: crossoverRate,
        mutation_rate: mutationRate,
        objectives: selectedObjectives,
        min_win_rate: minWinRate,
        min_trades: minTrades,
        min_return: minReturn,
        since: `${since}T00:00:00Z`,
        until: `${until}T23:59:59Z`,
        continue_previous: continuePrevious,
      });
      startRun(runId);
    } catch (e) {
      setError(String(e));
    }
  };
  const handleRun = () => launchRun(false);
  const handleContinue = () => launchRun(true);

  const handleCancel = async () => {
    try {
      await cancelOptimization();
    } catch (e) {
      setError(String(e));
    }
  };

  // Load every generation's Pareto front so the slider can scrub through
  // the whole run's evolution, not just the final state.
  const handleLoadRun = async (runId: number) => {
    try {
      const snapshots = await getOptimizationRunHistory(runId);
      if (snapshots.length === 0) {
        setError("No stored generations for that run.");
        return;
      }
      loadGenHistory(runId, snapshots);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleApplyToSimulation = (s: ParetoSolution) => {
    applyOptimizedParams({
      strategy: selectedStrategy,
      market,
      timeframe,
      since,
      until,
      params: s.parameters,
    });
    navigate("/simulation");
  };

  const handleExportCsv = () => {
    const shown = displayedEvent?.front ?? [];
    if (shown.length === 0) return;
    const paramKeys = Array.from(new Set(shown.flatMap((s) => Object.keys(s.parameters)))).sort();
    const header = [
      ...ALL_METRICS.map((m) => m.key),
      "rank",
      "crowding_distance",
      ...paramKeys,
    ];
    const rows = shown.map((s) => [
      ...ALL_METRICS.map((m) => {
        const v = metricValue(s, m.key);
        return v !== null ? v.toFixed(6) : "";
      }),
      String(s.rank),
      Number.isFinite(s.crowding_distance) ? s.crowding_distance.toFixed(6) : "Inf",
      ...paramKeys.map((k) => String(s.parameters[k] ?? "")),
    ]);
    const csv = [header.join(","), ...rows.map((r) => r.join(","))].join("\n");
    const blob = new Blob([csv], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `pareto_gen${displayedEvent?.generation ?? 0}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const displayedEvent = useMemo(() => {
    if (selectedGen === null) return progress;
    return genHistory.find((e) => e.generation === selectedGen) ?? progress;
  }, [selectedGen, genHistory, progress]);

  const toggleObjective = (key: string) => {
    const next = selectedObjectives.includes(key)
      ? selectedObjectives.filter((k) => k !== key)
      : [...selectedObjectives, key];
    setSelectedObjectives(next);
  };

  const progressPct =
    progress && progress.total_generations > 0
      ? Math.round((progress.generation / progress.total_generations) * 100)
      : 0;

  return (
    <div className="space-y-6 animate-fade-in">
      <div className="flex items-center gap-3 flex-wrap">
        <h1 className="text-xl font-semibold text-zinc-100">NSGA-II Optimization</h1>
        <Badge variant="blue">{market} / {timeframe}</Badge>
        {running && (
          <Badge variant="amber">
            Gen {progress?.generation ?? 0}/{progress?.total_generations ?? generations} · {progressPct}%
          </Badge>
        )}
      </div>

      {/* Config form */}
      <Card>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <Select
              label="Strategy"
              value={selectedStrategy}
              onChange={(e) => patchConfig({ selectedStrategy: e.target.value })}
              options={strategies.map((s) => ({ value: s.key, label: `${s.key} - ${s.name}` }))}
            />
            <Select
              label="Market"
              value={market}
              onChange={(e) => patchConfig({ market: e.target.value })}
              options={[
                { value: "BTC", label: "BTC" },
                { value: "ETH", label: "ETH" },
              ]}
            />
            <Select
              label="Timeframe"
              value={timeframe}
              onChange={(e) => patchConfig({ timeframe: e.target.value })}
              options={[
                { value: "hour", label: "Hour" },
                { value: "day", label: "Day" },
                { value: "week", label: "Week" },
              ]}
            />
            <div />
            <Input label="Since" type="date" value={since} onChange={(e) => patchConfig({ since: e.target.value })} />
            <Input label="Until" type="date" value={until} onChange={(e) => patchConfig({ until: e.target.value })} />
            <NumberInput
              label="Population Size"
              value={populationSize}
              onValueChange={(v) => patchConfig({ populationSize: v })}
              min={10}
              max={2000}
            />
            <NumberInput
              label="Generations"
              value={generations}
              onValueChange={(v) => patchConfig({ generations: v })}
              min={10}
              max={5000}
            />
            <NumberInput
              label="Crossover Rate"
              value={crossoverRate}
              onValueChange={(v) => patchConfig({ crossoverRate: v })}
              min={0}
              max={1}
              step={0.05}
            />
            <NumberInput
              label="Mutation Rate"
              value={mutationRate}
              onValueChange={(v) => patchConfig({ mutationRate: v })}
              min={0}
              max={1}
              step={0.01}
            />
            <NumberInput
              label="Min Win Rate (%)"
              value={minWinRate}
              onValueChange={(v) => patchConfig({ minWinRate: v })}
              min={0}
              max={100}
            />
            <NumberInput
              label="Min Trades"
              value={minTrades}
              onValueChange={(v) => patchConfig({ minTrades: v })}
              min={0}
            />
            <NumberInput
              label="Min Return (%)"
              value={minReturn}
              onValueChange={(v) => patchConfig({ minReturn: v })}
            />
          </div>

          <div className="mt-5">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-zinc-500 mb-2">
              Objectives (select at least one)
            </div>
            <div className="flex gap-2 flex-wrap">
              {ALL_METRICS.map((o) => {
                const active = selectedObjectives.includes(o.key);
                return (
                  <button
                    key={o.key}
                    type="button"
                    onClick={() => toggleObjective(o.key)}
                    className={`px-3 py-1.5 rounded-md text-xs font-medium border transition-colors ${
                      active
                        ? "bg-amber-500/15 border-amber-500/40 text-amber-400"
                        : "bg-[#141419] border-[#1e1e26] text-zinc-400 hover:border-zinc-600"
                    }`}
                  >
                    {o.label}
                    {o.minimize ? " ↓" : " ↑"}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="mt-5 flex gap-2 flex-wrap">
            <Button onClick={handleRun} disabled={running || selectedObjectives.length === 0} size="lg">
              <Rocket size={16} />
              {running ? "Running..." : "Run Optimization"}
            </Button>
            <Button
              onClick={handleContinue}
              disabled={running || selectedObjectives.length === 0}
              variant="secondary"
              size="lg"
              title="Seed the next run with the final population of the last completed run"
            >
              <Play size={16} />
              Continue
            </Button>
            <Button onClick={handleCancel} disabled={!running} variant="secondary" size="lg">
              <Square size={16} />
              Cancel
            </Button>
            <Button
              onClick={handleExportCsv}
              disabled={!displayedEvent?.front?.length}
              variant="secondary"
              size="lg"
            >
              <Download size={16} />
              Export CSV
            </Button>
          </div>
        </CardContent>
      </Card>

      {error && (
        <div className="bg-rose-500/10 border border-rose-500/20 rounded-xl px-4 py-3 text-rose-400 text-sm">
          {error}
        </div>
      )}

      {/* Unified Generation Timeline — merges the live progress bar with the
          retrospective gen slider. Works for active runs AND loaded past runs
          because `loadGenHistory` fills `genHistory` with every saved gen. */}
      {(running || genHistory.length > 0) && (() => {
        const firstGen = genHistory[0]?.generation ?? 1;
        const latestGen = genHistory[genHistory.length - 1]?.generation ?? 0;
        const targetGen = progress?.total_generations ?? Math.max(latestGen, generations);
        const progressedPct = targetGen > firstGen
          ? Math.round(((latestGen - firstGen) / (targetGen - firstGen)) * 100)
          : 100;
        return (
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between gap-3 flex-wrap">
                <h2 className="text-sm font-semibold text-zinc-300">
                  Generation Timeline — Gen {selectedGen ?? latestGen} / {targetGen}
                  {targetGen > latestGen && running && (
                    <span className="text-xs font-normal text-zinc-500 ml-2">
                      ({latestGen} computed, {progressedPct}%)
                    </span>
                  )}
                </h2>
                {progress && (
                  <div className="text-xs text-zinc-400 font-data">
                    <span className="text-emerald-400">{progress.best_return.toFixed(2)}%</span>
                    <span className="text-zinc-600 mx-2">·</span>
                    <span className="text-sky-400">WR {progress.best_win_rate.toFixed(1)}%</span>
                    <span className="text-zinc-600 mx-2">·</span>
                    <span className="text-amber-400">Front {progress.front_size}</span>
                  </div>
                )}
              </div>
            </CardHeader>
            <CardContent>
              <input
                type="range"
                min={firstGen}
                max={Math.max(targetGen, firstGen + 1)}
                value={selectedGen ?? latestGen}
                disabled={genHistory.length < 2}
                onChange={(e) => {
                  const v = Math.min(Number(e.target.value), latestGen);
                  setSelectedGen(v);
                }}
                className="w-full accent-amber-500 disabled:opacity-50"
              />
              <div className="flex justify-between text-xs text-zinc-500 mt-1">
                <span>Gen {firstGen}</span>
                <span>Gen {targetGen}</span>
              </div>
            </CardContent>
          </Card>
        );
      })()}

      {displayedEvent && displayedEvent.front.length > 0 && (
        <>
          <Card>
            <CardHeader>
              <h2 className="text-sm font-semibold text-zinc-300">
                Pareto Front (Gen {displayedEvent.generation})
              </h2>
            </CardHeader>
            <CardContent className="p-2">
              <ParetoChart solutions={displayedEvent.front} />
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex items-center justify-between">
              <h2 className="text-sm font-semibold text-zinc-300">Solutions</h2>
              <Badge variant="amber">{displayedEvent.front.length} found</Badge>
            </CardHeader>
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-[#1e1e26]">
                      <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">#</th>
                      {ALL_METRICS.map((m) => {
                        const selected = selectedObjectives.includes(m.key);
                        return (
                          <th
                            key={m.key}
                            className={`text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider ${
                              selected ? "text-amber-400" : "text-zinc-500"
                            }`}
                            title={selected ? "Selected as NSGA-II objective" : "Not an objective — shown for reference"}
                          >
                            {m.label}
                            {m.minimize ? " ↓" : ""}
                          </th>
                        );
                      })}
                      <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
                        Crowding
                      </th>
                      <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
                        Action
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {displayedEvent.front.map((s, i) => (
                      <tr
                        key={i}
                        className="border-b border-[#1e1e26]/50 hover:bg-[#141419] transition-colors cursor-pointer"
                        onClick={() => setSelectedSolution(s)}
                      >
                        <td className="py-2.5 px-3 text-zinc-500">{i + 1}</td>
                        {ALL_METRICS.map((m) => {
                          const v = metricValue(s, m.key);
                          return (
                            <td
                              key={m.key}
                              className="text-right py-2.5 px-3 font-data text-zinc-300"
                            >
                              {v !== null ? v.toFixed(2) : "—"}
                            </td>
                          );
                        })}
                        <td className="text-right py-2.5 px-3 font-data text-zinc-500">
                          {Number.isFinite(s.crowding_distance)
                            ? s.crowding_distance.toFixed(4)
                            : "Inf"}
                        </td>
                        <td className="py-2.5 px-3">
                          <button
                            className="text-sky-400 hover:text-sky-300 text-xs font-medium"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleApplyToSimulation(s);
                            }}
                          >
                            Apply → Simulation
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
        </>
      )}

      {selectedSolution && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6"
          onClick={() => setSelectedSolution(null)}
        >
          <div
            className="bg-[#0c0c0f] border border-[#1e1e26] rounded-xl shadow-2xl w-full max-w-4xl max-h-[85vh] flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-5 py-4 border-b border-[#1e1e26]">
              <div>
                <h3 className="text-sm font-semibold text-zinc-200">Solution Parameters</h3>
                <div className="mt-1 flex gap-3 text-xs text-zinc-500 flex-wrap">
                  {ALL_METRICS.map((m) => {
                    const v = metricValue(selectedSolution, m.key);
                    if (v === null) return null;
                    return (
                      <span key={m.key}>
                        {m.label}:{" "}
                        <span className="text-zinc-200 font-data">{v.toFixed(2)}</span>
                      </span>
                    );
                  })}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  size="sm"
                  onClick={() => {
                    handleApplyToSimulation(selectedSolution);
                    setSelectedSolution(null);
                  }}
                >
                  Apply → Simulation
                </Button>
                <button
                  className="text-zinc-500 hover:text-zinc-200 transition-colors"
                  onClick={() => setSelectedSolution(null)}
                  aria-label="Close"
                >
                  <X size={18} />
                </button>
              </div>
            </div>
            <div className="px-5 py-4 overflow-y-auto">
              <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-2">
                {Object.entries(selectedSolution.parameters)
                  .sort(([a], [b]) => a.localeCompare(b))
                  .map(([key, value]) => (
                    <div
                      key={key}
                      className="flex justify-between bg-[#141419] border border-[#1e1e26] rounded-lg px-3 py-1.5"
                    >
                      <span className="text-zinc-500 truncate mr-2 text-xs">{key}</span>
                      <span className="text-zinc-200 font-data text-xs">
                        {typeof value === "number" ? value.toFixed(4) : String(value)}
                      </span>
                    </div>
                  ))}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Run history */}
      <Card>
        <CardHeader className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <History size={14} className="text-zinc-500" />
            <h2 className="text-sm font-semibold text-zinc-300">Recent Runs</h2>
          </div>
          <Badge variant="default">{runs.length}</Badge>
        </CardHeader>
        <CardContent className="p-0">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[#1e1e26]">
                  <th className="text-left py-2 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Run</th>
                  <th className="text-left py-2 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Strategy</th>
                  <th className="text-left py-2 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Status</th>
                  <th className="text-right py-2 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Pop × Gen</th>
                  <th className="text-right py-2 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Best Return</th>
                  <th className="text-left py-2 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Started</th>
                  <th className="text-left py-2 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Action</th>
                </tr>
              </thead>
              <tbody>
                {runs.map((r) => (
                  <tr key={r.id} className="border-b border-[#1e1e26]/50 hover:bg-[#141419] transition-colors">
                    <td className="py-2 px-3 font-data text-zinc-500">#{r.id}</td>
                    <td className="py-2 px-3 text-zinc-300">{r.strategy_key}</td>
                    <td className="py-2 px-3">
                      <span
                        className={`inline-block px-2 py-0.5 rounded-md text-[11px] font-semibold ${
                          r.status === "completed"
                            ? "bg-emerald-500/15 text-emerald-400"
                            : r.status === "cancelled"
                            ? "bg-amber-500/15 text-amber-400"
                            : r.status === "running"
                            ? "bg-sky-500/15 text-sky-400"
                            : "bg-zinc-500/15 text-zinc-400"
                        }`}
                      >
                        {r.status}
                      </span>
                    </td>
                    <td className="text-right py-2 px-3 font-data text-zinc-400">
                      {r.population_size} × {r.generations}
                    </td>
                    <td className="text-right py-2 px-3 font-data text-zinc-300">
                      {r.best_return !== null ? `${r.best_return.toFixed(2)}%` : "—"}
                    </td>
                    <td className="py-2 px-3 font-data text-zinc-500 text-xs">
                      {r.started_at.slice(0, 16).replace("T", " ")}
                    </td>
                    <td className="py-2 px-3">
                      <button
                        className="text-sky-400 hover:text-sky-300 text-xs font-medium"
                        onClick={() => handleLoadRun(r.id)}
                      >
                        Load
                      </button>
                    </td>
                  </tr>
                ))}
                {runs.length === 0 && (
                  <tr>
                    <td colSpan={7} className="text-center py-6 text-zinc-500 text-xs">
                      No optimization runs yet.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

/// Metric lookup that prefers the backend-supplied full map and falls back
/// to the 2-tuple objectives array for legacy rows (pre-migration 004).
function metricValue(s: ParetoSolution, key: string): number | null {
  if (s.metrics && key in s.metrics) return s.metrics[key];
  // Legacy: only total_return & win_rate were stored as the first two objectives.
  if (key === "total_return") return s.objectives[0] ?? null;
  if (key === "win_rate") return s.objectives[1] ?? null;
  return null;
}
