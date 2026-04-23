import { useEffect, useState } from "react";
import { useTradingStore } from "../stores/tradingStore";
import PositionCard from "../components/trading/PositionCard";
import ManualOrderDialog from "../components/trading/ManualOrderDialog";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import {
  Activity,
  Radio,
  RadioOff,
  Bot,
  Square,
  RefreshCw,
} from "lucide-react";
import { listStrategies, autoUpdateAllMarkets } from "../lib/api";
import type { StrategyInfo, AutoTradeLog, AutoTradeEvent } from "../types";

const MARKET = "KRW-BTC";

export default function LiveTradingPage() {
  const {
    currentPrice,
    priceChange,
    balanceKrw,
    balanceCoin,
    position,
    isMonitoring,
    isAutoTrading,
    autoTradingStatus,
    logs,
    startMonitoring,
    stopMonitoring,
    startAutoTrading,
    stopAutoTrading,
    fetchAutoTradingStatus,
    addLog,
    fetchBalance,
    fetchPosition,
  } = useTradingStore();

  const [orderDialog, setOrderDialog] = useState<"buy" | "sell" | null>(null);
  const [strategies, setStrategies] = useState<StrategyInfo[]>([]);
  const [selectedStrategy, setSelectedStrategy] = useState("V3");
  const [isUpdating, setIsUpdating] = useState(false);

  // Load strategies on mount
  useEffect(() => {
    listStrategies()
      .then((list) => {
        setStrategies(list);
        if (list.length > 0) setSelectedStrategy(list[0].key);
      })
      .catch(() => {});
    fetchAutoTradingStatus();
  }, []);

  // Listen to Tauri events for auto-trading
  useEffect(() => {
    if (!("__TAURI__" in window)) return;

    let unlisten: (() => void)[] = [];

    (async () => {
      const { listen } = await import("@tauri-apps/api/event");

      const u1 = await listen<AutoTradeLog>("auto-trade:log", (e) => {
        const log = e.payload;
        addLog(`[${log.level}] ${log.message}`);
      });
      const u2 = await listen<AutoTradeEvent>("auto-trade:trade", (e) => {
        const t = e.payload;
        addLog(
          `[TRADE] ${t.side.toUpperCase()} ${t.market} @ ${t.price.toLocaleString()} (${t.signal})`
        );
        fetchBalance();
        fetchPosition(MARKET);
      });
      const u3 = await listen("auto-trade:position", () => {
        fetchPosition(MARKET);
      });
      const u4 = await listen("auto-trade:status", () => {
        fetchAutoTradingStatus();
      });

      unlisten = [u1, u2, u3, u4];
    })();

    return () => {
      unlisten.forEach((fn) => fn());
    };
  }, []);

  const handleOrderSuccess = (msg: string) => {
    addLog(`[SUCCESS] ${msg}`);
    fetchBalance();
    fetchPosition(MARKET);
  };

  const handleDataUpdate = async () => {
    setIsUpdating(true);
    try {
      const results = await autoUpdateAllMarkets();
      const total = results.reduce((sum, r) => sum + r.new_candles, 0);
      addLog(`[SUCCESS] Data updated: ${total} new candles across ${results.length} markets`);
    } catch (e) {
      addLog(`[ERROR] Data update failed: ${e}`);
    }
    setIsUpdating(false);
  };

  return (
    <div className="space-y-4 animate-fade-in">
      {/* Top bar: Price + Balance */}
      <div className="flex items-stretch gap-4">
        <Card className="flex-[2]">
          <CardContent className="flex items-center justify-between">
            <div>
              <p className="text-xs font-medium text-zinc-500 mb-1">{MARKET}</p>
              <p className="text-[32px] font-bold text-zinc-100 font-data leading-tight">
                {currentPrice > 0 ? currentPrice.toLocaleString() : "--"}
                <span className="text-sm ml-2 text-zinc-600 font-sans font-normal">
                  KRW
                </span>
              </p>
            </div>
            <div className="flex items-center gap-3">
              {isAutoTrading && (
                <Badge variant="green">
                  <Bot size={12} className="mr-1" />
                  Auto
                </Badge>
              )}
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
            </div>
          </CardContent>
        </Card>

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
            {/* Monitoring */}
            {!isMonitoring ? (
              <Button
                onClick={() => startMonitoring(MARKET)}
                className="w-full"
                size="lg"
              >
                <Radio size={16} />
                Start Monitoring
              </Button>
            ) : (
              <Button
                onClick={stopMonitoring}
                variant="secondary"
                className="w-full"
                size="lg"
              >
                <RadioOff size={16} />
                Stop Monitoring
              </Button>
            )}

            {/* Auto-trading */}
            <div className="flex gap-2 items-center">
              <select
                value={selectedStrategy}
                onChange={(e) => setSelectedStrategy(e.target.value)}
                disabled={isAutoTrading}
                className="flex-1 bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-200 disabled:opacity-50"
              >
                {strategies.map((s) => (
                  <option key={s.key} value={s.key}>
                    {s.key}: {s.name}
                  </option>
                ))}
              </select>
              {!isAutoTrading ? (
                <Button
                  onClick={() => startAutoTrading(MARKET, selectedStrategy)}
                  variant="success"
                  size="lg"
                >
                  <Bot size={16} />
                  Auto Start
                </Button>
              ) : (
                <Button onClick={stopAutoTrading} variant="danger" size="lg">
                  <Square size={16} />
                  Auto Stop
                </Button>
              )}
            </div>

            {/* Auto-trading status */}
            {autoTradingStatus?.running && (
              <div className="text-xs text-zinc-500 bg-zinc-800/50 rounded-lg px-3 py-2">
                <span className="text-emerald-400">●</span>{" "}
                {autoTradingStatus.strategy} strategy active
                {autoTradingStatus.last_signal && (
                  <span className="ml-2 text-zinc-400">
                    Last: {autoTradingStatus.last_signal}
                  </span>
                )}
                {autoTradingStatus.last_check && (
                  <span className="ml-2 text-zinc-600">
                    @ {autoTradingStatus.last_check}
                  </span>
                )}
              </div>
            )}

            {/* Manual orders + Data update */}
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
              <Button
                onClick={handleDataUpdate}
                variant="secondary"
                size="lg"
                disabled={isUpdating}
              >
                <RefreshCw
                  size={16}
                  className={isUpdating ? "animate-spin" : ""}
                />
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
              <p className="text-zinc-700">
                No logs yet. Start monitoring to begin.
              </p>
            ) : (
              logs.map((log, i) => (
                <p
                  key={i}
                  className={
                    log.includes("[ERROR]")
                      ? "text-rose-400"
                      : log.includes("[SUCCESS]") || log.includes("[TRADE]")
                      ? "text-emerald-400"
                      : log.includes("[WARN]")
                      ? "text-amber-400"
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
