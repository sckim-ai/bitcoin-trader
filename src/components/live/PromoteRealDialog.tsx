import { useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { AlertTriangle } from "lucide-react";
import type { LiveSession, UpbitAccount } from "../../types";
import { listUpbitAccounts } from "../../lib/live";

interface Props {
  session: LiveSession;
  onClose: () => void;
  /** Receives the selected account id so the caller can call toggleSessionMode(id, 'real', accountId). */
  onConfirm: (accountId: number) => Promise<void>;
}

/**
 * Hard-confirm dialog for paper → real promotion.
 *
 * Required step: pick the Upbit account. The backend's partial unique index
 * (idx_session_account_running_real) rejects a second running real session on
 * the same account, so accounts with an existing running real are visible but
 * disabled. If this session was previously real on some account, that account
 * is the default selection (preserved across paper↔real toggling).
 */
export default function PromoteRealDialog({ session, onClose, onConfirm }: Props) {
  const [accounts, setAccounts] = useState<UpbitAccount[]>([]);
  const [accountId, setAccountId] = useState<number | null>(
    session.upbit_account_id ?? null,
  );
  const [loadingAccounts, setLoadingAccounts] = useState(true);
  const [acknowledged, setAcknowledged] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listUpbitAccounts()
      .then((list) => {
        setAccounts(list);
        // 세션에 이미 연결된 계정이 없으면, 선택 가능한 계정이 1개일 때 자동 선택.
        if (accountId == null) {
          const selectable = list.filter((a) => a.enabled && !a.has_running_session);
          if (selectable.length === 1) setAccountId(selectable[0].id);
        }
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoadingAccounts(false));
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleConfirm = async () => {
    if (!acknowledged || accountId == null) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm(accountId);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSubmitting(false);
    }
  };

  const previousAccountId = session.upbit_account_id;

  return (
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="bg-zinc-900 border border-rose-900/60 rounded-xl p-6 w-[480px] space-y-4">
        <div className="flex items-center gap-2">
          <AlertTriangle size={20} className="text-rose-500" />
          <h3 className="text-lg font-semibold text-zinc-100">실거래 모드로 전환</h3>
        </div>

        <div className="text-sm text-zinc-300 space-y-2">
          <p>
            세션 <span className="font-medium text-amber-400">{session.label}</span>을(를)
            <span className="font-medium text-rose-400"> 실거래(real)</span> 모드로 전환합니다.
          </p>
          <p className="text-zinc-400 text-xs leading-relaxed">
            전환 후 매 정시 사이클에서 발생하는 매수/매도 신호가 실제 Upbit 주문으로 실행됩니다.
            보유 ETH 잔고와 KRW 현금이 직접 영향을 받으며, 시뮬레이션과 실제 체결가의 차이로
            손실이 발생할 수 있습니다.
          </p>
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">
            Upbit Account <span className="text-rose-400">*</span>
          </label>
          {loadingAccounts ? (
            <p className="text-xs text-zinc-500">계정 로딩 중...</p>
          ) : accounts.length === 0 ? (
            <p className="text-xs text-amber-400">
              등록된 계정이 없습니다. Accounts 페이지에서 먼저 등록하세요.
            </p>
          ) : (
            <>
              <select
                value={accountId ?? ""}
                onChange={(e) => setAccountId(e.target.value === "" ? null : Number(e.target.value))}
                className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-200"
              >
                <option value="">— 계정 선택 —</option>
                {accounts.map((a) => {
                  // 이 세션 자신이 이미 그 계정에 묶여 있다면 disable하지 않음.
                  const selfOnAccount = a.id === previousAccountId;
                  const blockedByOther = a.has_running_session && !selfOnAccount;
                  const unavailable = !a.enabled || blockedByOther;
                  return (
                    <option key={a.id} value={a.id} disabled={unavailable}>
                      {a.label}
                      {!a.enabled ? " (비활성)" : blockedByOther ? " (다른 real 실행 중)" : ""}
                    </option>
                  );
                })}
              </select>
              {previousAccountId != null && (
                <p className="text-[10px] text-zinc-500 mt-1">
                  이전에 연결된 계정이 기본 선택됩니다.
                </p>
              )}
            </>
          )}
        </div>

        <div className="bg-zinc-800/50 border border-zinc-700 rounded-lg p-3 text-xs text-zinc-400 space-y-1">
          <div><span className="text-zinc-500">Market:</span> <span className="text-zinc-200">{session.market}</span></div>
          <div><span className="text-zinc-500">Initial capital:</span> <span className="text-zinc-200">{session.initial_capital.toLocaleString()} KRW</span></div>
          <div><span className="text-zinc-500">Current position:</span> <span className="text-zinc-200">{session.current_position}</span></div>
          <div className="border-t border-zinc-700 pt-1 mt-1">
            <span className="text-zinc-500">Daily safety limits:</span>{" "}
            <span className="text-rose-300">{session.max_daily_loss_pct}% loss</span>
            {" · "}
            <span className="text-rose-300">{session.max_daily_trades} trades</span>
            <span className="text-zinc-600"> (auto-stop)</span>
          </div>
        </div>

        <div className="space-y-2 text-xs">
          <p className="text-amber-400">
            ⚠ 첫 운용 권장: <span className="font-medium">매우 작은 자본(예: 50,000원)</span>으로 paper 결과와 비교 검증.
          </p>
          <p className="text-zinc-500">
            동일 계정에서는 실거래 세션 1개만 동시 운용 가능합니다. (계정별 부분 유니크 인덱스로 강제)
          </p>
        </div>

        <label className="flex items-start gap-2 text-xs text-zinc-300 cursor-pointer">
          <input
            type="checkbox"
            checked={acknowledged}
            onChange={(e) => setAcknowledged(e.target.checked)}
            className="mt-0.5 accent-rose-500 cursor-pointer"
          />
          <span>실제 자금이 움직이며, 손실 가능성을 이해합니다.</span>
        </label>

        {error && <p className="text-xs text-rose-400 break-all">{error}</p>}

        <div className="flex justify-end gap-2 pt-1">
          <Button variant="secondary" onClick={onClose} disabled={submitting}>Cancel</Button>
          <Button
            variant="danger"
            disabled={!acknowledged || submitting || accountId == null}
            onClick={handleConfirm}
          >
            {submitting ? "Promoting..." : "Promote to Real"}
          </Button>
        </div>
      </div>
    </div>
  );
}
