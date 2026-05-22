import { useEffect, useMemo, useState } from "react";
import { Button } from "../components/ui/Button";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Badge } from "../components/ui/Badge";
import { confirmDialog } from "../components/ui/ConfirmDialog";
import { Plus, Trash2, AlertOctagon } from "lucide-react";
import SessionTable from "../components/live/SessionTable";
import NewSessionDialog from "../components/live/NewSessionDialog";
import LiveLogPanel from "../components/live/LiveLogPanel";
import PromoteRealDialog from "../components/live/PromoteRealDialog";
import { NotifyChannelsDialog } from "../components/live/NotifyChannelsDialog";
import PendingOrdersWidget from "../components/live/PendingOrdersWidget";
import ManualOrderCard from "../components/live/ManualOrderCard";
import LiveKpiBar from "../components/live/LiveKpiBar";
import CandleChart from "../components/live/CandleChart";
import { colorFor } from "../components/live/charts/sessionPalette";
import type { LiveSession, LiveTrade, MarketData } from "../types";
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
    toggleSessionMode, emergencyStopAllReal,
    subscribeEvents,
    loadMarketData, loadAllSessionTrades, loadSessionSignals,
  } = useLiveTradingStore();
  const [showNew, setShowNew] = useState(false);
  const [promoteTarget, setPromoteTarget] = useState<LiveSession | null>(null);
  const [notifyTarget, setNotifyTarget] = useState<LiveSession | null>(null);
  const [killBusy, setKillBusy] = useState(false);
  const setSessionNotifyAccountIds = useLiveTradingStore((s) => s.setSessionNotifyAccountIds);

  const realSessionCount = useMemo(
    () => sessions.filter((s) => s.mode === "real").length,
    [sessions],
  );
  const runningRealCount = useMemo(
    () => sessions.filter((s) => s.mode === "real" && s.status === "running").length,
    [sessions],
  );

  const handleKillSwitch = async () => {
    const ok = await confirmDialog({
      title: "Kill switch — 실거래 세션 즉시 정지",
      severity: "danger",
      confirmLabel: "정지",
      body: (
        <div className="space-y-2">
          <p>
            실행 중인{" "}
            <span className="text-rose-400 font-data font-semibold">
              real 세션 {runningRealCount}개
            </span>
            를 즉시 정지합니다.
          </p>
          <p className="text-xs text-zinc-500">
            ⚠ 미체결 주문은 별도로 취소되지 않습니다 (Phase 4A.5 예정).
          </p>
        </div>
      ),
    });
    if (!ok) return;
    setKillBusy(true);
    try {
      const ids = await emergencyStopAllReal();
      if (ids.length > 0) {
        window.alert(`${ids.length}개 real 세션이 정지되었습니다.`);
      } else {
        window.alert("실행 중이던 real 세션이 없었습니다.");
      }
    } catch (e) {
      window.alert(`Emergency stop failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setKillBusy(false);
    }
  };

  const handleDemote = async (id: number) => {
    const ok = await confirmDialog({
      title: "Paper 모드로 되돌리기",
      severity: "warning",
      confirmLabel: "되돌리기",
      body: (
        <p>
          세션 <Badge variant="amber">#{id}</Badge> 을(를) paper 모드로 되돌립니다.
          이후 자동매매는 모의 주문으로만 실행됩니다.
        </p>
      ),
    });
    if (!ok) return;
    try {
      await toggleSessionMode(id, "paper");
    } catch (e) {
      window.alert(`Demote failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

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
                        onClick={async () => {
                          const ok = await confirmDialog({
                            title: "Preset 삭제",
                            severity: "warning",
                            confirmLabel: "삭제",
                            body: (
                              <p>
                                Preset{" "}
                                <span className="text-amber-400 font-data font-semibold">
                                  "{p.name}"
                                </span>{" "}
                                을(를) 삭제합니다.
                              </p>
                            ),
                          });
                          if (ok) deletePreset(p.id);
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

      <PendingOrdersWidget />

      <ManualOrderCard
        sessions={sessions}
        onPlaced={() => {
          // 주문 성공 후 sessions/trades 갱신 — 페이지가 즉시 반영
          loadAllSessionTrades();
        }}
      />

      <Card>
        <CardHeader className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-semibold text-zinc-300">
              Live Trading — Sessions
            </h3>
            {realSessionCount > 0 && (
              <span className="px-1.5 py-0.5 text-[10px] rounded bg-amber-500/20 text-amber-400 border border-amber-500/30">
                {realSessionCount} REAL
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            {runningRealCount > 0 && (
              <Button
                size="sm"
                variant="danger"
                onClick={handleKillSwitch}
                disabled={killBusy}
                title="모든 실거래 세션을 즉시 정지"
              >
                <AlertOctagon size={14} /> Kill switch ({runningRealCount})
              </Button>
            )}
            <Button size="sm" onClick={() => setShowNew(true)} disabled={presets.length === 0}>
              <Plus size={14} /> New Session
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <SessionTable
            sessions={sessions}
            presets={presets}
            ticks={ticks}
            onStart={startSession}
            onStop={stopSession}
            onDelete={async (id) => {
              const ok = await confirmDialog({
                title: "세션 삭제",
                severity: "danger",
                confirmLabel: "삭제",
                body: (
                  <div className="space-y-2">
                    <p>
                      세션 <Badge variant="amber">#{id}</Badge> 을(를) 삭제합니다.
                    </p>
                    <p className="text-xs text-zinc-500">
                      해당 세션의 모든 trade 와 equity 기록이 영구 삭제되며, 되돌릴 수 없습니다.
                    </p>
                  </div>
                ),
              });
              if (ok) deleteSession(id);
            }}
            onPromoteRequest={(s) => setPromoteTarget(s)}
            onDemote={handleDemote}
            onEditNotifyChannels={(s) => setNotifyTarget(s)}
          />
        </CardContent>
      </Card>

      <LiveLogPanel />

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

      {promoteTarget && (
        <PromoteRealDialog
          session={promoteTarget}
          onClose={() => setPromoteTarget(null)}
          onConfirm={async (accountId) => {
            await toggleSessionMode(promoteTarget.id, "real", accountId);
          }}
        />
      )}

      {notifyTarget && (
        <NotifyChannelsDialog
          sessionId={notifyTarget.id}
          sessionLabel={notifyTarget.label}
          initialAccountIds={notifyTarget.notify_account_ids ?? []}
          onClose={() => setNotifyTarget(null)}
          onSave={async (ids) => {
            await setSessionNotifyAccountIds(notifyTarget.id, ids);
          }}
        />
      )}
    </div>
  );
}
