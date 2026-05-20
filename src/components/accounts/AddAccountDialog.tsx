import { useState } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { addUpbitAccount } from "../../lib/live";

interface Props {
  open: boolean;
  onClose: () => void;
  onAdded: () => void;
}

export function AddAccountDialog({ open, onClose, onAdded }: Props) {
  const [label, setLabel] = useState("");
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [discordWebhook, setDiscordWebhook] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  const canSubmit = label.trim().length > 0 && accessKey.trim().length > 0 && secretKey.trim().length > 0;

  const handleSubmit = async () => {
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      await addUpbitAccount({
        label: label.trim(),
        access_key: accessKey.trim(),
        secret_key: secretKey.trim(),
        discord_webhook_url: discordWebhook.trim() || undefined,
      });
      setLabel("");
      setAccessKey("");
      setSecretKey("");
      setDiscordWebhook("");
      onAdded();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleClose = () => {
    if (busy) return;
    setLabel("");
    setAccessKey("");
    setSecretKey("");
    setDiscordWebhook("");
    setError(null);
    onClose();
  };

  return (
    <div
      className="fixed inset-0 bg-black/60 flex items-center justify-center z-50"
      onMouseDown={(e) => { if (e.target === e.currentTarget) handleClose(); }}
    >
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 w-96 space-y-4">
        <h3 className="text-lg font-semibold text-zinc-100">계정 추가</h3>

        <p className="text-xs text-zinc-500">
          OS 키체인에 저장됩니다. 등록 시 연결 테스트를 자동으로 수행하며, 실패하면 등록이 취소됩니다.
        </p>

        <Input
          label="라벨"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          placeholder="예: 주계좌, 법인"
          disabled={busy}
        />
        <Input
          label="Access Key"
          type="password"
          passwordToggle
          value={accessKey}
          onChange={(e) => setAccessKey(e.target.value)}
          placeholder="Upbit Access Key"
          disabled={busy}
        />
        <Input
          label="Secret Key"
          type="password"
          passwordToggle
          value={secretKey}
          onChange={(e) => setSecretKey(e.target.value)}
          placeholder="Upbit Secret Key"
          disabled={busy}
        />
        <div className="space-y-1">
          <Input
            label="Discord Webhook (선택)"
            value={discordWebhook}
            onChange={(e) => setDiscordWebhook(e.target.value)}
            placeholder="https://discord.com/api/webhooks/..."
            disabled={busy}
          />
          <p className="text-[10px] text-zinc-500">
            비워두면 Settings의 글로벌 Discord 채널을 사용합니다.
          </p>
        </div>

        {error && (
          <p className="text-xs text-rose-400 break-all">{error}</p>
        )}

        <div className="flex justify-end gap-2 pt-1">
          <Button variant="secondary" onClick={handleClose} disabled={busy}>
            취소
          </Button>
          <Button onClick={handleSubmit} disabled={!canSubmit || busy}>
            {busy ? "등록 중…" : "추가"}
          </Button>
        </div>
      </div>
    </div>
  );
}
