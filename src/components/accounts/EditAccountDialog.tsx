import { useState } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { updateUpbitAccount } from "../../lib/live";
import type { UpbitAccount } from "../../types";

interface Props {
  account: UpbitAccount;
  onClose: () => void;
  onUpdated: () => void;
}

type Tab = "label" | "keys" | "discord";

export function EditAccountDialog({ account, onClose, onUpdated }: Props) {
  const [tab, setTab] = useState<Tab>("label");
  const [label, setLabel] = useState(account.label);
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [discordWebhook, setDiscordWebhook] = useState(account.discord_webhook_url ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleClose = () => {
    if (busy) return;
    onClose();
  };

  const handleSubmit = async () => {
    setBusy(true);
    setError(null);
    try {
      if (tab === "label") {
        const trimmed = label.trim();
        if (!trimmed) {
          setError("라벨을 입력하세요.");
          return;
        }
        await updateUpbitAccount({ id: account.id, label: trimmed });
      } else if (tab === "keys") {
        if (!accessKey.trim() || !secretKey.trim()) {
          setError("Access Key와 Secret Key를 모두 입력하세요.");
          return;
        }
        await updateUpbitAccount({ id: account.id, access_key: accessKey.trim(), secret_key: secretKey.trim() });
      } else {
        // discord 탭 — 빈 문자열도 명시적으로 보내야 NULL로 정규화됨.
        await updateUpbitAccount({ id: account.id, discord_webhook_url: discordWebhook.trim() });
      }
      onUpdated();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const keysDisabled = account.has_running_session;

  return (
    <div
      className="fixed inset-0 bg-black/60 flex items-center justify-center z-50"
      onMouseDown={(e) => { if (e.target === e.currentTarget) handleClose(); }}
    >
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 w-96 space-y-4">
        <h3 className="text-lg font-semibold text-zinc-100">계정 편집</h3>
        <p className="text-xs text-zinc-500">{account.label}</p>

        {/* Tab toggle */}
        <div className="flex gap-1 bg-zinc-800 rounded-lg p-1">
          <button
            onClick={() => { setTab("label"); setError(null); }}
            className={`flex-1 py-1.5 rounded-md text-xs font-medium transition-colors ${
              tab === "label"
                ? "bg-zinc-700 text-zinc-100"
                : "text-zinc-400 hover:text-zinc-200"
            }`}
          >
            라벨 변경
          </button>
          <button
            onClick={() => { if (!keysDisabled) { setTab("keys"); setError(null); } }}
            disabled={keysDisabled}
            title={keysDisabled ? "실행 중인 세션이 있어 키를 변경할 수 없습니다." : undefined}
            className={`flex-1 py-1.5 rounded-md text-xs font-medium transition-colors ${
              tab === "keys"
                ? "bg-zinc-700 text-zinc-100"
                : keysDisabled
                  ? "text-zinc-600 cursor-not-allowed"
                  : "text-zinc-400 hover:text-zinc-200"
            }`}
          >
            API 키 교체
            {keysDisabled && <span className="ml-1 text-[10px] text-amber-500">(세션 중)</span>}
          </button>
          <button
            onClick={() => { setTab("discord"); setError(null); }}
            className={`flex-1 py-1.5 rounded-md text-xs font-medium transition-colors ${
              tab === "discord"
                ? "bg-zinc-700 text-zinc-100"
                : "text-zinc-400 hover:text-zinc-200"
            }`}
          >
            Discord
          </button>
        </div>

        {/* Label mode */}
        {tab === "label" && (
          <Input
            label="라벨"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="예: 주계좌, 법인"
            disabled={busy}
          />
        )}

        {/* Keys mode */}
        {tab === "keys" && (
          <div className="space-y-3">
            <p className="text-xs text-zinc-500">
              새 키를 입력하면 연결 테스트 후 교체됩니다.
            </p>
            <Input
              label="Access Key"
              type="password"
              passwordToggle
              value={accessKey}
              onChange={(e) => setAccessKey(e.target.value)}
              placeholder="새 Upbit Access Key"
              disabled={busy}
            />
            <Input
              label="Secret Key"
              type="password"
              passwordToggle
              value={secretKey}
              onChange={(e) => setSecretKey(e.target.value)}
              placeholder="새 Upbit Secret Key"
              disabled={busy}
            />
          </div>
        )}

        {/* Discord mode */}
        {tab === "discord" && (
          <div className="space-y-2">
            <p className="text-xs text-zinc-500">
              이 계정 거래에 대한 Discord webhook. 비워두고 저장하면 Settings의
              글로벌 webhook을 fallback으로 사용합니다.
            </p>
            <Input
              label="Webhook URL"
              value={discordWebhook}
              onChange={(e) => setDiscordWebhook(e.target.value)}
              placeholder="https://discord.com/api/webhooks/..."
              disabled={busy}
            />
          </div>
        )}

        {error && (
          <p className="text-xs text-rose-400 break-all">{error}</p>
        )}

        <div className="flex justify-end gap-2 pt-1">
          <Button variant="secondary" onClick={handleClose} disabled={busy}>
            취소
          </Button>
          <Button onClick={handleSubmit} disabled={busy}>
            {busy ? "저장 중…" : "저장"}
          </Button>
        </div>
      </div>
    </div>
  );
}
