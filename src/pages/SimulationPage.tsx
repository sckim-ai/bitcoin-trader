import { useEffect } from "react";
import { useSimulationStore } from "../stores/simulationStore";
import { useMarketDataStore } from "../stores/marketDataStore";
import PerformanceChart from "../components/charts/PerformanceChart";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { MetricCard } from "../components/ui/MetricCard";
import { Button } from "../components/ui/Button";
import { Select } from "../components/ui/Select";
import { Badge } from "../components/ui/Badge";
import { Play } from "lucide-react";

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
    <div className="space-y-6 animate-fade-in">
      {/* Controls */}
      <div className="flex items-end gap-4">
        <div className="w-64">
          <Select
            label="Strategy"
            value={selectedStrategy}
            onChange={(e) => setSelectedStrategy(e.target.value)}
            options={strategies.map((s) => ({ value: s.key, label: `${s.key} - ${s.name}` }))}
          />
        </div>
        <Badge variant="blue" className="mb-1">{market} / {timeframe}</Badge>
        <Button onClick={handleRun} disabled={loading}>
          {loading ? (
            <>
              <div className="w-4 h-4 border-2 border-black/30 border-t-black rounded-full animate-spin" />
              Running...
            </>
          ) : (
            <>
              <Play size={16} />
              Run Simulation
            </>
          )}
        </Button>
      </div>

      {error && (
        <div className="bg-rose-500/10 border border-rose-500/20 rounded-xl px-4 py-3 text-rose-400 text-sm">
          {error}
        </div>
      )}

      {/* Results */}
      {result && (
        <>
          {/* Metric cards */}
          <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
            <MetricCard
              label="Total Return"
              value={`${result.total_return.toFixed(2)}%`}
              color={result.total_return > 0 ? "green" : "red"}
              trend={result.total_return > 0 ? "up" : "down"}
            />
            <MetricCard
              label="Win Rate"
              value={`${result.win_rate.toFixed(1)}%`}
              color={result.win_rate > 50 ? "green" : "red"}
            />
            <MetricCard
              label="Max Drawdown"
              value={`${result.max_drawdown.toFixed(2)}%`}
              color="red"
            />
            <MetricCard
              label="Trades"
              value={String(result.total_trades)}
              color="neutral"
            />
            <MetricCard
              label="Sharpe Ratio"
              value={result.sharpe_ratio.toFixed(3)}
              color={result.sharpe_ratio > 0 ? "green" : "red"}
            />
          </div>

          {/* Equity curve */}
          {result.trades.length > 0 && (
            <Card>
              <CardHeader>
                <h2 className="text-sm font-semibold text-zinc-300">Equity Curve</h2>
              </CardHeader>
              <CardContent className="p-2">
                <PerformanceChart trades={result.trades} />
              </CardContent>
            </Card>
          )}

          {/* Trade table */}
          {result.trades.length > 0 && (
            <Card>
              <CardHeader>
                <h2 className="text-sm font-semibold text-zinc-300">Trade History</h2>
              </CardHeader>
              <CardContent className="p-0">
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b border-[#1e1e26]">
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">#</th>
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Buy Idx</th>
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Sell Idx</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Buy Price</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Sell Price</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">PnL %</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Hold</th>
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Signal</th>
                      </tr>
                    </thead>
                    <tbody>
                      {result.trades.map((t, i) => (
                        <tr key={i} className="border-b border-[#1e1e26]/50 hover:bg-[#141419] transition-colors">
                          <td className="py-2.5 px-3 text-zinc-500">{i + 1}</td>
                          <td className="py-2.5 px-3 font-data text-zinc-300">{t.buy_index}</td>
                          <td className="py-2.5 px-3 font-data text-zinc-300">{t.sell_index}</td>
                          <td className="text-right py-2.5 px-3 font-data text-zinc-300">{t.buy_price.toLocaleString()}</td>
                          <td className="text-right py-2.5 px-3 font-data text-zinc-300">{t.sell_price.toLocaleString()}</td>
                          <td className="text-right py-2.5 px-3">
                            <span
                              className={`inline-block px-2 py-0.5 rounded-md text-xs font-semibold font-data ${
                                t.pnl_pct >= 0
                                  ? "bg-emerald-500/15 text-emerald-400"
                                  : "bg-rose-500/15 text-rose-400"
                              }`}
                            >
                              {(t.pnl_pct * 100).toFixed(2)}%
                            </span>
                          </td>
                          <td className="text-right py-2.5 px-3 font-data text-zinc-400">{t.hold_bars}</td>
                          <td className="py-2.5 px-3 text-zinc-500 text-xs">{t.sell_signal}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </CardContent>
            </Card>
          )}
        </>
      )}
    </div>
  );
}
