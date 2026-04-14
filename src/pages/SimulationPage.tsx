import { useEffect } from "react";
import { useSimulationStore } from "../stores/simulationStore";
import { useMarketDataStore } from "../stores/marketDataStore";
import PerformanceChart from "../components/charts/PerformanceChart";

export default function SimulationPage() {
  const { strategies, selectedStrategy, result, loading, error, setSelectedStrategy, fetchStrategies, runSimulation } =
    useSimulationStore();
  const { market, timeframe } = useMarketDataStore();

  useEffect(() => {
    fetchStrategies();
  }, [fetchStrategies]);

  const handleRun = () => {
    runSimulation(market, timeframe);
  };

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Simulation</h1>

      {/* Controls */}
      <div className="flex gap-4 items-end">
        <div>
          <label className="block text-sm text-gray-400 mb-1">Strategy</label>
          <select
            value={selectedStrategy}
            onChange={(e) => setSelectedStrategy(e.target.value)}
            className="bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
          >
            {strategies.map((s) => (
              <option key={s.key} value={s.key}>
                {s.key} - {s.name}
              </option>
            ))}
          </select>
        </div>
        <div className="text-sm text-gray-400">
          {market} / {timeframe}
        </div>
        <button
          onClick={handleRun}
          disabled={loading}
          className="bg-violet-600 hover:bg-violet-700 disabled:bg-gray-700 text-white px-6 py-2 rounded font-medium transition-colors"
        >
          {loading ? "Running..." : "Run Simulation"}
        </button>
      </div>

      {error && <p className="text-red-400">{error}</p>}

      {/* Results */}
      {result && (
        <>
          {/* Metric cards */}
          <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
            <MetricCard label="Total Return" value={`${result.total_return.toFixed(2)}%`} positive={result.total_return > 0} />
            <MetricCard label="Win Rate" value={`${result.win_rate.toFixed(1)}%`} positive={result.win_rate > 50} />
            <MetricCard label="Max Drawdown" value={`${result.max_drawdown.toFixed(2)}%`} positive={false} />
            <MetricCard label="Trades" value={String(result.total_trades)} />
            <MetricCard label="Sharpe" value={result.sharpe_ratio.toFixed(3)} positive={result.sharpe_ratio > 0} />
          </div>

          {/* Equity curve */}
          {result.trades.length > 0 && (
            <div className="bg-gray-900 rounded-lg p-4">
              <PerformanceChart trades={result.trades} />
            </div>
          )}

          {/* Trade table */}
          {result.trades.length > 0 && (
            <div className="bg-gray-900 rounded-lg p-4 overflow-x-auto">
              <h2 className="text-lg font-semibold mb-3">Trade History</h2>
              <table className="w-full text-sm">
                <thead>
                  <tr className="text-gray-400 border-b border-gray-800">
                    <th className="text-left py-2 px-2">#</th>
                    <th className="text-left py-2 px-2">Buy Idx</th>
                    <th className="text-left py-2 px-2">Sell Idx</th>
                    <th className="text-right py-2 px-2">Buy Price</th>
                    <th className="text-right py-2 px-2">Sell Price</th>
                    <th className="text-right py-2 px-2">PnL %</th>
                    <th className="text-right py-2 px-2">Hold Bars</th>
                    <th className="text-left py-2 px-2">Signal</th>
                  </tr>
                </thead>
                <tbody>
                  {result.trades.map((t, i) => (
                    <tr key={i} className="border-b border-gray-800/50 hover:bg-gray-800/50">
                      <td className="py-2 px-2">{i + 1}</td>
                      <td className="py-2 px-2">{t.buy_index}</td>
                      <td className="py-2 px-2">{t.sell_index}</td>
                      <td className="text-right py-2 px-2">{t.buy_price.toLocaleString()}</td>
                      <td className="text-right py-2 px-2">{t.sell_price.toLocaleString()}</td>
                      <td className={`text-right py-2 px-2 ${t.pnl_pct >= 0 ? "text-green-400" : "text-red-400"}`}>
                        {(t.pnl_pct * 100).toFixed(2)}%
                      </td>
                      <td className="text-right py-2 px-2">{t.hold_bars}</td>
                      <td className="py-2 px-2 text-gray-400">{t.sell_signal}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </div>
  );
}

function MetricCard({ label, value, positive }: { label: string; value: string; positive?: boolean }) {
  const colorClass = positive === undefined ? "text-white" : positive ? "text-green-400" : "text-red-400";
  return (
    <div className="bg-gray-900 rounded-lg p-4">
      <p className="text-sm text-gray-400">{label}</p>
      <p className={`text-xl font-bold ${colorClass}`}>{value}</p>
    </div>
  );
}
