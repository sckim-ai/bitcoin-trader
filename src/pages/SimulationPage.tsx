import { useEffect, useMemo } from "react";
import { useSimulationStore } from "../stores/simulationStore";
import PerformanceChart from "../components/charts/PerformanceChart";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { MetricCard } from "../components/ui/MetricCard";
import { Button } from "../components/ui/Button";
import { Select } from "../components/ui/Select";
import { Input } from "../components/ui/Input";
import { NumberInput } from "../components/ui/NumberInput";
import { Play, RotateCcw } from "lucide-react";
import type { ParameterRange } from "../types";

const MARKET_OPTIONS = [
  { value: "BTC", label: "BTC" },
  { value: "ETH", label: "ETH" },
];

const TIMEFRAME_OPTIONS = [
  { value: "hour", label: "Hour" },
  { value: "day", label: "Day" },
  { value: "week", label: "Week" },
];

function classifySection(name: string): string {
  const versioned = name.match(/^v(\d+)_(buy|sell)?/);
  if (versioned) {
    const version = `V${versioned[1]}`;
    if (versioned[2] === "buy") return `${version} · Buy`;
    if (versioned[2] === "sell") return `${version} · Sell`;
    return `${version} · Misc`;
  }
  if (name.startsWith("urgent_buy") || name.startsWith("buy_")) return "Buy";
  if (name.startsWith("urgent_sell") || name.startsWith("sell_")) return "Sell";
  return "Risk";
}

function groupBySection(ranges: ParameterRange[]): Record<string, ParameterRange[]> {
  const out: Record<string, ParameterRange[]> = {};
  for (const r of ranges) {
    const section = classifySection(r.name);
    (out[section] ??= []).push(r);
  }
  return out;
}

export default function SimulationPage() {
  const {
    strategies,
    selectedStrategy,
    market,
    timeframe,
    since,
    until,
    dataRange,
    params,
    result,
    loading,
    error,
    setSelectedStrategy,
    setMarket,
    setTimeframe,
    setSince,
    setUntil,
    setParam,
    resetParams,
    fetchStrategies,
    runSimulation,
  } = useSimulationStore();

  useEffect(() => {
    fetchStrategies();
  }, [fetchStrategies]);

  const currentStrategy = strategies.find((s) => s.key === selectedStrategy);
  const sections = useMemo(
    () => (currentStrategy ? groupBySection(currentStrategy.ranges) : {}),
    [currentStrategy]
  );

  return (
    <div className="space-y-6 animate-fade-in">
      {/* Controls */}
      <div className="grid grid-cols-2 md:grid-cols-6 gap-3 items-end">
        <div className="md:col-span-2">
          <Select
            label="Strategy"
            value={selectedStrategy}
            onChange={(e) => {
              void setSelectedStrategy(e.target.value);
            }}
            options={strategies.map((s) => ({ value: s.key, label: `${s.key} - ${s.name}` }))}
          />
        </div>
        <Select
          label="Market"
          value={market}
          onChange={(e) => setMarket(e.target.value)}
          options={MARKET_OPTIONS}
        />
        <Select
          label="Timeframe"
          value={timeframe}
          onChange={(e) => setTimeframe(e.target.value)}
          options={TIMEFRAME_OPTIONS}
        />
        <Input
          label="Since"
          type="date"
          value={since}
          onChange={(e) => setSince(e.target.value)}
        />
        <Input
          label="Until"
          type="date"
          value={until}
          onChange={(e) => setUntil(e.target.value)}
        />
      </div>

      <div className="flex items-center gap-3 flex-wrap">
        <Button onClick={runSimulation} disabled={loading}>
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
        {dataRange && dataRange.count > 0 && (
          <span className="text-xs text-zinc-500">
            Data: {dataRange.count.toLocaleString()} bars · {since || formatTs(dataRange.min_timestamp, true)} → {until || formatTs(dataRange.max_timestamp, true)}
          </span>
        )}
        {selectedStrategy === "V3" && market === "ETH" && timeframe === "hour" &&
          since === "2025-01-01" && until === "2026-03-31" && (
          <span className="inline-block px-2 py-0.5 rounded-md text-[11px] font-semibold bg-emerald-500/15 text-emerald-400">
            Legacy V3 baseline · expected ≈ 278.64%
          </span>
        )}
      </div>

      {error && (
        <div className="bg-rose-500/10 border border-rose-500/20 rounded-xl px-4 py-3 text-rose-400 text-sm">
          {error}
        </div>
      )}

      {/* Parameters */}
      {currentStrategy && currentStrategy.ranges.length > 0 && (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-semibold text-zinc-300">
                Parameters — {currentStrategy.name}
              </h2>
              <Button onClick={resetParams} variant="secondary" size="sm">
                <RotateCcw size={12} />
                Reset
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-6">
            {Object.entries(sections).map(([section, items]) => (
              <div key={section}>
                <div className="text-[11px] font-semibold uppercase tracking-wider text-zinc-500 mb-3">
                  {section}
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                  {items.map((r) => (
                    <NumberInput
                      key={r.name}
                      label={`${r.name}  (${r.min} ~ ${r.max})`}
                      min={r.min}
                      max={r.max}
                      step={r.step}
                      value={params[r.name] ?? r.min}
                      onValueChange={(v) => setParam(r.name, v)}
                    />
                  ))}
                </div>
              </div>
            ))}
          </CardContent>
        </Card>
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
              label="Market Return"
              value={`${result.market_return.toFixed(2)}%`}
              color={result.market_return > 0 ? "green" : "red"}
            />
            <MetricCard
              label="Win Rate"
              value={`${result.win_rate.toFixed(1)}%`}
              color={result.win_rate > 50 ? "green" : "red"}
            />
            <MetricCard
              label="Profit Factor"
              value={result.profit_factor.toFixed(2)}
              color={result.profit_factor >= 1 ? "green" : "red"}
            />
            <MetricCard
              label="Max Drawdown"
              value={`${result.max_drawdown.toFixed(2)}%`}
              color="red"
            />
            <MetricCard
              label="Sharpe Ratio"
              value={result.sharpe_ratio.toFixed(3)}
              color={result.sharpe_ratio > 0 ? "green" : "red"}
            />
            <MetricCard
              label="Sortino Ratio"
              value={result.sortino_ratio.toFixed(3)}
              color={result.sortino_ratio > 0 ? "green" : "red"}
            />
            <MetricCard
              label="Annual Return"
              value={`${result.annual_return.toFixed(2)}%`}
              color={result.annual_return > 0 ? "green" : "red"}
            />
            <MetricCard
              label="Trades"
              value={String(result.total_trades)}
              color="neutral"
            />
            <MetricCard
              label="Max Consec. Losses"
              value={String(result.max_consecutive_losses)}
              color={result.max_consecutive_losses > 5 ? "red" : "neutral"}
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
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Buy Time</th>
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Sell Time</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Buy Price</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Sell Price</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">PnL %</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Hold</th>
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Buy Signal</th>
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Sell Signal</th>
                      </tr>
                    </thead>
                    <tbody>
                      {result.trades.map((t, i) => (
                        <tr key={i} className="border-b border-[#1e1e26]/50 hover:bg-[#141419] transition-colors">
                          <td className="py-2.5 px-3 text-zinc-500">{i + 1}</td>
                          <td className="py-2.5 px-3 font-data text-zinc-300 text-xs whitespace-nowrap">{formatTs(t.buy_timestamp)}</td>
                          <td className="py-2.5 px-3 font-data text-zinc-300 text-xs whitespace-nowrap">{formatTs(t.sell_timestamp)}</td>
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
                          <td className="py-2.5 px-3 text-sky-400 text-xs">{t.buy_signal || "—"}</td>
                          <td className="py-2.5 px-3 text-amber-400 text-xs">{t.sell_signal || "—"}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </CardContent>
            </Card>
          )}

          {/* Signal timeline */}
          {result.signal_log && result.signal_log.length > 0 && (
            <Card>
              <CardHeader>
                <h2 className="text-sm font-semibold text-zinc-300">
                  Signal Timeline{" "}
                  <span className="text-xs font-normal text-zinc-500">
                    ({result.signal_log.length} state transitions)
                  </span>
                </h2>
              </CardHeader>
              <CardContent className="p-0">
                <div className="overflow-x-auto max-h-[400px] overflow-y-auto">
                  <table className="w-full text-sm">
                    <thead className="sticky top-0 bg-[#0c0c0f]">
                      <tr className="border-b border-[#1e1e26]">
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Bar</th>
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Timestamp</th>
                        <th className="text-left py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Signal</th>
                        <th className="text-right py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Price</th>
                        <th className="text-center py-2.5 px-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Pos</th>
                      </tr>
                    </thead>
                    <tbody>
                      {result.signal_log.map((s, i) => (
                        <tr key={i} className="border-b border-[#1e1e26]/50 hover:bg-[#141419] transition-colors">
                          <td className="py-2 px-3 font-data text-zinc-500">{s.index}</td>
                          <td className="py-2 px-3 font-data text-zinc-400 text-xs">{s.timestamp.slice(0, 16).replace("T", " ")}</td>
                          <td className="py-2 px-3">
                            <span className={`inline-block px-2 py-0.5 rounded-md text-xs font-semibold ${signalBadgeClass(s.signal_type)}`}>
                              {s.signal_type}
                            </span>
                          </td>
                          <td className="text-right py-2 px-3 font-data text-zinc-300">{s.price.toLocaleString()}</td>
                          <td className="text-center py-2 px-3 font-data text-zinc-400">{s.position}</td>
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

function formatTs(ts: string | undefined | null, dateOnly = false): string {
  if (!ts) return "—";
  const s = ts.replace("T", " ");
  return dateOnly ? s.slice(0, 10) : s.slice(0, 16);
}

function signalBadgeClass(signal: string): string {
  switch (signal) {
    case "buy":
      return "bg-sky-500/20 text-sky-300";
    case "sell":
      return "bg-amber-500/20 text-amber-300";
    case "buy ready":
      return "bg-sky-500/10 text-sky-400";
    case "sell ready":
      return "bg-amber-500/10 text-amber-400";
    case "hold":
      return "bg-emerald-500/10 text-emerald-400";
    default:
      return "bg-zinc-700/30 text-zinc-400";
  }
}
