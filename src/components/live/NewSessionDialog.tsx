import { useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import type { Preset } from "../../types";

interface Props {
  presets: Preset[];
  onClose: () => void;
  onSubmit: (args: {
    label: string;
    preset_id: number;
    market: string;
    initial_capital: number;
  }) => void;
}

export default function NewSessionDialog({ presets, onClose, onSubmit }: Props) {
  const [label, setLabel] = useState("");
  const [labelTouched, setLabelTouched] = useState(false);
  const [presetId, setPresetId] = useState<number | null>(null);
  const [capital, setCapital] = useState(1_000_000);

  useEffect(() => {
    if (presets.length > 0 && presetId == null) setPresetId(presets[0].id);
  }, [presets, presetId]);

  useEffect(() => {
    if (!labelTouched && presetId != null) {
      const p = presets.find((x) => x.id === presetId);
      if (p) setLabel(p.name);
    }
  }, [presetId, presets, labelTouched]);

  const canSubmit = label.trim().length > 0 && presetId != null && capital > 0;
  const selectedPreset = presetId != null ? presets.find((p) => p.id === presetId) : null;

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 w-96 space-y-4">
        <h3 className="text-lg font-semibold text-zinc-100">New Paper Session</h3>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">
            Label <span className="text-rose-400">*</span>
          </label>
          <Input
            value={label}
            onChange={(e) => { setLabel(e.target.value); setLabelTouched(true); }}
            placeholder="세션 이름 (프리셋 이름이 기본값)"
          />
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Market</label>
          <p className="text-sm text-zinc-300">KRW-ETH <span className="text-zinc-600">(fixed)</span></p>
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Preset</label>
          <select
            value={presetId ?? ""}
            onChange={(e) => setPresetId(Number(e.target.value))}
            className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-200"
          >
            {presets.length === 0 ? (
              <option value="">No presets available — create one in Simulation page</option>
            ) : (
              presets.map((p) => (
                <option key={p.id} value={p.id}>{p.strategy_key}: {p.name}</option>
              ))
            )}
          </select>
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">Initial Capital (KRW)</label>
          <Input type="number" value={capital} onChange={(e) => setCapital(Number(e.target.value))} />
        </div>

        <div className="text-xs text-zinc-500 bg-zinc-800/40 rounded-lg p-3">
          <div className="text-zinc-400 mb-1">Simulation window</div>
          <div>
            {selectedPreset?.since_ts && selectedPreset?.until_ts
              ? <span className="text-zinc-200">{selectedPreset.since_ts} ~ now</span>
              : <span className="text-zinc-500">프리셋의 시작 시점부터 현재까지 리플레이</span>
            }
          </div>
          <div className="text-zinc-600 mt-1">
            Start를 누르면 이 구간을 즉시 리플레이해 현재 포지션·시그널을 계산하고,
            이후 매 정시에 최신 봉으로 갱신합니다.
          </div>
        </div>

        <div className="flex justify-end gap-2 pt-2">
          <Button variant="secondary" onClick={onClose}>Cancel</Button>
          <Button
            disabled={!canSubmit}
            onClick={() => {
              onSubmit({
                label: label.trim(),
                preset_id: presetId!,
                market: "KRW-ETH",
                initial_capital: capital,
              });
            }}
          >
            Create
          </Button>
        </div>
      </div>
    </div>
  );
}
