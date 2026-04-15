import { useEffect, useState } from "react";
import { useMarketDataStore } from "../stores/marketDataStore";
import { startOptimization, listStrategies } from "../lib/api";
import ParetoChart from "../components/charts/ParetoChart";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Input } from "../components/ui/Input";
import { Select } from "../components/ui/Select";
import { Badge } from "../components/ui/Badge";
import { Rocket } from "lucide-react";
import type { ParetoSolution, StrategyInfo } from "../types";

export default function OptimizationPage() {
  const { market, timeframe } = useMarketDataStore();
  const [strategies, setStrategies] = useState<StrategyInfo[]>([]);
  const [selectedStrategy, setSelectedStrategy] = useState("V0");
  const [populationSize, setPopulationSize] = useState(50);
  const [generations, setGenerations] = useState(100);
  const [crossoverRate, setCrossoverRate] = useState(0.9);
  const [mutationRate, setMutationRate] = useState(0.1);
  const [minWinRate, setMinWinRate] = useState(0);
  const [minTrades, setMinTrades] = useState(0);
  const [minReturn, setMinReturn] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [solutions, setSolutions] = useState<ParetoSolution[]>([]);
  const [selectedSolution, setSelectedSolution] = useState<ParetoSolution | null>(null);

  useEffect(() => {
    listStrategies().then(setStrategies).catch(() => {});
  }, []);

  const handleRun = async () => {
    setLoading(true);
    setError(null);
    setSolutions([]);
    setSelectedSolution(null);
    try {
      const result = await startOptimization(selectedStrategy, market, timeframe, {
        population_size: populationSize,
        generations: generations,
        crossover_rate: crossoverRate,
        mutation_rate: mutationRate,
        objectives: ["total_return", "win_rate"],
        min_win_rate: minWinRate,
        min_trades: minTrades,
        min_return: minReturn,
      });
      setSolutions(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-6 animate-fade-in">
      <div className="flex items-center gap-3">
        <h1 className="text-xl font-semibold text-zinc-100">NSGA-II Optimization</h1>
        <Badge variant="blue">{market} / {timeframe}</Badge>
      </div>

      {/* Config form */}
      <Card>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <Select
              label="Strategy"
              value={selectedStrategy}
              onChange={(e) => setSelectedStrategy(e.target.value)}
              options={strategies.map((s) => ({ value: s.key, label: `${s.key} - ${s.name}` }))}
            />
            <Input
              label="Population Size"
              type="number"
              value={populationSize}
              onChange={(e) => setPopulationSize(Number(e.target.value))}
              min={10}
              max={500}
            />
            <Input
              label="Generations"
              type="number"
              value={generations}
              onChange={(e) => setGenerations(Number(e.target.value))}
              min={10}
              max={1000}
            />
            <Input
              label="Crossover Rate"
              type="number"
              value={crossoverRate}
              onChange={(e) => setCrossoverRate(Number(e.target.value))}
              min={0}
              max={1}
              step={0.05}
            />
            <Input
              label="Mutation Rate"
              type="number"
              value={mutationRate}
              onChange={(e) => setMutationRate(Number(e.target.value))}
              min={0}
              max={1}
              step={0.01}
            />
            <Input
              label="Min Win Rate (%)"
              type="number"
              value={minWinRate}
              onChange={(e) => setMinWinRate(Number(e.target.value))}
              min={0}
              max={100}
            />
            <Input
              label="Min Trades"
              type="number"
              value={minTrades}
              onChange={(e) => setMinTrades(Number(e.target.value))}
              min={0}
            />
            <Input
              label="Min Return (%)"
              type="number"
              value={minReturn}
              onChange={(e) => setMinReturn(Number(e.target.value))}
            />
          </div>
          <div className="mt-5">
            <Button onClick={handleRun} disabled={loading} size="lg">
              {loading ? (
                <>
                  <div className="w-4 h-4 border-2 border-black/30 border-t-black rounded-full animate-spin" />
                  Optimizing...
                </>
              ) : (
                <>
                  <Rocket size={16} />
                  Run Optimization
                </>
              )}
            </Button>
          </div>
        </CardContent>
      </Card>

      {error && (
        <div className="bg-rose-500/10 border border-rose-500/20 rounded-xl px-4 py-3 text-rose-400 text-sm">
          {error}
        </div>
      )}

      {/* Results */}
      {solutions.length > 0 && (
        <>
          <Card>
            <CardHeader>
              <h2 className="text-sm font-semibold text-zinc-300">Pareto Front</h2>
            </CardHeader>
            <CardContent className="p-2">
              <ParetoChart solutions={solutions} />
            </CardContent>
          </Card>

          {/* Results table */}
          <Card>
            <CardHeader className="flex items-center justify-between">
              <h2 className="text-sm font-semibold text-zinc-300">
                Pareto Solutions
              </h2>
              <Badge variant="amber">{solutions.length} found</Badge>
            </CardHeader>
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-[#1e1e26]">
                      <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">#</th>
                      <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Return %</th>
                      <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Win Rate %</th>
                      <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Crowding</th>
                      <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {solutions.map((s, i) => (
                      <tr
                        key={i}
                        className={`border-b border-[#1e1e26]/50 transition-colors cursor-pointer ${
                          selectedSolution === s ? "bg-amber-500/5" : "hover:bg-[#141419]"
                        }`}
                        onClick={() => setSelectedSolution(s)}
                      >
                        <td className="py-2.5 px-3 text-zinc-500">{i + 1}</td>
                        <td className="text-right py-2.5 px-3">
                          <span
                            className={`inline-block px-2 py-0.5 rounded-md text-xs font-semibold font-data ${
                              (s.objectives[0] ?? 0) >= 0
                                ? "bg-emerald-500/15 text-emerald-400"
                                : "bg-rose-500/15 text-rose-400"
                            }`}
                          >
                            {(s.objectives[0] ?? 0).toFixed(2)}
                          </span>
                        </td>
                        <td className="text-right py-2.5 px-3 font-data text-zinc-300">
                          {(s.objectives[1] ?? 0).toFixed(1)}
                        </td>
                        <td className="text-right py-2.5 px-3 font-data text-zinc-500">
                          {s.crowding_distance === Infinity
                            ? "Inf"
                            : s.crowding_distance.toFixed(4)}
                        </td>
                        <td className="py-2.5 px-3">
                          <button
                            className="text-amber-500 hover:text-amber-400 text-xs font-medium"
                            onClick={(e) => {
                              e.stopPropagation();
                              setSelectedSolution(s);
                            }}
                          >
                            View Params
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>

          {/* Parameter detail */}
          {selectedSolution && (
            <Card glow>
              <CardHeader>
                <h2 className="text-sm font-semibold text-zinc-300">Parameters</h2>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-sm">
                  {Object.entries(selectedSolution.parameters)
                    .filter(([, v]) => v !== 0)
                    .sort(([a], [b]) => a.localeCompare(b))
                    .map(([key, value]) => (
                      <div key={key} className="flex justify-between bg-[#141419] border border-[#1e1e26] rounded-lg px-3 py-1.5">
                        <span className="text-zinc-500 truncate mr-2 text-xs">{key}</span>
                        <span className="text-zinc-200 font-data text-xs">
                          {typeof value === "number" ? value.toFixed(4) : value}
                        </span>
                      </div>
                    ))}
                </div>
              </CardContent>
            </Card>
          )}
        </>
      )}

      {loading && (
        <Card>
          <CardContent className="py-12 text-center">
            <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-amber-500/10 mb-4">
              <div className="w-6 h-6 border-2 border-amber-500/30 border-t-amber-500 rounded-full animate-spin" />
            </div>
            <p className="text-amber-400 font-medium">Running NSGA-II optimization...</p>
            <p className="text-xs text-zinc-600 mt-1">
              Pop: {populationSize}, Gen: {generations} — This may take a while
            </p>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
