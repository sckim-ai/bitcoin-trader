import { useEffect, useState } from "react";
import { Button } from "../components/ui/Button";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Plus, Trash2 } from "lucide-react";
import SessionTable from "../components/live/SessionTable";
import NewSessionDialog from "../components/live/NewSessionDialog";
import { useLiveTradingStore } from "../stores/liveTradingStore";

export default function LiveTradingPage() {
  const {
    sessions, presets,
    refreshAll, createSession,
    startSession, stopSession, deleteSession, deletePreset,
    subscribeEvents,
  } = useLiveTradingStore();
  const [showNew, setShowNew] = useState(false);

  useEffect(() => {
    refreshAll();
    let unlisten: (() => void) | null = null;
    subscribeEvents().then((fn) => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
  }, []);

  return (
    <div className="space-y-4 animate-fade-in">
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
