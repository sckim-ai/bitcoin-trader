import { useState } from "react";
import { ShoppingCart, AlertTriangle } from "lucide-react";
import { Card, CardContent, CardHeader } from "../ui/Card";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import type { LiveSession } from "../../types";
import { manualMarketOrder, type ManualOrderResult } from "../../lib/api";

interface Props {
  /// All sessions — we filter for real ones internally.
  sessions: LiveSession[];
  /// Called after a successful order so the page can refresh trades / KPIs.
  onPlaced?: (result: ManualOrderResult) => void;
}

/// User-triggered market order. Distinct from auto-cycle orders
/// (signal="real_buy"/"real_sell") — this writes signal="manual_buy"/
/// "manual_sell" so history can separate "user clicked" from "strategy decided".
///
/// Multi-real=1 policy: real session typically 0 or 1. If 0, the card
/// disables itself (Upbit balance has no session to attribute to).
export default function ManualOrderCard({ sessions, onPlaced }: Props) {
  const realSessions = sessions.filter((s) => s.mode === "real");
  const hasReal = realSessions.length > 0;

  const [side, setSide] = useState<"buy" | "sell">("buy");
  const [amount, setAmount] = useState<string>("10000");
  const [submitting, setSubmitting] = useState(false);
  const [result, setResult] = useState<ManualOrderResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handlePlace = async () => {
    setError(null);
    setResult(null);

    const amt = Number(amount);
    if (!Number.isFinite(amt) || amt <= 0) {
      setError("금액(또는 수량)은 양수여야 합니다.");
      return;
    }

    // Sanity guard before confirm — Upbit minimum + reasonable manual cap.
    if (side === "buy") {
      if (amt < 5_000) { setError("최소 매수 5,000 KRW."); return; }
      if (amt > 1_000_000) {
        if (!window.confirm(`매수 금액이 1,000,000 KRW 를 초과합니다 (${amt.toLocaleString()}). 진행할까요?`)) return;
      }
    } else {
      if (amt < 0.0001) { setError("매도 수량이 너무 작습니다 (≥ 0.0001 ETH)."); return; }
    }

    const sessionLabel = hasReal ? `세션 #${realSessions[0].id} ${realSessions[0].label}` : "세션 미부착";
    const desc = side === "buy"
      ? `${amt.toLocaleString()} KRW 어치 ETH를 시장가 매수`
      : `${amt} ETH를 시장가 매도`;
    if (!window.confirm(`⚠️ 실제 자금 이동\n\n${desc}\n${sessionLabel}\n\n진행할까요?`)) return;

    setSubmitting(true);
    try {
      const r = await manualMarketOrder({
        market: "KRW-ETH",
        side,
        krw_amount: side === "buy" ? amt : undefined,
        volume: side === "sell" ? amt : undefined,
        session_id: hasReal ? realSessions[0].id : undefined,
      });
      setResult(r);
      onPlaced?.(r);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Card>
      <CardHeader className="flex items-center gap-2">
        <ShoppingCart size={16} className="text-amber-500" />
        <h3 className="text-sm font-semibold text-zinc-300">Manual Order (KRW-ETH)</h3>
        {hasReal ? (
          <span className="ml-2 text-[10px] text-zinc-500">
            ↳ session #{realSessions[0].id} {realSessions[0].label}
          </span>
        ) : (
          <span className="ml-2 text-[10px] text-zinc-600">
            ↳ no real session — fill won't attribute to history
          </span>
        )}
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-xs text-amber-400 flex items-start gap-1.5">
          <AlertTriangle size={12} className="mt-0.5 flex-shrink-0" />
          실제 Upbit 시장가 주문이 실행됩니다. 사용자 자금이 즉시 움직이며, 시장가는 호가 갭만큼 슬리피지 발생.
        </p>

        <div className="flex items-end gap-2">
          <div className="flex gap-1">
            <button
              type="button"
              onClick={() => setSide("buy")}
              className={`px-3 py-1.5 text-xs rounded-md border ${
                side === "buy"
                  ? "border-emerald-500 bg-emerald-500/10 text-emerald-300"
                  : "border-zinc-700 bg-zinc-900 text-zinc-400 hover:text-zinc-200"
              }`}
            >
              Buy
            </button>
            <button
              type="button"
              onClick={() => setSide("sell")}
              className={`px-3 py-1.5 text-xs rounded-md border ${
                side === "sell"
                  ? "border-rose-500 bg-rose-500/10 text-rose-300"
                  : "border-zinc-700 bg-zinc-900 text-zinc-400 hover:text-zinc-200"
              }`}
            >
              Sell
            </button>
          </div>

          <div className="flex-1">
            <label className="block text-[10px] text-zinc-500 mb-0.5">
              {side === "buy" ? "Amount (KRW)" : "Volume (ETH)"}
            </label>
            <Input
              type="number"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              placeholder={side === "buy" ? "10000" : "0.001"}
              disabled={submitting}
            />
          </div>

          <Button
            onClick={handlePlace}
            disabled={submitting || !amount}
            variant={side === "buy" ? "success" : "danger"}
          >
            {submitting ? "Placing..." : side === "buy" ? "Buy" : "Sell"}
          </Button>
        </div>

        {error && (
          <p className="text-xs text-rose-400 break-all">✗ {error}</p>
        )}
        {result && (
          <div className="text-xs space-y-0.5 bg-zinc-900/50 border border-zinc-800 rounded p-2 font-data">
            <div className="text-emerald-400">✓ 주문 성공</div>
            <div className="text-zinc-400">UUID: <span className="text-zinc-200">{result.uuid}</span></div>
            <div className="text-zinc-400">State: <span className="text-zinc-200">{result.state}</span></div>
            <div className="text-zinc-400">
              Executed: <span className="text-zinc-200">{result.executed_volume.toFixed(8)}</span>
              {result.state === "wait" && (
                <span className="text-amber-400 ml-2">(미체결 — 다음 cycle에서 reconcile)</span>
              )}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
