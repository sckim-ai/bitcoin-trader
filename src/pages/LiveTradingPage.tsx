import { useEffect, useState } from "react";
import { Button } from "../components/ui/Button";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Plus } from "lucide-react";
import SessionTable from "../components/live/SessionTable";
import NewSessionDialog from "../components/live/NewSessionDialog";
import { useLiveTradingStore } from "../stores/liveTradingStore";

export default function LiveTradingPage() {
  const {
    sessions, presets,
    refreshAll, createSession, createDefaultPreset,
    startSession, stopSession, deleteSession,
    subscribeEvents,
  } = useLiveTradingStore();
  const [showNew, setShowNew] = useState(false);
  const [seeding, setSeeding] = useState(false);

  const handleSeedPreset = async (strategyKey: "V3" | "V3.1" | "V5") => {
    setSeeding(true);
    try {
      const ts = new Date().toISOString().slice(11, 19).replace(/:/g, "");
      await createDefaultPreset(`${strategyKey}-default-${ts}`, strategyKey);
    } catch (e) {
      alert(`Preset creation failed: ${e}`);
    } finally {
      setSeeding(false);
    }
  };

  useEffect(() => {
    refreshAll();
    let unlisten: (() => void) | null = null;
    subscribeEvents().then((fn) => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
  }, []);

  return (
    <div className="space-y-4 animate-fade-in">
      <Card>
        <CardHeader className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-zinc-300">
            Presets ({presets.length})
          </h3>
          <div className="flex gap-2">
            <Button size="sm" variant="secondary" disabled={seeding} onClick={() => handleSeedPreset("V3")}>
              + V3 default
            </Button>
            <Button size="sm" variant="secondary" disabled={seeding} onClick={() => handleSeedPreset("V3.1")}>
              + V3.1 default
            </Button>
            <Button size="sm" variant="secondary" disabled={seeding} onClick={() => handleSeedPreset("V5")}>
              + V5 default
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {presets.length === 0 ? (
            <p className="text-xs text-zinc-500">
              프리셋이 없습니다. 위 버튼으로 기본 파라미터 프리셋을 생성하세요.
              Phase 2+에서 Optimization 결과를 프리셋으로 import하는 UI가 추가됩니다.
            </p>
          ) : (
            <ul className="space-y-1 text-xs">
              {presets.map((p) => (
                <li key={p.id} className="text-zinc-400">
                  <span className="text-zinc-200 font-medium">{p.strategy_key}</span>: {p.name}
                  <span className="text-zinc-600 ml-2">({p.source})</span>
                </li>
              ))}
            </ul>
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
