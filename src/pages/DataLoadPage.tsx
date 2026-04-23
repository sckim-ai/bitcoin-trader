import { useEffect, useState } from "react";
import { useMarketDataStore } from "../stores/marketDataStore";
import CandlestickChart from "../components/charts/CandlestickChart";
import { Card, CardContent } from "../components/ui/Card";
import { MetricCard } from "../components/ui/MetricCard";
import { getCurrentPrice } from "../lib/api";

const MARKETS = ["BTC", "ETH"];
const TIMEFRAMES = ["hour", "day", "week"];
const LIMITS = [100, 500, 1000, 5000];
const LIVE_POLL_MS = 2000;

export default function DataLoadPage() {
  const { candles, market, timeframe, limit, loading, error, setMarket, setTimeframe, setLimit, loadCandles, refreshCandles } =
    useMarketDataStore();
  const [livePrice, setLivePrice] = useState<number | null>(null);

  useEffect(() => {
    loadCandles();
  }, [market, timeframe, limit, loadCandles]);

  // Periodic refresh so corrected high/low from background UPSERT reaches chart.
  useEffect(() => {
    const id = setInterval(() => refreshCandles(), 30_000);
    return () => clearInterval(id);
  }, [refreshCandles]);

  // Poll live ticker every 2s; chart updates last bar with this price.
  useEffect(() => {
    setLivePrice(null);
    const apiMarket = `KRW-${market}`;
    let cancelled = false;

    const tick = async () => {
      try {
        const p = await getCurrentPrice(apiMarket);
        if (!cancelled) setLivePrice(p);
      } catch { /* ignore transient errors */ }
    };
    tick();
    const id = setInterval(tick, LIVE_POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [market]);

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
          <select
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            className="bg-[#0c0c0f] border border-[#1e1e26] rounded-lg px-3 py-1.5 text-xs font-semibold text-zinc-300"
          >
            {LIMITS.map((n) => (
              <option key={n} value={n}>{n} bars</option>
            ))}
          </select>
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
            <CandlestickChart candles={candles} timeframe={timeframe} livePrice={livePrice} />
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
