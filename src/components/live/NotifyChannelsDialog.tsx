import { useEffect, useState } from "react";
import { MessageCircle, MessageCircleOff } from "lucide-react";
import { Button } from "../ui/Button";
import { listUpbitAccounts } from "../../lib/live";
import type { UpbitAccount } from "../../types";

interface Props {
  sessionId: number;
  sessionLabel: string;
  /** 현재 선택된 계정 ID 목록(서버 기준). undefined/[] 모두 "꺼짐"으로 간주. */
  initialAccountIds: number[];
  onClose: () => void;
  /** 모달이 [저장]을 누른 결과를 전달. 0개면 알림 off. */
  onSave: (accountIds: number[]) => Promise<void>;
}

/**
 * Paper 세션의 Discord 알림 fan-out 계정 선택 모달.
 *
 * - 등록된 Upbit 계정 목록을 보여주고 다중 체크박스로 선택.
 * - 각 계정의 webhook 상태(전용 vs 글로벌 fallback)를 라벨로 표기 — 어디로 갈지
 *   사용자가 알 수 있게.
 * - 0개 선택 + 저장 = 알림 off.
 *
 * 글로벌 webhook 자체는 선택 옵션으로 노출하지 않음(사용자 정책: 계정만).
 * 계정 webhook이 비어 있는 계정을 선택하면 백엔드가 글로벌 webhook으로 fallback해
 * 발송한다 — 이는 with_account_discord_webhook의 기존 동작과 일치.
 */
export function NotifyChannelsDialog({
  sessionId,
  sessionLabel,
  initialAccountIds,
  onClose,
  onSave,
}: Props) {
  const [accounts, setAccounts] = useState<UpbitAccount[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set(initialAccountIds));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listUpbitAccounts()
      .then(setAccounts)
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false));
  }, []);

  const toggle = (id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await onSave(Array.from(selected).sort((a, b) => a - b));
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleClose = () => {
    if (saving) return;
    onClose();
  };

  return (
    <div
      className="fixed inset-0 bg-black/60 flex items-center justify-center z-50"
      onMouseDown={(e) => { if (e.target === e.currentTarget) handleClose(); }}
    >
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 w-[420px] max-w-[90vw] space-y-4">
        <div>
          <h3 className="text-lg font-semibold text-zinc-100">Discord 알림 채널</h3>
          <p className="text-xs text-zinc-500 mt-1">
            세션 <span className="text-zinc-300">#{sessionId} · {sessionLabel}</span> · paper 모드
          </p>
        </div>

        <p className="text-xs text-zinc-500">
          선택한 계정의 Discord webhook으로 paper 신호 전이/시뮬 체결을 발송합니다.
          계정에 webhook이 없으면 Settings의 글로벌 채널로 fallback됩니다.
          체크 해제 후 저장하면 알림 off.
        </p>

        {loading ? (
          <p className="text-sm text-zinc-500">계정 목록 불러오는 중…</p>
        ) : accounts.length === 0 ? (
          <div className="text-sm text-zinc-500 bg-zinc-800/40 rounded-lg p-3">
            등록된 Upbit 계정이 없습니다. Accounts 페이지에서 먼저 추가하세요.
          </div>
        ) : (
          <div className="space-y-1 max-h-72 overflow-y-auto">
            {accounts.map((a) => {
              const checked = selected.has(a.id);
              const hasOwnWebhook = !!a.discord_webhook_url;
              return (
                <label
                  key={a.id}
                  className="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-zinc-800/50 cursor-pointer"
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={() => toggle(a.id)}
                    className="accent-violet-500"
                  />
                  <span className="flex-1 text-sm text-zinc-200">{a.label}</span>
                  <span
                    className={`text-[10px] ${hasOwnWebhook ? "text-violet-400" : "text-zinc-500"}`}
                    title={hasOwnWebhook
                      ? "계정 전용 webhook으로 전송"
                      : "이 계정엔 webhook이 없어 글로벌 webhook으로 fallback"}
                  >
                    {hasOwnWebhook ? "전용 채널" : "글로벌 fallback"}
                  </span>
                </label>
              );
            })}
          </div>
        )}

        <div className="text-xs text-zinc-500 bg-zinc-800/40 rounded-lg px-3 py-2 flex items-center gap-2">
          {selected.size > 0 ? (
            <>
              <MessageCircle size={13} className="text-violet-400" />
              <span>저장 시 <span className="text-zinc-300">{selected.size}개 채널</span>로 fan-out 됩니다.</span>
            </>
          ) : (
            <>
              <MessageCircleOff size={13} className="text-zinc-500" />
              <span>저장 시 이 세션의 Discord 알림이 <span className="text-zinc-300">꺼집니다</span>.</span>
            </>
          )}
        </div>

        {error && <p className="text-xs text-rose-400 break-all">{error}</p>}

        <div className="flex justify-end gap-2 pt-1">
          <Button variant="secondary" onClick={handleClose} disabled={saving}>취소</Button>
          <Button onClick={handleSave} disabled={saving || loading}>
            {saving ? "저장 중…" : "저장"}
          </Button>
        </div>
      </div>
    </div>
  );
}
