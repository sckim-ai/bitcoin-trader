import { useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";

interface Props {
  strategyKey: string;
  market: string;
  timeframe: string;
  since: string;
  until: string;
  onClose: () => void;
  onSubmit: (name: string) => Promise<void> | void;
}

export default function SavePresetDialog({
  strategyKey, market, timeframe, since, until, onClose, onSubmit,
}: Props) {
  const [name, setName] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    if (!name.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      await onSubmit(name.trim());
      onClose();
    } catch (e) {
      setError(String(e));
      setSubmitting(false);
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !submitting) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, submitting]);

  return (
    <div
      className="fixed inset-0 bg-black/60 flex items-center justify-center z-50"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !submitting) onClose();
      }}
    >
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 w-96 space-y-4">
        <h3 className="text-lg font-semibold text-zinc-100">Save Simulation Preset</h3>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Preset Name</label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g., V3-baseline-eth-2025"
            autoFocus
          />
        </div>

        <div className="text-xs text-zinc-500 bg-zinc-800/40 rounded-lg p-3 space-y-1">
          <div><span className="text-zinc-400">Strategy:</span> <span className="text-zinc-200">{strategyKey}</span></div>
          <div><span className="text-zinc-400">Market:</span> <span className="text-zinc-200">{market}</span></div>
          <div><span className="text-zinc-400">Timeframe:</span> <span className="text-zinc-200">{timeframe}</span></div>
          <div><span className="text-zinc-400">Window:</span> <span className="text-zinc-200">{since} ~ {until}</span></div>
        </div>

        {error && <p className="text-xs text-rose-400">{error}</p>}

        <div className="flex justify-end gap-2 pt-2">
          <Button variant="secondary" onClick={onClose} disabled={submitting}>Cancel</Button>
          <Button disabled={!name.trim() || submitting} onClick={handleSubmit}>
            {submitting ? "Saving..." : "Save"}
          </Button>
        </div>
      </div>
    </div>
  );
}
