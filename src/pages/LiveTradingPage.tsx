import { useState } from "react";
import { useTradingStore } from "../stores/tradingStore";
import PositionCard from "../components/trading/PositionCard";
import ManualOrderDialog from "../components/trading/ManualOrderDialog";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Activity, Radio, RadioOff } from "lucide-react";

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
    <div className="space-y-4 animate-fade-in">
      {/* Top bar: Price + Balance */}
      <div className="flex items-stretch gap-4">
        {/* Current Price - hero element */}
        <Card className="flex-[2]">
          <CardContent className="flex items-center justify-between">
            <div>
              <p className="text-xs font-medium text-zinc-500 mb-1">{MARKET}</p>
              <p className="text-[32px] font-bold text-zinc-100 font-data leading-tight">
                {currentPrice > 0 ? currentPrice.toLocaleString() : "--"}
                <span className="text-sm ml-2 text-zinc-600 font-sans font-normal">KRW</span>
              </p>
            </div>
            {priceChange !== 0 && (
              <span
                className={`inline-flex items-center px-3 py-1.5 rounded-lg text-sm font-semibold font-data ${
                  priceChange >= 0
                    ? "bg-emerald-500/15 text-emerald-400"
                    : "bg-rose-500/15 text-rose-400"
                }`}
              >
                {priceChange >= 0 ? "+" : ""}
                {priceChange.toFixed(2)}%
              </span>
            )}
          </CardContent>
        </Card>

        {/* Balances */}
        <Card className="flex-1">
          <CardContent>
            <p className="text-xs font-medium text-zinc-500 mb-1">KRW Balance</p>
            <p className="text-xl font-semibold text-zinc-100 font-data">
              {balanceKrw > 0 ? balanceKrw.toLocaleString() : "--"}
            </p>
          </CardContent>
        </Card>
        <Card className="flex-1">
          <CardContent>
            <p className="text-xs font-medium text-zinc-500 mb-1">BTC Balance</p>
            <p className="text-xl font-semibold text-zinc-100 font-data">
              {balanceCoin > 0 ? balanceCoin.toFixed(8) : "--"}
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Position + Controls */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <PositionCard position={position} currentPrice={currentPrice} />

        <Card>
          <CardHeader className="flex items-center gap-2">
            <Activity size={16} className="text-zinc-500" />
            <h3 className="text-sm font-semibold text-zinc-300">Controls</h3>
          </CardHeader>
          <CardContent className="space-y-3">
            {!isMonitoring ? (
              <Button onClick={() => startMonitoring(MARKET)} className="w-full" size="lg">
                <Radio size={16} />
                Start Monitoring
              </Button>
            ) : (
              <Button onClick={stopMonitoring} variant="secondary" className="w-full" size="lg">
                <RadioOff size={16} />
                Stop Monitoring
              </Button>
            )}
            <div className="flex gap-3">
              <Button
                onClick={() => setOrderDialog("buy")}
                variant="success"
                className="flex-1"
                size="lg"
              >
                Buy
              </Button>
              <Button
                onClick={() => setOrderDialog("sell")}
                variant="danger"
                className="flex-1"
                size="lg"
              >
                Sell
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Log area */}
      <Card>
        <CardHeader>
          <h3 className="text-sm font-semibold text-zinc-300">Logs</h3>
        </CardHeader>
        <CardContent className="p-0">
          <div className="h-52 overflow-y-auto px-4 py-3 font-data text-xs space-y-0.5">
            {logs.length === 0 ? (
              <p className="text-zinc-700">No logs yet. Start monitoring to begin.</p>
            ) : (
              logs.map((log, i) => (
                <p
                  key={i}
                  className={
                    log.includes("[ERROR]")
                      ? "text-rose-400"
                      : log.includes("[SUCCESS]")
                      ? "text-emerald-400"
                      : "text-zinc-500"
                  }
                >
                  {log}
                </p>
              ))
            )}
          </div>
        </CardContent>
      </Card>

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
