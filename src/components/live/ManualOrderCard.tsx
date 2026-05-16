import { useEffect, useState } from "react";
import { ShoppingCart, AlertTriangle } from "lucide-react";
import { Card, CardContent, CardHeader } from "../ui/Card";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Badge } from "../ui/Badge";
import { confirmDialog } from "../ui/ConfirmDialog";
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

  // 1개면 자동 선택, 2개 이상이면 사용자가 직접 선택
  const [selectedSessionId, setSelectedSessionId] = useState<number | null>(
    realSessions.length === 1 ? realSessions[0].id : null,
  );

  useEffect(() => {
    if (realSessions.length === 1 && selectedSessionId === null) {
      setSelectedSessionId(realSessions[0].id);
    } else if (realSessions.length === 0) {
      setSelectedSessionId(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [realSessions.length]);

  const activeSession = realSessions.find((s) => s.id === selectedSessionId) ?? null;

  const [side, setSide] = useState<"buy" | "sell">("buy");
  const [ordType, setOrdType] = useState<"market" | "limit">("market");
  const [amount, setAmount] = useState<string>("10000");
  /// limit 주문 시 가격 (KRW per ETH). 비어있으면 입력 강제.
  const [limitPrice, setLimitPrice] = useState<string>("");
  const [submitting, setSubmitting] = useState(false);
  const [result, setResult] = useState<ManualOrderResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  /// limit-buy 시 입력한 KRW + 가격으로 자동 환산되는 ETH 예상 수량.
  /// UI에 미리보기로 보여 사용자가 “얼마나 살 수 있는지” 즉시 인지.
  const estimatedVolume = (() => {
    if (ordType !== "limit" || side !== "buy") return null;
    const amt = Number(amount);
    const px = Number(limitPrice);
    if (!Number.isFinite(amt) || !Number.isFinite(px) || amt <= 0 || px <= 0) return null;
    return Math.floor((amt / px) * 1e8) / 1e8;
  })();

  const handlePlace = async () => {
    setError(null);
    setResult(null);

    const amt = Number(amount);
    if (!Number.isFinite(amt) || amt <= 0) {
      setError("금액(또는 수량)은 양수여야 합니다.");
      return;
    }

    // Limit 주문이면 limit_price도 필수.
    let limitPx: number | undefined;
    if (ordType === "limit") {
      limitPx = Number(limitPrice);
      if (!Number.isFinite(limitPx) || limitPx <= 0) {
        setError("Limit 주문에는 가격(Limit Price)이 필요합니다.");
        return;
      }
    }

    // Sanity guard before confirm — Upbit minimum + reasonable manual cap.
    if (side === "buy") {
      if (ordType === "market") {
        if (amt < 5_000) { setError("최소 매수 5,000 KRW."); return; }
      } else {
        // limit-buy: 명목금액(volume × price)이 5K 이상이어야.
        const notional = (estimatedVolume ?? 0) * (limitPx ?? 0);
        if (notional < 5_000) {
          setError(`Limit 매수 명목금액이 너무 작음 (${Math.round(notional)} KRW < 5,000)`);
          return;
        }
      }
      if (amt > 1_000_000) {
        const ok = await confirmDialog({
          title: "큰 금액 매수 확인",
          severity: "warning",
          confirmLabel: "계속",
          body: (
            <p>
              매수 금액이{" "}
              <span className="text-amber-400 font-data font-semibold">
                {amt.toLocaleString()} KRW
              </span>{" "}
              로 1,000,000 KRW 를 초과합니다. 진행할까요?
            </p>
          ),
        });
        if (!ok) return;
      }
    } else {
      if (amt < 0.0001) { setError("매도 수량이 너무 작습니다 (≥ 0.0001 ETH)."); return; }
    }

    const orderTypeLabel = ordType === "market" ? "시장가" : "지정가";
    const sideColor = side === "buy" ? "text-emerald-400" : "text-rose-400";
    const sideLabel = side === "buy" ? "매수" : "매도";
    const ok = await confirmDialog({
      title: "실제 자금 이동",
      severity: "danger",
      confirmLabel: `${sideLabel} 진행`,
      body: (
        <div className="space-y-3">
          <p className="text-zinc-200">
            {side === "buy" ? (
              <>
                <span className="text-amber-400 font-data font-semibold">
                  {amt.toLocaleString()} KRW
                </span>
                <span className="text-zinc-500"> 어치 </span>
                <span className="font-medium">ETH</span>
                <span className="text-zinc-500">를 </span>
                <span className={`font-medium ${sideColor}`}>{orderTypeLabel} {sideLabel}</span>
              </>
            ) : (
              <>
                <span className="text-amber-400 font-data font-semibold">{amt} ETH</span>
                <span className="text-zinc-500">를 </span>
                <span className={`font-medium ${sideColor}`}>{orderTypeLabel} {sideLabel}</span>
              </>
            )}
          </p>
          {ordType === "limit" && limitPx != null && (
            <p className="text-xs text-zinc-400">
              지정가:{" "}
              <span className="text-zinc-200 font-data">{limitPx.toLocaleString()} KRW</span>
              {side === "buy" && estimatedVolume != null && (
                <>
                  {" · 예상 수량: "}
                  <span className="text-zinc-200 font-data">{estimatedVolume.toFixed(8)} ETH</span>
                </>
              )}
            </p>
          )}
          <div className="flex items-center gap-2 text-xs">
            <span className="text-zinc-500">대상 세션</span>
            {activeSession ? (
              <Badge variant="amber">
                #{activeSession.id} {activeSession.label}
                {activeSession.account_label && (
                  <span className="ml-1 text-zinc-400">[{activeSession.account_label}]</span>
                )}
              </Badge>
            ) : (
              <span className="text-zinc-600">미부착 (history 미귀속)</span>
            )}
          </div>
          <p className="text-xs text-zinc-500">
            {ordType === "market"
              ? "Upbit 계정에서 실제 자금이 즉시 이동하며, 시장가 슬리피지가 발생합니다."
              : "Upbit 호가창에 지정가 주문을 등록합니다. 미체결 상태로 남을 수 있으며, 별도 cancel 전까지 호가창에 살아있습니다."}
          </p>
        </div>
      ),
    });
    if (!ok) return;

    setSubmitting(true);
    try {
      const r = await manualMarketOrder({
        market: "KRW-ETH",
        side,
        ord_type: ordType,
        krw_amount: side === "buy" ? amt : undefined,
        volume: side === "sell" ? amt : undefined,
        limit_price: ordType === "limit" ? limitPx : undefined,
        session_id: selectedSessionId ?? undefined,
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
      <CardHeader className="flex flex-wrap items-center gap-2">
        <ShoppingCart size={16} className="text-amber-500" />
        <h3 className="text-sm font-semibold text-zinc-300">Manual Order (KRW-ETH)</h3>
        {realSessions.length === 0 && (
          <span className="ml-2 text-[10px] text-zinc-600">
            ↳ no real session — fill won't attribute to history
          </span>
        )}
        {realSessions.length === 1 && activeSession && (
          <span className="ml-2 text-[10px] text-zinc-500">
            ↳ session #{activeSession.id} {activeSession.label}
            {activeSession.account_label && ` [${activeSession.account_label}]`}
          </span>
        )}
        {realSessions.length >= 2 && (
          <select
            value={selectedSessionId ?? ""}
            onChange={(e) => setSelectedSessionId(e.target.value === "" ? null : Number(e.target.value))}
            className="ml-2 bg-zinc-800 border border-zinc-700 rounded-md px-2 py-0.5 text-xs text-zinc-200"
          >
            <option value="">— 세션 선택 —</option>
            {realSessions.map((s) => (
              <option key={s.id} value={s.id}>
                {s.account_label ? `[${s.account_label}] ` : ""}{s.label}
              </option>
            ))}
          </select>
        )}
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-xs text-amber-400 flex items-start gap-1.5">
          <AlertTriangle size={12} className="mt-0.5 flex-shrink-0" />
          실제 Upbit 시장가 주문이 실행됩니다. 사용자 자금이 즉시 움직이며, 시장가는 호가 갭만큼 슬리피지 발생.
        </p>

        {/* Side + ord_type 토글 묶음 */}
        <div className="flex flex-wrap items-center gap-3">
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

          <div className="flex gap-1" role="group" aria-label="Order type">
            <button
              type="button"
              onClick={() => setOrdType("market")}
              className={`px-3 py-1.5 text-xs rounded-md border ${
                ordType === "market"
                  ? "border-amber-500 bg-amber-500/10 text-amber-300"
                  : "border-zinc-700 bg-zinc-900 text-zinc-400 hover:text-zinc-200"
              }`}
              title="시장가 — 즉시 체결, 슬리피지 발생"
            >
              Market
            </button>
            <button
              type="button"
              onClick={() => setOrdType("limit")}
              className={`px-3 py-1.5 text-xs rounded-md border ${
                ordType === "limit"
                  ? "border-amber-500 bg-amber-500/10 text-amber-300"
                  : "border-zinc-700 bg-zinc-900 text-zinc-400 hover:text-zinc-200"
              }`}
              title="지정가 — Upbit 호가창에 등록, 미체결 가능"
            >
              Limit
            </button>
          </div>
        </div>

        {/* Amount/Volume + (limit) Price 입력 */}
        <div className="flex flex-wrap items-end gap-2">
          <div className="flex-1 min-w-[150px]">
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

          {ordType === "limit" && (
            <div className="flex-1 min-w-[150px]">
              <label className="block text-[10px] text-zinc-500 mb-0.5">
                Limit Price (KRW)
              </label>
              <Input
                type="number"
                value={limitPrice}
                onChange={(e) => setLimitPrice(e.target.value)}
                placeholder="3400000"
                disabled={submitting}
              />
            </div>
          )}

          <Button
            onClick={handlePlace}
            disabled={
              submitting ||
              !amount ||
              (ordType === "limit" && !limitPrice) ||
              (realSessions.length >= 2 && selectedSessionId === null)
            }
            variant={side === "buy" ? "success" : "danger"}
          >
            {submitting ? "Placing..." : `${side === "buy" ? "Buy" : "Sell"} (${ordType})`}
          </Button>
        </div>

        {/* Limit-buy 예상 수량 미리보기 */}
        {ordType === "limit" && side === "buy" && estimatedVolume != null && (
          <p className="text-[11px] text-zinc-500 font-data">
            예상 수량: <span className="text-zinc-300">{estimatedVolume.toFixed(8)} ETH</span>
            <span className="text-zinc-600"> (= floor({Number(amount).toLocaleString()} ÷ {Number(limitPrice).toLocaleString()} × 10⁸) ÷ 10⁸)</span>
          </p>
        )}

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
