import { Button } from "../ui/Button";
import { Badge } from "../ui/Badge";
import type { LiveSession, TickData } from "../../types";
import { deriveSessionPnl } from "../../stores/liveTradingStore";

interface Props {
  sessions: LiveSession[];
  ticks: Record<string, TickData>;
  onStart: (id: number) => void;
  onStop: (id: number) => void;
  onDelete: (id: number) => void;
}

export default function SessionTable({ sessions, ticks, onStart, onStop, onDelete }: Props) {
  if (sessions.length === 0) {
    return <p className="text-zinc-500 text-sm">No sessions yet. Create one to start.</p>;
  }
  return (
    <table className="w-full text-sm">
      <thead className="text-zinc-400 text-xs">
        <tr className="border-b border-zinc-800">
          <th className="text-left py-2">Label</th>
          <th className="text-left">Market</th>
          <th className="text-left">Status</th>
          <th className="text-left">Position</th>
          <th className="text-right">Equity</th>
          <th className="text-right">Total P/L</th>
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
          const totalColor = derived.pnlPctSinceStart > 0 ? "text-emerald-400"
            : derived.pnlPctSinceStart < 0 ? "text-rose-400" : "text-zinc-400";
          const unrColor = derived.unrealizedPnlPct > 0 ? "text-emerald-400"
            : derived.unrealizedPnlPct < 0 ? "text-rose-400" : "text-zinc-500";
          return (
            <tr key={s.id} className="border-b border-zinc-900 hover:bg-zinc-900/40">
              <td className="py-2 font-medium text-zinc-200">{s.label}</td>
              <td className="text-zinc-400">{s.market}</td>
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
              <td className={`text-right font-data ${totalColor}`}>
                {derived.pnlPctSinceStart.toFixed(2)}%
              </td>
              <td className={`text-right font-data ${unrColor}`}>
                {s.current_position === "holding"
                  ? `${derived.unrealizedPnlPct.toFixed(2)}%`
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
