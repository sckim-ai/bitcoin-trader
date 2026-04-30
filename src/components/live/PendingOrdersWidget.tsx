import { useEffect, useState } from "react";
import { Clock, ChevronDown, ChevronRight } from "lucide-react";
import { listPendingOrders, type PendingOrderRow } from "../../lib/live";
import { Card, CardContent, CardHeader } from "../ui/Card";

/// Polls list_pending_orders periodically. In steady operation the list is
/// empty (orders fill in <1s and the cycle re-pegs every hour). Non-empty
/// rows usually indicate an Upbit outage or a price gap big enough that the
/// limit didn't fill — the user can use this to decide whether to manually
/// cancel via Upbit or wait.
export default function PendingOrdersWidget() {
  const [orders, setOrders] = useState<PendingOrderRow[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = async () => {
    try {
      const rows = await listPendingOrders();
      setOrders(rows);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    reload();
    const t = setInterval(reload, 30_000); // 30s — pending state changes slowly
    return () => clearInterval(t);
  }, []);

  if (orders.length === 0 && !error) return null; // hidden when nothing pending

  return (
    <Card>
      <CardHeader>
        <button
          type="button"
          className="w-full flex items-center justify-between cursor-pointer hover:bg-zinc-900/40 transition-colors -m-1 p-1 rounded"
          onClick={() => setExpanded((e) => !e)}
        >
          <div className="flex items-center gap-2">
            {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            <Clock size={14} className="text-amber-500" />
            <h3 className="text-sm font-semibold text-zinc-300">
              Pending Orders ({orders.length})
            </h3>
            {error && <span className="text-xs text-rose-400 ml-2">{error}</span>}
          </div>
          <span className="text-[10px] text-zinc-500">auto-refresh 30s · cancel on next cycle</span>
        </button>
      </CardHeader>
      {expanded && (
        <CardContent>
          {orders.length === 0 ? (
            <p className="text-xs text-zinc-500">No pending orders.</p>
          ) : (
            <table className="w-full text-xs [&_th]:px-2 [&_td]:px-2">
              <thead className="text-zinc-500">
                <tr className="border-b border-zinc-800">
                  <th className="text-left py-1.5">UUID</th>
                  <th className="text-left">Session</th>
                  <th className="text-left">Side</th>
                  <th className="text-right">Target</th>
                  <th className="text-right">Requested</th>
                  <th className="text-left">Placed</th>
                  <th className="text-left">Last check</th>
                </tr>
              </thead>
              <tbody>
                {orders.map((o) => (
                  <tr key={o.uuid} className="border-b border-zinc-900">
                    <td className="py-1 font-data text-zinc-400">{o.uuid.slice(0, 8)}…</td>
                    <td className="text-zinc-400">#{o.session_id}</td>
                    <td className={o.side === "bid" ? "text-emerald-400" : "text-rose-400"}>
                      {o.side === "bid" ? "buy" : "sell"}
                    </td>
                    <td className="text-right font-data text-zinc-300">
                      {o.target_price != null ? Math.round(o.target_price).toLocaleString() : "—"}
                    </td>
                    <td className="text-right font-data text-zinc-300">
                      {o.side === "bid"
                        ? `${Math.round(o.requested).toLocaleString()} KRW`
                        : o.requested.toFixed(8)}
                    </td>
                    <td className="text-zinc-500 font-data">{o.placed_at.slice(11, 19)}</td>
                    <td className="text-zinc-600 font-data">
                      {o.last_checked ? o.last_checked.slice(11, 19) : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </CardContent>
      )}
    </Card>
  );
}
