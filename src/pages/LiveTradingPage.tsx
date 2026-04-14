import { useState } from "react";
import { useTradingStore } from "../stores/tradingStore";
import PositionCard from "../components/trading/PositionCard";
import ManualOrderDialog from "../components/trading/ManualOrderDialog";

const MARKET = "KRW-BTC";

export default function LiveTradingPage() {
  const {
    currentPrice,
    priceChange,
    balanceKrw,
    balanceCoin,
    position,
    isMonitoring,
    logs,
    startMonitoring,
    stopMonitoring,
    addLog,
    fetchBalance,
    fetchPosition,
  } = useTradingStore();

  const [orderDialog, setOrderDialog] = useState<"buy" | "sell" | null>(null);

  const handleOrderSuccess = (msg: string) => {
    addLog(`[SUCCESS] ${msg}`);
    fetchBalance();
    fetchPosition(MARKET);
  };

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Live Trading</h1>

      {/* Top bar: Price + Balance */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="bg-gray-900 rounded-lg p-4 md:col-span-2">
          <p className="text-sm text-gray-400">Current Price ({MARKET})</p>
          <p className="text-3xl font-bold text-white font-mono">
            {currentPrice > 0 ? currentPrice.toLocaleString() : "--"}
            <span className="text-sm ml-2 text-gray-500">KRW</span>
          </p>
          {priceChange !== 0 && (
            <p
              className={`text-sm font-mono ${
                priceChange >= 0 ? "text-green-400" : "text-red-400"
              }`}
            >
              {priceChange >= 0 ? "+" : ""}
              {priceChange.toFixed(2)}%
            </p>
          )}
        </div>
        <div className="bg-gray-900 rounded-lg p-4">
          <p className="text-sm text-gray-400">KRW Balance</p>
          <p className="text-xl font-bold text-white font-mono">
            {balanceKrw > 0 ? balanceKrw.toLocaleString() : "--"}
          </p>
        </div>
        <div className="bg-gray-900 rounded-lg p-4">
          <p className="text-sm text-gray-400">BTC Balance</p>
          <p className="text-xl font-bold text-white font-mono">
            {balanceCoin > 0 ? balanceCoin.toFixed(8) : "--"}
          </p>
        </div>
      </div>

      {/* Position + Controls */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <PositionCard position={position} currentPrice={currentPrice} />

        <div className="bg-gray-900 rounded-lg p-4 space-y-3">
          <h3 className="text-sm text-gray-400 mb-3">Controls</h3>
          <div className="flex gap-3">
            {!isMonitoring ? (
              <button
                onClick={() => startMonitoring(MARKET)}
                className="flex-1 py-2 bg-violet-600 hover:bg-violet-700 text-white rounded font-medium transition-colors"
              >
                Start Monitoring
              </button>
            ) : (
              <button
                onClick={stopMonitoring}
                className="flex-1 py-2 bg-gray-700 hover:bg-gray-600 text-white rounded font-medium transition-colors"
              >
                Stop Monitoring
              </button>
            )}
          </div>
          <div className="flex gap-3">
            <button
              onClick={() => setOrderDialog("buy")}
              className="flex-1 py-2 bg-green-600 hover:bg-green-700 text-white rounded font-medium transition-colors"
            >
              Buy
            </button>
            <button
              onClick={() => setOrderDialog("sell")}
              className="flex-1 py-2 bg-red-600 hover:bg-red-700 text-white rounded font-medium transition-colors"
            >
              Sell
            </button>
          </div>
        </div>
      </div>

      {/* Log area */}
      <div className="bg-gray-900 rounded-lg p-4">
        <h3 className="text-sm text-gray-400 mb-2">Logs</h3>
        <div className="h-48 overflow-y-auto bg-gray-950 rounded p-2 font-mono text-xs space-y-0.5">
          {logs.length === 0 ? (
            <p className="text-gray-600">No logs yet. Start monitoring to begin.</p>
          ) : (
            logs.map((log, i) => (
              <p
                key={i}
                className={
                  log.includes("[ERROR]")
                    ? "text-red-400"
                    : log.includes("[SUCCESS]")
                    ? "text-green-400"
                    : "text-gray-400"
                }
              >
                {log}
              </p>
            ))
          )}
        </div>
      </div>

      {/* Order dialog */}
      {orderDialog && (
        <ManualOrderDialog
          side={orderDialog}
          market={MARKET}
          currentPrice={currentPrice}
          onClose={() => setOrderDialog(null)}
          onSuccess={handleOrderSuccess}
          onError={(msg) => addLog(`[ERROR] ${msg}`)}
        />
      )}
    </div>
  );
}
