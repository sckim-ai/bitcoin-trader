import { useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { listPresets } from "../../lib/live";
import type { Preset } from "../../types";

interface Props {
  strategyKey: string;
  onClose: () => void;
  onSelect: (preset: Preset) => void;
}

export default function LoadPresetDialog({ strategyKey, onClose, onSelect }: Props) {
  const [presets, setPresets] = useState<Preset[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listPresets()
      .then((list) => setPresets(list.filter((p) => p.strategy_key === strategyKey)))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [strategyKey]);

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 w-[520px] max-h-[80vh] flex flex-col">
        <h3 className="text-lg font-semibold text-zinc-100 mb-4">
          Load Preset — {strategyKey}
        </h3>

        <div className="flex-1 overflow-y-auto">
          {loading && <p className="text-xs text-zinc-500">Loading...</p>}
          {error && <p className="text-xs text-rose-400">{error}</p>}
          {!loading && !error && presets.length === 0 && (
            <p className="text-xs text-zinc-500">
              No saved presets for {strategyKey} yet. Save the current parameters first.
            </p>
          )}
          {presets.length > 0 && (
            <ul className="space-y-1">
              {presets.map((p) => (
                <li
                  key={p.id}
                  onClick={() => { onSelect(p); onClose(); }}
                  className="cursor-pointer bg-zinc-800/40 hover:bg-zinc-800 border border-zinc-800 rounded-lg p-3 transition-colors"
                >
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-zinc-200 font-medium">{p.name}</span>
                    <span className="text-[10px] text-zinc-600">{p.created_at.slice(0, 10)}</span>
                  </div>
                  <div className="text-xs text-zinc-500 mt-1">
                    {p.market ?? "?"} · {p.timeframe ?? "?"}
                    {p.since_ts && p.until_ts ? ` · ${p.since_ts} ~ ${p.until_ts}` : ""}
                    <span className="ml-2 text-zinc-600">({p.source})</span>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="flex justify-end gap-2 pt-4 border-t border-zinc-800 mt-4">
          <Button variant="secondary" onClick={onClose}>Close</Button>
        </div>
      </div>
    </div>
  );
}
