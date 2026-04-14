import { useEffect, useState } from "react";
import { useMarketDataStore } from "../stores/marketDataStore";
import { startOptimization, listStrategies } from "../lib/api";
import ParetoChart from "../components/charts/ParetoChart";
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
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">NSGA-II Optimization</h1>

      {/* Controls */}
      <div className="bg-gray-900 rounded-lg p-4">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {/* Strategy */}
          <div>
            <label className="block text-sm text-gray-400 mb-1">Strategy</label>
            <select
              value={selectedStrategy}
              onChange={(e) => setSelectedStrategy(e.target.value)}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
            >
              {strategies.map((s) => (
                <option key={s.key} value={s.key}>
                  {s.key} - {s.name}
                </option>
              ))}
            </select>
          </div>

          {/* Market info */}
          <div className="flex items-end pb-2">
            <span className="text-sm text-gray-400">
              {market} / {timeframe}
            </span>
          </div>

          {/* Population Size */}
          <div>
            <label className="block text-sm text-gray-400 mb-1">Population Size</label>
            <input
              type="number"
              value={populationSize}
              onChange={(e) => setPopulationSize(Number(e.target.value))}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
              min={10}
              max={500}
            />
          </div>

          {/* Generations */}
          <div>
            <label className="block text-sm text-gray-400 mb-1">Generations</label>
            <input
              type="number"
              value={generations}
              onChange={(e) => setGenerations(Number(e.target.value))}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
              min={10}
              max={1000}
            />
          </div>

          {/* Crossover Rate */}
          <div>
            <label className="block text-sm text-gray-400 mb-1">Crossover Rate</label>
            <input
              type="number"
              value={crossoverRate}
              onChange={(e) => setCrossoverRate(Number(e.target.value))}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
              min={0}
              max={1}
              step={0.05}
            />
          </div>

          {/* Mutation Rate */}
          <div>
            <label className="block text-sm text-gray-400 mb-1">Mutation Rate</label>
            <input
              type="number"
              value={mutationRate}
              onChange={(e) => setMutationRate(Number(e.target.value))}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
              min={0}
              max={1}
              step={0.01}
            />
          </div>

          {/* Min Win Rate */}
          <div>
            <label className="block text-sm text-gray-400 mb-1">Min Win Rate (%)</label>
            <input
              type="number"
              value={minWinRate}
              onChange={(e) => setMinWinRate(Number(e.target.value))}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
              min={0}
              max={100}
            />
          </div>

          {/* Min Trades */}
          <div>
            <label className="block text-sm text-gray-400 mb-1">Min Trades</label>
            <input
              type="number"
              value={minTrades}
              onChange={(e) => setMinTrades(Number(e.target.value))}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
              min={0}
            />
          </div>

          {/* Min Return */}
          <div>
            <label className="block text-sm text-gray-400 mb-1">Min Return (%)</label>
            <input
              type="number"
              value={minReturn}
              onChange={(e) => setMinReturn(Number(e.target.value))}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
            />
          </div>
        </div>

        <div className="mt-4">
          <button
            onClick={handleRun}
            disabled={loading}
            className="bg-violet-600 hover:bg-violet-700 disabled:bg-gray-700 text-white px-8 py-2 rounded font-medium transition-colors"
          >
            {loading ? "Optimizing..." : "Run Optimization"}
          </button>
        </div>
      </div>

      {error && <p className="text-red-400">{error}</p>}

      {/* Results */}
      {solutions.length > 0 && (
        <>
          <div className="bg-gray-900 rounded-lg p-4">
            <ParetoChart solutions={solutions} />
          </div>

          {/* Results table */}
          <div className="bg-gray-900 rounded-lg p-4 overflow-x-auto">
            <h2 className="text-lg font-semibold mb-3">
              Pareto Front ({solutions.length} solutions)
            </h2>
            <table className="w-full text-sm">
              <thead>
                <tr className="text-gray-400 border-b border-gray-800">
                  <th className="text-left py-2 px-2">#</th>
                  <th className="text-right py-2 px-2">Total Return (%)</th>
                  <th className="text-right py-2 px-2">Win Rate (%)</th>
                  <th className="text-right py-2 px-2">Crowding Dist</th>
                  <th className="text-left py-2 px-2">Action</th>
                </tr>
              </thead>
              <tbody>
                {solutions.map((s, i) => (
                  <tr
                    key={i}
                    className={`border-b border-gray-800/50 hover:bg-gray-800/50 cursor-pointer ${
                      selectedSolution === s ? "bg-violet-900/30" : ""
                    }`}
                    onClick={() => setSelectedSolution(s)}
                  >
                    <td className="py-2 px-2">{i + 1}</td>
                    <td
                      className={`text-right py-2 px-2 ${
                        (s.objectives[0] ?? 0) >= 0 ? "text-green-400" : "text-red-400"
                      }`}
                    >
                      {(s.objectives[0] ?? 0).toFixed(2)}
                    </td>
                    <td className="text-right py-2 px-2">
                      {(s.objectives[1] ?? 0).toFixed(1)}
                    </td>
                    <td className="text-right py-2 px-2 text-gray-400">
                      {s.crowding_distance === Infinity
                        ? "Inf"
                        : s.crowding_distance.toFixed(4)}
                    </td>
                    <td className="py-2 px-2">
                      <button
                        className="text-violet-400 hover:text-violet-300 text-xs"
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

          {/* Parameter detail */}
          {selectedSolution && (
            <div className="bg-gray-900 rounded-lg p-4">
              <h2 className="text-lg font-semibold mb-3">Parameters</h2>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-sm">
                {Object.entries(selectedSolution.parameters)
                  .filter(([_, v]) => v !== 0)
                  .sort(([a], [b]) => a.localeCompare(b))
                  .map(([key, value]) => (
                    <div key={key} className="flex justify-between bg-gray-800 rounded px-3 py-1">
                      <span className="text-gray-400 truncate mr-2">{key}</span>
                      <span className="text-white font-mono">
                        {typeof value === "number" ? value.toFixed(4) : value}
                      </span>
                    </div>
                  ))}
              </div>
            </div>
          )}
        </>
      )}

      {loading && (
        <div className="bg-gray-900 rounded-lg p-8 text-center">
          <div className="animate-pulse text-violet-400 text-lg">
            Running NSGA-II optimization...
          </div>
          <p className="text-sm text-gray-500 mt-2">
            Pop: {populationSize}, Gen: {generations} - This may take a while
          </p>
        </div>
      )}
    </div>
  );
}
