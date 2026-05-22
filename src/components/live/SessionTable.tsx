import { MessageCircle, MessageCircleOff } from "lucide-react";
import { Button } from "../ui/Button";
import { Badge } from "../ui/Badge";
import type { LiveSession, Preset, TickData } from "../../types";
import { deriveSessionPnl, useLiveTradingStore } from "../../stores/liveTradingStore";
import { colorFor } from "./charts/sessionPalette";

interface Props {
  sessions: LiveSession[];
  presets: Preset[];
  ticks: Record<string, TickData>;
  onStart: (id: number) => void;
  onStop: (id: number) => void;
  onDelete: (id: number) => void;
  /** Open the paper→real confirm dialog for the given session. */
  onPromoteRequest: (session: LiveSession) => void;
  /** Demote a real session back to paper (no confirm — reversible direction). */
  onDemote: (id: number) => void;
  /** Open the per-session Discord channel picker (paper only). */
  onEditNotifyChannels: (session: LiveSession) => void;
}

function pctColor(v: number, neutral = "text-zinc-500") {
  return v > 0 ? "text-emerald-400" : v < 0 ? "text-rose-400" : neutral;
}

function fmtPct(v: number) {
  return `${v >= 0 ? "+" : ""}${v.toFixed(2)}%`;
}

export default function SessionTable({
  sessions, presets, ticks, onStart, onStop, onDelete, onPromoteRequest, onDemote, onEditNotifyChannels,
}: Props) {
  const hiddenSessionIds = useLiveTradingStore((s) => s.hiddenSessionIds);
  const toggleSessionVisibility = useLiveTradingStore((s) => s.toggleSessionVisibility);
  const presetById = new Map(presets.map((p) => [p.id, p]));
  const sortedSessionIds = sessions.map((s) => s.id).slice().sort((a, b) => a - b);

  if (sessions.length === 0) {
    return <p className="text-zinc-500 text-sm">No sessions yet. Create one to start.</p>;
  }
  // 모든 셀에 가로 padding 일괄 적용 — text-right + text-left 인접 컬럼이
  // 패딩 없이 만나면 헤더("UnrealizedSignal") / 값("-0.18%holding")이 붙어 보임.
  return (
    <table className="w-full text-sm [&_th]:px-3 [&_td]:px-3">
      <thead className="text-zinc-400 text-xs">
        <tr className="border-b border-zinc-800">
          <th className="text-center py-2 w-10">Show</th>
          <th className="text-left">Label</th>
          <th className="text-left">Market</th>
          <th className="text-left">Mode</th>
          <th className="text-left">Status</th>
          <th className="text-left">Position</th>
          <th className="text-right">Equity</th>
          <th className="text-right" title="Preset 저장 시 측정한 백테스트 결과">Backtest</th>
          <th className="text-right" title="Start 누른 시점 이후 발생한 매매 누적">Live</th>
          <th className="text-right">Unrealized</th>
          <th className="text-left">Signal</th>
          <th className="text-right">Last Cycle</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {sessions.map((s) => {
          const tick = ticks[s.market];
          const derived = deriveSessionPnl(s, tick);
          const preset = presetById.get(s.preset_id);
          const baseline = preset?.baseline_return;
          const baselineTrades = preset?.baseline_trades;
          const visible = !hiddenSessionIds.includes(s.id);
          const color = colorFor(s.id, sortedSessionIds);
          return (
            <tr key={s.id} className="border-b border-zinc-900 hover:bg-zinc-900/40">
              <td className="py-2 text-center">
                <label
                  className="inline-flex items-center gap-1.5 cursor-pointer"
                  title={visible ? "Hide on chart" : "Show on chart"}
                >
                  <input
                    type="checkbox"
                    checked={visible}
                    onChange={() => toggleSessionVisibility(s.id)}
                    className="accent-emerald-500 cursor-pointer"
                  />
                  <span
                    className="w-2.5 h-2.5 rounded-sm"
                    style={{ backgroundColor: color, opacity: visible ? 1 : 0.35 }}
                  />
                </label>
              </td>
              <td className="font-medium text-zinc-200">
                <div className="flex flex-col gap-0.5 leading-tight">
                  <span>{s.label}</span>
                  {s.account_label ? (
                    <Badge variant="default" className="self-start text-[10px] px-1 py-0">
                      [{s.account_label}]
                    </Badge>
                  ) : s.upbit_account_id !== null ? (
                    <Badge variant="default" className="self-start text-[10px] px-1 py-0 opacity-50">
                      [삭제됨]
                    </Badge>
                  ) : null}
                </div>
              </td>
              <td className="text-zinc-400">{s.market}</td>
              <td>
                <div className="flex flex-col gap-0.5 leading-tight">
                  <Badge variant={s.mode === "real" ? "amber" : "default"}>
                    {s.mode === "real" ? "REAL" : "paper"}
                  </Badge>
                  {s.mode === "real" ? (
                    <span
                      className="text-[10px] text-zinc-600 font-data italic"
                      title="Daily loss / trade-count auto-stop is currently disabled. Use Kill switch for manual emergency stop."
                    >
                      limits off
                    </span>
                  ) : (() => {
                    const channelCount = s.notify_account_ids?.length ?? 0;
                    const on = channelCount > 0;
                    return (
                      <button
                        type="button"
                        onClick={() => onEditNotifyChannels(s)}
                        title={
                          on
                            ? `Discord 알림 ${channelCount}개 채널로 발송 중 — 클릭하여 채널 편집`
                            : "Discord 알림 꺼짐 — 클릭하여 채널 선택"
                        }
                        className={`inline-flex items-center gap-1 text-[10px] self-start ${
                          on ? "text-violet-400 hover:text-violet-300" : "text-zinc-600 hover:text-zinc-400"
                        }`}
                      >
                        {on ? <MessageCircle size={11} /> : <MessageCircleOff size={11} />}
                        <span>{on ? `discord (${channelCount})` : "discord off"}</span>
                      </button>
                    );
                  })()}
                </div>
              </td>
              <td>
                <Badge variant={s.status === "running" ? "green" : "default"}>
                  {s.status}
                </Badge>
              </td>
              <td>
                <Badge variant={s.current_position === "holding" ? "amber" : "default"}>
                  {s.current_position}
                  {s.current_position === "holding" && s.current_buy_price != null && (
                    <span className="ml-1 text-[10px] text-zinc-400 font-data">
                      @ {s.current_buy_price.toLocaleString()}
                    </span>
                  )}
                </Badge>
              </td>
              <td className="text-right font-data text-zinc-200">
                {Math.round(derived.currentEquity).toLocaleString()}
              </td>
              <td className="text-right font-data">
                {baseline != null ? (
                  <div className="leading-tight">
                    <div className={pctColor(baseline)}>{fmtPct(baseline)}</div>
                    {baselineTrades != null && (
                      <div className="text-[10px] text-zinc-600">{baselineTrades} tr</div>
                    )}
                  </div>
                ) : (
                  <span className="text-zinc-600">—</span>
                )}
              </td>
              <td className="text-right font-data">
                <div className={`leading-tight ${pctColor(s.live_return, "text-zinc-400")}`}>
                  {fmtPct(s.live_return)}
                </div>
              </td>
              <td className={`text-right font-data ${pctColor(derived.unrealizedPnlPct)}`}>
                {s.current_position === "holding"
                  ? fmtPct(derived.unrealizedPnlPct)
                  : "--"}
              </td>
              <td className="text-zinc-400">{s.last_signal ?? "--"}</td>
              <td className="text-right text-zinc-500 text-xs">
                {s.last_cycle_ts?.slice(11, 16) ?? "--"}
              </td>
              <td className="text-right">
                <div className="flex gap-1 justify-end">
                  {s.status === "stopped" ? (
                    <Button size="sm" variant="success" onClick={() => onStart(s.id)}>Start</Button>
                  ) : (
                    <Button size="sm" variant="secondary" onClick={() => onStop(s.id)}>Stop</Button>
                  )}
                  {s.mode === "paper" ? (
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => onPromoteRequest(s)}
                      title="실거래 모드로 전환 (확인 다이얼로그 표시)"
                    >
                      → Real
                    </Button>
                  ) : (
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => onDemote(s.id)}
                      title="paper 모드로 되돌림"
                    >
                      → Paper
                    </Button>
                  )}
                  <Button size="sm" variant="danger" onClick={() => onDelete(s.id)}>Del</Button>
                </div>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
