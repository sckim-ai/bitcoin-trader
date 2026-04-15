import { useEffect } from "react";
import { useMarketDataStore } from "../stores/marketDataStore";
import CandlestickChart from "../components/charts/CandlestickChart";
import { Card, CardContent } from "../components/ui/Card";
import { MetricCard } from "../components/ui/MetricCard";

const MARKETS = ["BTC", "ETH"];
const TIMEFRAMES = ["hour", "day", "week"];

export default function DataLoadPage() {
  const { candles, market, timeframe, loading, error, setMarket, setTimeframe, loadCandles } =
    useMarketDataStore();

  useEffect(() => {
    loadCandles();
  }, [market, timeframe, loadCandles]);

  return (
    <div className="space-y-6 animate-fade-in">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-zinc-100">Market Data</h1>

        {/* Pill toggles */}
        <div className="flex items-center gap-3">
          <div className="flex bg-[#0c0c0f] border border-[#1e1e26] rounded-lg p-0.5">
            {MARKETS.map((m) => (
              <button
                key={m}
                onClick={() => setMarket(m)}
                className={`px-4 py-1.5 rounded-md text-xs font-semibold transition-all ${
                  market === m
                    ? "bg-amber-500 text-black shadow-sm"
                    : "text-zinc-500 hover:text-zinc-300"
                }`}
              >
                {m}
              </button>
            ))}
          </div>
          <div className="flex bg-[#0c0c0f] border border-[#1e1e26] rounded-lg p-0.5">
            {TIMEFRAMES.map((t) => (
              <button
                key={t}
                onClick={() => setTimeframe(t)}
                className={`px-3.5 py-1.5 rounded-md text-xs font-semibold transition-all capitalize ${
                  timeframe === t
                    ? "bg-amber-500 text-black shadow-sm"
                    : "text-zinc-500 hover:text-zinc-300"
                }`}
              >
                {t}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Status */}
      {loading && (
        <div className="flex items-center gap-2 text-zinc-500 text-sm">
          <div className="w-4 h-4 border-2 border-amber-500/30 border-t-amber-500 rounded-full animate-spin" />
          Loading data...
        </div>
      )}
      {error && (
        <div className="bg-rose-500/10 border border-rose-500/20 rounded-xl px-4 py-3 text-rose-400 text-sm">
          {error}
        </div>
      )}

      {/* Chart */}
      {candles.length > 0 && (
        <Card>
          <CardContent className="p-2">
            <CandlestickChart candles={candles} />
          </CardContent>
        </Card>
      )}

      {/* Summary cards */}
      {candles.length > 0 && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <MetricCard label="Candles" value={candles.length.toLocaleString()} color="amber" />
          <MetricCard
            label="Latest Close"
            value={candles[candles.length - 1].close.toLocaleString()}
            color="neutral"
          />
          <MetricCard
            label="Latest Volume"
            value={candles[candles.length - 1].volume.toLocaleString()}
            color="neutral"
          />
          <MetricCard
            label="Date Range"
            value={`${new Date(candles[0].timestamp).toLocaleDateString()} ~ ${new Date(candles[candles.length - 1].timestamp).toLocaleDateString()}`}
            color="neutral"
          />
        </div>
      )}
    </div>
  );
}
