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
import { listSessions } from "../lib/live";
import type { LiveSession } from "../types";

/// 90일 전 날짜 (YYYY-MM-DD).
const ninetyDaysAgo = (): string => {
  const d = new Date();
  d.setDate(d.getDate() - 90);
  return d.toISOString().slice(0, 10);
};

/// UTC 타임스탬프를 KST 표시 문자열로 변환. 백엔드 ts는 항상 UTC RFC3339.
/// 한국 사용자가 본인 시간대에서 거래 시각을 직관적으로 읽을 수 있도록.
function formatKst(ts: string): string {
  const d = new Date(ts);
  if (isNaN(d.getTime())) return ts;
  return d.toLocaleString("ko-KR", {
    timeZone: "Asia/Seoul",
    year: "numeric", month: "2-digit", day: "2-digit",
    hour: "2-digit", minute: "2-digit", second: "2-digit",
    hour12: false,
  }).replace(/\. /g, "-").replace(".", "");
}

/// signal 컬럼을 사용자 친화 라벨로. 백엔드의 정확한 값(`real_buy_late` 등)은
/// CSV에 그대로 보존되고, 화면에서만 짧고 읽기 쉽게 변환.
function formatSignal(signal: string): string {
  switch (signal) {
    case "real_buy": return "buy";
    case "real_sell": return "sell";
    case "real_buy_late": return "buy (late)";
    case "real_sell_late": return "sell (late)";
    case "manual_buy": return "buy (manual)";
    case "manual_sell": return "sell (manual)";
    case "manual_buy_limit": return "buy (manual, limit)";
    case "manual_sell_limit": return "sell (manual, limit)";
    default: return signal;
  }
}

export default function HistoryPage() {
  const [since, setSince] = useState(ninetyDaysAgo);
  const [until, setUntil] = useState<string>("");
  const [sessionFilter, setSessionFilter] = useState<number | "all">("all");
  const [trades, setTrades] = useState<HistoryTrade[]>([]);
  const [daily, setDaily] = useState<DailyBucket[]>([]);
  const [sessions, setSessions] = useState<LiveSession[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /// session_id → label 매핑. SessionTable의 `#N`만 표시되던 문제를 해소.
  const sessionLabel = useMemo(() => {
    const m = new Map<number, string>();
    for (const s of sessions) m.set(s.id, s.label);
    return m;
  }, [sessions]);

  const filter: HistoryFilter = useMemo(
    () => ({
      session_id: sessionFilter === "all" ? undefined : sessionFilter,
      since: since || undefined,
      until: until || undefined,
    }),
    [sessionFilter, since, until],
  );

  const reload = async () => {
    setLoading(true);
    setError(null);
    try {
      const [t, d, s] = await Promise.all([
        listRealTrades(filter),
        realPnlSummary(filter),
        listSessions(),
      ]);
      setTrades(t);
      setDaily(d);
      setSessions(s);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  // Initial load + filter changes.
  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filter]);

  // Auto-refresh on session:update — same Tauri channel SessionTable subscribes to.
  // When a real cycle ends, this page silently re-queries so newly booked
  // trades show up without a manual Reload click.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    if ("__TAURI_INTERNALS__" in window) {
      (async () => {
        const { listen } = await import("@tauri-apps/api/event");
        const u = await listen("session:update", () => {
          // Best-effort silent refresh; never block the user's current action.
          reload();
        });
        unlisten = u;
      })();
    }
    return () => { if (unlisten) unlisten(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filter]);

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
  const totalFee = trades.reduce((acc, t) => acc + t.fee, 0);

  // Bar chart (simple SVG): one bar per day, height ∝ |realized_pnl|.
  const maxAbs = Math.max(1, ...daily.map((d) => Math.abs(d.realized_pnl)));
  const chartHeight = 120;

  // since~until 사이의 모든 UTC 일자를 채워서 차트의 시간축을 보존한다.
  // 백엔드 `real_pnl_summary`의 `GROUP BY day`는 거래 있는 일자만 row를 만들기
  // 때문에, 차트가 그 row만 18 unit 간격으로 깔면 sell이 1일뿐일 때 막대 1개만
  // 좌측 끝에 그려지고 90일 시간 흐름이 사라진다. KPI 합산은 daily 그대로 사용.
  const fullDaily = useMemo<DailyBucket[]>(() => {
    if (daily.length === 0) return [];
    const map = new Map(daily.map((d) => [d.date, d]));
    const startStr = since || ninetyDaysAgo();
    const endStr = until || new Date().toISOString().slice(0, 10);
    const startMs = Date.parse(`${startStr}T00:00:00Z`);
    const endMs = Date.parse(`${endStr}T00:00:00Z`);
    if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs < startMs) {
      return daily;
    }
    const out: DailyBucket[] = [];
    for (let t = startMs; t <= endMs; t += 86400000) {
      const key = new Date(t).toISOString().slice(0, 10);
      out.push(map.get(key) ?? { date: key, trade_count: 0, realized_pnl: 0, avg_pnl_pct: 0 });
    }
    return out;
  }, [daily, since, until]);

  // X축 일자 라벨은 차트 폭에 비례해 12개 정도만 표시 (90일이면 매 8일).
  const labelInterval = Math.max(1, Math.ceil(fullDaily.length / 12));

  // 막대 끝 위 KRW 값 라벨용 단축 포맷: 1,667,545 → "+1.67M", -123,000 → "-123k".
  const formatKrwShort = (v: number): string => {
    if (v === 0) return "";
    const sign = v > 0 ? "+" : "-";
    const abs = Math.abs(v);
    if (abs >= 1_000_000) return `${sign}${(abs / 1_000_000).toFixed(2)}M`;
    if (abs >= 1_000) return `${sign}${Math.round(abs / 1_000)}k`;
    return `${sign}${Math.round(abs)}`;
  };

  // Real-mode sessions only (REAL or formerly REAL — paper sessions don't
  // produce is_real=1 trades). Show ALL sessions in the filter for
  // completeness, but mark current real with an asterisk.
  const filterSessionOptions = sessions
    .slice()
    .sort((a, b) => a.id - b.id);

  return (
    <div className="space-y-4 animate-fade-in">
      <h1 className="text-xl font-semibold text-zinc-100 flex items-center gap-2">
        <HistoryIcon size={22} className="text-zinc-500" />
        Trading History (Real)
        <span className="text-xs text-zinc-500 font-normal ml-2">시간대: KST (Asia/Seoul)</span>
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
          <div>
            <label className="block text-xs text-zinc-500 mb-1">Session</label>
            <select
              value={sessionFilter}
              onChange={(e) => setSessionFilter(e.target.value === "all" ? "all" : Number(e.target.value))}
              className="bg-zinc-800 border border-zinc-700 rounded-md px-3 py-2 text-sm text-zinc-200"
            >
              <option value="all">All</option>
              {filterSessionOptions.map((s) => (
                <option key={s.id} value={s.id}>
                  #{s.id} {s.label}{s.mode === "real" ? " ★" : ""}
                </option>
              ))}
            </select>
          </div>
          <Button onClick={reload} disabled={loading}>
            <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
            Reload
          </Button>
          <Button onClick={handleExport} variant="secondary" disabled={loading || trades.length === 0}>
            <Download size={14} /> CSV
          </Button>
          {error && <span className="text-rose-400 text-xs ml-2 break-all">{error}</span>}
        </CardContent>
      </Card>

      {/* Summary KPIs */}
      <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
        <KpiCell label="Realized P/L (KRW)" value={totalRealizedPnl.toLocaleString(undefined, { maximumFractionDigits: 0 })} tone={totalRealizedPnl > 0 ? "good" : totalRealizedPnl < 0 ? "bad" : "neutral"} />
        <KpiCell label="Sells" value={`${totalTradeCount}`} tone="neutral" />
        <KpiCell label="Profit days" value={`${profitDays}`} tone="good" />
        <KpiCell label="Loss days" value={`${lossDays}`} tone="bad" />
        <KpiCell label="Total fee (KRW)" value={Math.round(totalFee).toLocaleString()} tone="neutral" />
      </div>

      {/* Daily P/L bar chart (oldest left → newest right) */}
      <Card>
        <CardHeader>
          <h2 className="text-sm font-semibold text-zinc-300">
            Daily realized P/L <span className="text-zinc-500 text-xs font-normal">(UTC 일자 기준)</span>
            {daily.length > 0 && (
              <span className="text-zinc-500 text-xs font-normal ml-2">
                · 막대 최대치 ±{Math.round(maxAbs).toLocaleString()} KRW
              </span>
            )}
          </h2>
        </CardHeader>
        <CardContent>
          {daily.length === 0 ? (
            <p className="text-zinc-500 text-sm">기간 내 거래가 없습니다.</p>
          ) : (
            // viewBox width의 min을 카드 폭(~1200px)에 가깝게 둔다. w-full +
            // height 미지정이면 브라우저는 viewBox aspect ratio를 유지한 채
            // 카드 폭에 맞춰 SVG height를 산출하므로, viewBox가 정사각에 가까우면
            // (데이터 1건일 때 200×150) SVG가 ~900px 높이로 거대화된다.
            <svg viewBox={`0 0 ${Math.max(fullDaily.length * 18, 1200)} ${chartHeight + 30}`} className="w-full">
              {fullDaily.map((d, i) => {
                const h = (Math.abs(d.realized_pnl) / maxAbs) * chartHeight;
                const isPos = d.realized_pnl > 0;
                const isNeg = d.realized_pnl < 0;
                const x = i * 18;
                const y = isPos ? chartHeight - h : chartHeight;
                return (
                  <g key={d.date}>
                    <rect
                      x={x}
                      y={y}
                      width={14}
                      height={Math.max(h, 1)}
                      fill={isPos ? "#3b82f6" : isNeg ? "#f43f5e" : "#3f3f46"}
                    >
                      <title>{`${d.date}: ${d.realized_pnl.toLocaleString(undefined, { maximumFractionDigits: 0 })} KRW (${d.trade_count} trades)`}</title>
                    </rect>
                    {/* 거래 있는 일자만 막대 끝 위에 KRW 값 라벨 — 양수는 막대 위,
                        음수는 0선 직하 (막대 길이 무관, X축 라벨과 겹치지 않게
                        chartHeight + 10 고정). hover title은 풀자릿수 보존. */}
                    {d.realized_pnl !== 0 && (
                      <text
                        x={x + 7}
                        y={isPos ? Math.max(y - 3, 8) : chartHeight + 10}
                        textAnchor="middle"
                        fontSize="9"
                        fill={isPos ? "#93c5fd" : "#fda4af"}
                      >
                        {formatKrwShort(d.realized_pnl)}
                      </text>
                    )}
                    {i % labelInterval === 0 && (
                      <text
                        x={x}
                        y={chartHeight + 22}
                        fill="#71717a"
                        fontSize="9"
                        transform={`rotate(45, ${x}, ${chartHeight + 22})`}
                      >
                        {d.date.slice(5)}
                      </text>
                    )}
                  </g>
                );
              })}
              <line x1="0" y1={chartHeight} x2={Math.max(fullDaily.length * 18, 1200)} y2={chartHeight} stroke="#52525b" strokeWidth="1" />
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
                    <th className="text-left py-2">Time (KST)</th>
                    <th className="text-left">Session</th>
                    <th className="text-left">Side</th>
                    <th className="text-right">Price</th>
                    <th className="text-right">Volume</th>
                    <th className="text-right">Fee</th>
                    <th className="text-right">P/L</th>
                    <th className="text-right">P/L %</th>
                    <th className="text-left">Signal</th>
                  </tr>
                </thead>
                <tbody>
                  {trades.map((t) => (
                    <tr key={t.id} className="border-b border-zinc-900 hover:bg-zinc-900/40">
                      <td className="py-1 font-data text-zinc-300 whitespace-nowrap">{formatKst(t.ts)}</td>
                      <td className="text-zinc-400 whitespace-nowrap">
                        <span className="text-zinc-600">#{t.session_id}</span>{" "}
                        {sessionLabel.get(t.session_id) ?? <span className="text-zinc-700">(deleted)</span>}
                      </td>
                      <td className={t.side === "buy" ? "text-emerald-400" : "text-rose-400"}>
                        {t.side}
                      </td>
                      <td className="text-right font-data text-zinc-200">{Math.round(t.price).toLocaleString()}</td>
                      <td className="text-right font-data text-zinc-300">{t.volume.toFixed(8)}</td>
                      <td className="text-right font-data text-zinc-500">{Math.round(t.fee).toLocaleString()}</td>
                      <td className={`text-right font-data ${t.pnl == null ? "text-zinc-600" : t.pnl > 0 ? "text-emerald-400" : t.pnl < 0 ? "text-rose-400" : "text-zinc-400"}`}>
                        {t.pnl != null ? Math.round(t.pnl).toLocaleString() : "—"}
                      </td>
                      <td className={`text-right font-data ${t.pnl_pct == null ? "text-zinc-600" : t.pnl_pct > 0 ? "text-emerald-400" : t.pnl_pct < 0 ? "text-rose-400" : "text-zinc-400"}`}>
                        {t.pnl_pct != null ? `${t.pnl_pct >= 0 ? "+" : ""}${(t.pnl_pct * 100).toFixed(2)}%` : "—"}
                      </td>
                      <td className="text-zinc-500">{formatSignal(t.signal)}</td>
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
