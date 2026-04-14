import { useEffect } from "react";
import { useMarketDataStore } from "../stores/marketDataStore";
import CandlestickChart from "../components/charts/CandlestickChart";

const MARKETS = ["BTC", "ETH"];
const TIMEFRAMES = ["hour", "day", "week"];

export default function DataLoadPage() {
  const { candles, market, timeframe, loading, error, setMarket, setTimeframe, loadCandles } =
    useMarketDataStore();

  useEffect(() => {
    loadCandles();
  }, [market, timeframe, loadCandles]);

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Market Data</h1>

      {/* Selectors */}
      <div className="flex gap-4">
        <div>
          <label className="block text-sm text-gray-400 mb-1">Market</label>
          <select
            value={market}
            onChange={(e) => setMarket(e.target.value)}
            className="bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
          >
            {MARKETS.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="block text-sm text-gray-400 mb-1">Timeframe</label>
          <select
            value={timeframe}
            onChange={(e) => setTimeframe(e.target.value)}
            className="bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
          >
            {TIMEFRAMES.map((t) => (
              <option key={t} value={t}>{t}</option>
            ))}
          </select>
        </div>
      </div>

      {/* Status */}
      {loading && <p className="text-gray-400">Loading...</p>}
      {error && <p className="text-red-400">{error}</p>}

      {/* Chart */}
      {candles.length > 0 && (
        <div className="bg-gray-900 rounded-lg p-4">
          <CandlestickChart candles={candles} />
        </div>
      )}

      {/* Summary */}
      {candles.length > 0 && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <SummaryCard label="Candles" value={candles.length.toLocaleString()} />
          <SummaryCard label="Latest Close" value={candles[candles.length - 1].close.toLocaleString()} />
          <SummaryCard label="Latest Volume" value={candles[candles.length - 1].volume.toLocaleString()} />
          <SummaryCard
            label="Date Range"
            value={`${new Date(candles[0].timestamp).toLocaleDateString()} - ${new Date(candles[candles.length - 1].timestamp).toLocaleDateString()}`}
          />
        </div>
      )}
    </div>
  );
}

function SummaryCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-gray-900 rounded-lg p-4">
      <p className="text-sm text-gray-400">{label}</p>
      <p className="text-lg font-semibold text-white">{value}</p>
    </div>
  );
}
