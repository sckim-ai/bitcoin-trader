import { useEffect, useMemo, useState } from "react";
import { History as HistoryIcon, Download, RefreshCw } from "lucide-react";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Input } from "../components/ui/Input";
import {
  listRealTrades,
  realPnlSummary,
  exportRealTradesCsv,
  type HistoryFilter,
  type HistoryTrade,
  type DailyBucket,
} from "../lib/api";

/// 90일 전 날짜 (YYYY-MM-DD).
const ninetyDaysAgo = (): string => {
  const d = new Date();
  d.setDate(d.getDate() - 90);
  return d.toISOString().slice(0, 10);
};

export default function HistoryPage() {
  const [since, setSince] = useState(ninetyDaysAgo);
  const [until, setUntil] = useState<string>("");
  const [trades, setTrades] = useState<HistoryTrade[]>([]);
  const [daily, setDaily] = useState<DailyBucket[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const filter: HistoryFilter = useMemo(
    () => ({
      since: since || undefined,
      until: until || undefined,
    }),
    [since, until],
  );

  const reload = async () => {
    setLoading(true);
    setError(null);
    try {
      const [t, d] = await Promise.all([
        listRealTrades(filter),
        realPnlSummary(filter),
      ]);
      setTrades(t);
      setDaily(d);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleExport = async () => {
    try {
      const csv = await exportRealTradesCsv(filter);
      const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `real-trades-${new Date().toISOString().slice(0, 10)}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setError(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // KPI summary (totals across the filtered window).
  const totalRealizedPnl = daily.reduce((acc, d) => acc + d.realized_pnl, 0);
  const totalTradeCount = daily.reduce((acc, d) => acc + d.trade_count, 0);
  const profitDays = daily.filter((d) => d.realized_pnl > 0).length;
  const lossDays = daily.filter((d) => d.realized_pnl < 0).length;

  // Bar chart (simple SVG): one bar per day, height ∝ |realized_pnl|.
  const maxAbs = Math.max(1, ...daily.map((d) => Math.abs(d.realized_pnl)));
  const chartHeight = 120;

  return (
    <div className="space-y-4 animate-fade-in">
      <h1 className="text-xl font-semibold text-zinc-100 flex items-center gap-2">
        <HistoryIcon size={22} className="text-zinc-500" />
        Trading History (Real)
      </h1>

      {/* Filter bar */}
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <div>
            <label className="block text-xs text-zinc-500 mb-1">Since</label>
            <Input type="date" value={since} onChange={(e) => setSince(e.target.value)} />
          </div>
          <div>
            <label className="block text-xs text-zinc-500 mb-1">Until</label>
            <Input
              type="date"
              value={until}
              onChange={(e) => setUntil(e.target.value)}
              placeholder="(open)"
            />
          </div>
          <Button onClick={reload} disabled={loading}>
            <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
            Reload
          </Button>
          <Button onClick={handleExport} variant="secondary" disabled={loading || trades.length === 0}>
            <Download size={14} /> CSV
          </Button>
          {error && <span className="text-rose-400 text-xs ml-2">{error}</span>}
        </CardContent>
      </Card>

      {/* Summary KPIs */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <KpiCell label="Realized P/L (KRW)" value={totalRealizedPnl.toLocaleString(undefined, { maximumFractionDigits: 0 })} tone={totalRealizedPnl > 0 ? "good" : totalRealizedPnl < 0 ? "bad" : "neutral"} />
        <KpiCell label="Sells" value={`${totalTradeCount}`} tone="neutral" />
        <KpiCell label="Profit days" value={`${profitDays}`} tone="good" />
        <KpiCell label="Loss days" value={`${lossDays}`} tone="bad" />
      </div>

      {/* Daily P/L bar chart (oldest left → newest right) */}
      <Card>
        <CardHeader>
          <h2 className="text-sm font-semibold text-zinc-300">Daily realized P/L</h2>
        </CardHeader>
        <CardContent>
          {daily.length === 0 ? (
            <p className="text-zinc-500 text-sm">기간 내 거래가 없습니다.</p>
          ) : (
            <svg viewBox={`0 0 ${Math.max(daily.length * 18, 200)} ${chartHeight + 30}`} className="w-full">
              {[...daily].reverse().map((d, i) => {
                const h = (Math.abs(d.realized_pnl) / maxAbs) * chartHeight;
                const isPos = d.realized_pnl > 0;
                const x = i * 18;
                const y = isPos ? chartHeight - h : chartHeight;
                return (
                  <g key={d.date}>
                    <rect
                      x={x}
                      y={y}
                      width={14}
                      height={Math.max(h, 1)}
                      fill={isPos ? "#3b82f6" : d.realized_pnl < 0 ? "#f43f5e" : "#52525b"}
                    >
                      <title>{`${d.date}: ${d.realized_pnl.toLocaleString(undefined, { maximumFractionDigits: 0 })} KRW (${d.trade_count} trades)`}</title>
                    </rect>
                    {i % 5 === 0 && (
                      <text
                        x={x}
                        y={chartHeight + 12}
                        fill="#71717a"
                        fontSize="9"
                        transform={`rotate(45, ${x}, ${chartHeight + 12})`}
                      >
                        {d.date.slice(5)}
                      </text>
                    )}
                  </g>
                );
              })}
              {/* Zero baseline */}
              <line x1="0" y1={chartHeight} x2={Math.max(daily.length * 18, 200)} y2={chartHeight} stroke="#3f3f46" strokeWidth="1" strokeDasharray="2,2" />
            </svg>
          )}
        </CardContent>
      </Card>

      {/* Trades table */}
      <Card>
        <CardHeader>
          <h2 className="text-sm font-semibold text-zinc-300">
            Trades ({trades.length}) — newest first
          </h2>
        </CardHeader>
        <CardContent>
          {trades.length === 0 ? (
            <p className="text-zinc-500 text-sm">기간 내 거래가 없습니다.</p>
          ) : (
            <div className="overflow-x-auto max-h-[60vh] overflow-y-auto">
              <table className="w-full text-xs [&_th]:px-3 [&_td]:px-3">
                <thead className="text-zinc-500 sticky top-0 bg-zinc-950">
                  <tr className="border-b border-zinc-800">
                    <th className="text-left py-2">Time (UTC)</th>
                    <th className="text-left">Session</th>
                    <th className="text-left">Side</th>
                    <th className="text-right">Price</th>
                    <th className="text-right">Volume</th>
                    <th className="text-right">P/L</th>
                    <th className="text-right">P/L %</th>
                    <th className="text-left">Signal</th>
                  </tr>
                </thead>
                <tbody>
                  {trades.map((t) => (
                    <tr key={t.id} className="border-b border-zinc-900 hover:bg-zinc-900/40">
                      <td className="py-1 font-data text-zinc-300">{t.ts.slice(0, 19).replace("T", " ")}</td>
                      <td className="text-zinc-400">#{t.session_id}</td>
                      <td className={t.side === "buy" ? "text-emerald-400" : "text-rose-400"}>
                        {t.side}
                      </td>
                      <td className="text-right font-data text-zinc-200">{Math.round(t.price).toLocaleString()}</td>
                      <td className="text-right font-data text-zinc-300">{t.volume.toFixed(8)}</td>
                      <td className={`text-right font-data ${t.pnl == null ? "text-zinc-600" : t.pnl > 0 ? "text-emerald-400" : t.pnl < 0 ? "text-rose-400" : "text-zinc-400"}`}>
                        {t.pnl != null ? Math.round(t.pnl).toLocaleString() : "—"}
                      </td>
                      <td className={`text-right font-data ${t.pnl_pct == null ? "text-zinc-600" : t.pnl_pct > 0 ? "text-emerald-400" : t.pnl_pct < 0 ? "text-rose-400" : "text-zinc-400"}`}>
                        {t.pnl_pct != null ? `${t.pnl_pct >= 0 ? "+" : ""}${t.pnl_pct.toFixed(2)}%` : "—"}
                      </td>
                      <td className="text-zinc-500">{t.signal}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function KpiCell({ label, value, tone }: { label: string; value: string; tone: "good" | "bad" | "neutral" }) {
  const color = tone === "good" ? "text-emerald-400" : tone === "bad" ? "text-rose-400" : "text-zinc-200";
  return (
    <Card>
      <CardContent>
        <div className="text-[11px] text-zinc-500">{label}</div>
        <div className={`mt-1 text-lg font-semibold font-data ${color}`}>{value}</div>
      </CardContent>
    </Card>
  );
}
