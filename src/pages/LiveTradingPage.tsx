import { useEffect, useMemo, useState } from "react";
import { Button } from "../components/ui/Button";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Plus, Trash2 } from "lucide-react";
import SessionTable from "../components/live/SessionTable";
import NewSessionDialog from "../components/live/NewSessionDialog";
import LiveKpiBar from "../components/live/LiveKpiBar";
import CandleChart from "../components/live/CandleChart";
import { colorFor } from "../components/live/charts/sessionPalette";
import type { LiveTrade, MarketData } from "../types";
import { useLiveTradingStore } from "../stores/liveTradingStore";

const defaultRangeStart = (): string => {
  const d = new Date();
  d.setMonth(d.getMonth() - 3);
  return d.toISOString().slice(0, 10);
};

export default function LiveTradingPage() {
  const {
    sessions, presets, ticks,
    marketData, tradesBySession, signalsBySession,
    hiddenSessionIds,
    refreshAll, createSession,
    startSession, stopSession, deleteSession, deletePreset,
    toggleSessionVisibility, setAllSessionsVisible,
    subscribeEvents,
    loadMarketData, loadAllSessionTrades, loadSessionSignals,
  } = useLiveTradingStore();
  const [showNew, setShowNew] = useState(false);

  // Single source of truth for the chart window. Data is filtered to this
  // start before rendering so the chart's auto-fit lands on the user's
  // chosen range.
  const [rangeStart, setRangeStart] = useState<string>(defaultRangeStart);

  // Which session's signal log to render in the strip chart.
  const [stripSessionId, setStripSessionId] = useState<number | null>(null);

  useEffect(() => {
    refreshAll().then(() => {
      loadMarketData();
      loadAllSessionTrades();
    });
    let unlisten: (() => void) | null = null;
    subscribeEvents().then((fn) => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Refresh per-session trades whenever session list changes (id-set proxy).
  const sessionIdsKey = sessions.map(s => s.id).sort((a, b) => a - b).join(",");
  useEffect(() => {
    if (sessions.length > 0) loadAllSessionTrades();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionIdsKey]);

  // Sorted full id list — the chart palette assigns colours by index in this
  // array, so we MUST pass the full list (not the visible subset) to keep
  // colours stable across visibility toggles.
  const sortedSessionIds = useMemo(
    () => sessions.map(s => s.id).slice().sort((a, b) => a - b),
    [sessions],
  );
  const visibleSessions = useMemo(
    () => sessions.filter(s => !hiddenSessionIds.includes(s.id)),
    [sessions, hiddenSessionIds],
  );

  // Default the strip's session to the first VISIBLE one, and track it if
  // the current selection is removed or hidden.
  useEffect(() => {
    if (visibleSessions.length === 0) {
      if (stripSessionId !== null) setStripSessionId(null);
      return;
    }
    if (stripSessionId == null || !visibleSessions.find(s => s.id === stripSessionId)) {
      setStripSessionId(visibleSessions[0].id);
    }
  }, [sessionIdsKey, stripSessionId, visibleSessions]);

  // Re-run the strip's signal log when the picked session or window changes.
  useEffect(() => {
    if (stripSessionId != null) loadSessionSignals(stripSessionId, rangeStart);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stripSessionId, rangeStart]);

  // Derived: market data and trades scoped to the visible window.
  const cutoffMs = useMemo(
    () => new Date(rangeStart + "T00:00:00Z").getTime(),
    [rangeStart],
  );
  const visibleMarketData = useMemo<MarketData[]>(() => {
    if (!marketData) return [];
    return marketData.filter((m) => new Date(m.candle.timestamp).getTime() >= cutoffMs);
  }, [marketData, cutoffMs]);
  const visibleTradesBySession = useMemo<Record<number, LiveTrade[]>>(() => {
    const out: Record<number, LiveTrade[]> = {};
    for (const [sid, trades] of Object.entries(tradesBySession)) {
      out[Number(sid)] = trades.filter((t) => new Date(t.ts).getTime() >= cutoffMs);
    }
    return out;
  }, [tradesBySession, cutoffMs]);

  return (
    <div className="space-y-4 animate-fade-in">
      <LiveKpiBar tick={ticks["KRW-ETH"]} />

      {visibleMarketData.length > 0 && (
        <Card>
          <CardHeader className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-zinc-300">KRW-ETH (1h)</h3>
            <div className="flex items-center gap-2 text-xs text-zinc-400">
              <span>From</span>
              <input
                type="date"
                value={rangeStart}
                onChange={(e) => setRangeStart(e.target.value)}
                className="bg-zinc-800 border border-zinc-700 rounded-md px-2 py-1 text-zinc-200"
              />
              <span className="text-zinc-600">→ now</span>
            </div>
          </CardHeader>
          <CardContent>
            {sessions.length > 0 && (
              <div className="mb-2 flex flex-wrap items-center justify-between gap-2 text-xs">
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="text-zinc-500 mr-1">Show:</span>
                  {sessions.map((s) => {
                    const hidden = hiddenSessionIds.includes(s.id);
                    const color = colorFor(s.id, sortedSessionIds);
                    return (
                      <button
                        key={s.id}
                        onClick={() => toggleSessionVisibility(s.id)}
                        className={`flex items-center gap-1.5 px-2 py-0.5 rounded-md border transition-opacity ${
                          hidden
                            ? "border-zinc-800 bg-zinc-900/40 opacity-40"
                            : "border-zinc-700 bg-zinc-800/60"
                        }`}
                        title={hidden ? "Click to show" : "Click to hide"}
                      >
                        <span
                          className="w-2.5 h-2.5 rounded-sm"
                          style={{ backgroundColor: color }}
                        />
                        <span className="text-zinc-200">{s.label}</span>
                      </button>
                    );
                  })}
                  <span className="mx-1 text-zinc-700">|</span>
                  <button
                    onClick={() => setAllSessionsVisible(true)}
                    className="px-1.5 py-0.5 text-zinc-500 hover:text-zinc-200"
                  >
                    All
                  </button>
                  <button
                    onClick={() => setAllSessionsVisible(false)}
                    className="px-1.5 py-0.5 text-zinc-500 hover:text-zinc-200"
                  >
                    None
                  </button>
                </div>
                {visibleSessions.length > 0 && (
                  <div className="flex items-center gap-2">
                    <span className="text-zinc-500">Signal session</span>
                    <select
                      value={stripSessionId ?? ""}
                      onChange={(e) => setStripSessionId(Number(e.target.value))}
                      className="bg-zinc-800 border border-zinc-700 rounded-md px-2 py-1 text-zinc-200"
                    >
                      {visibleSessions.map((s) => (
                        <option key={s.id} value={s.id}>{s.label}</option>
                      ))}
                    </select>
                  </div>
                )}
              </div>
            )}
            <CandleChart
              marketData={visibleMarketData}
              sessions={sessions}
              tradesBySession={visibleTradesBySession}
              tick={ticks["KRW-ETH"]}
              signals={stripSessionId != null ? (signalsBySession[stripSessionId] ?? []) : []}
              hiddenSessionIds={hiddenSessionIds}
            />
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <h3 className="text-sm font-semibold text-zinc-300">
            Presets ({presets.length})
          </h3>
        </CardHeader>
        <CardContent>
          {presets.length === 0 ? (
            <p className="text-xs text-zinc-500">
              프리셋이 없습니다. <span className="text-zinc-300 font-medium">Simulation 페이지</span>에서
              파라미터를 조정한 뒤 "Save as preset"으로 저장하세요.
            </p>
          ) : (
            <table className="w-full text-xs">
              <thead className="text-zinc-500">
                <tr className="border-b border-zinc-800">
                  <th className="text-left py-1.5">Name</th>
                  <th className="text-left">Strategy</th>
                  <th className="text-left">Market</th>
                  <th className="text-left">Timeframe</th>
                  <th className="text-left">Window</th>
                  <th className="text-left">Source</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {presets.map((p) => (
                  <tr key={p.id} className="border-b border-zinc-900 hover:bg-zinc-900/40">
                    <td className="py-1.5 text-zinc-200 font-medium">{p.name}</td>
                    <td className="text-zinc-400">{p.strategy_key}</td>
                    <td className="text-zinc-400">{p.market ?? "--"}</td>
                    <td className="text-zinc-400">{p.timeframe ?? "--"}</td>
                    <td className="text-zinc-500">
                      {p.since_ts && p.until_ts ? `${p.since_ts} ~ ${p.until_ts}` : "--"}
                    </td>
                    <td className="text-zinc-500">{p.source}</td>
                    <td className="text-right">
                      <Button
                        size="sm"
                        variant="danger"
                        onClick={() => {
                          if (window.confirm(`Delete preset "${p.name}"?`)) {
                            deletePreset(p.id);
                          }
                        }}
                      >
                        <Trash2 size={12} />
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-zinc-300">Live Trading — Paper Sessions</h3>
          <Button size="sm" onClick={() => setShowNew(true)} disabled={presets.length === 0}>
            <Plus size={14} /> New Session
          </Button>
        </CardHeader>
        <CardContent>
          <SessionTable
            sessions={sessions}
            presets={presets}
            ticks={ticks}
            onStart={startSession}
            onStop={stopSession}
            onDelete={(id) => {
              if (window.confirm("Delete this session? All trades and equity history will be removed.")) {
                deleteSession(id);
              }
            }}
          />
        </CardContent>
      </Card>

      {showNew && (
        <NewSessionDialog
          presets={presets}
          onClose={() => setShowNew(false)}
          onSubmit={async (args) => {
            await createSession(args);
            setShowNew(false);
          }}
        />
      )}
    </div>
  );
}
