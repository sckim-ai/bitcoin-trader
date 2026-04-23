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
    refreshAll, createSession,
    startSession, stopSession, deleteSession,
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
        <CardHeader className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-zinc-300">Live Trading — Paper Sessions</h3>
          <Button size="sm" onClick={() => setShowNew(true)}>
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

      {presets.length === 0 && (
        <p className="text-xs text-zinc-500 px-2">
          No presets yet. Phase 2/3에서 Optimization 페이지와 연동됩니다.
          당장 테스트하려면 개발자 콘솔에서{" "}
          <code className="text-zinc-400">create_default_preset</code>을 호출하세요.
        </p>
      )}

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
