import { useEffect, useState, type ReactNode } from "react";
import { AlertTriangle, Info } from "lucide-react";
import { Button } from "./Button";

type Severity = "info" | "warning" | "danger";

interface ConfirmOptions {
  title: string;
  body?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  severity?: Severity;
}

interface QueuedDialog extends ConfirmOptions {
  id: number;
  resolve: (v: boolean) => void;
}

let nextId = 1;
let pending: QueuedDialog | null = null;
let listener: ((q: QueuedDialog | null) => void) | null = null;

/// Promise-based replacement for window.confirm().
/// Resolves true on confirm, false on cancel/Esc/backdrop click.
/// If a second call comes in while one is open, the previous resolves false.
// eslint-disable-next-line react-refresh/only-export-components
export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    if (pending) pending.resolve(false);
    const q: QueuedDialog = { ...opts, id: nextId++, resolve };
    pending = q;
    listener?.(q);
  });
}

export function ConfirmDialogHost() {
  const [state, setState] = useState<QueuedDialog | null>(null);

  useEffect(() => {
    listener = setState;
    return () => {
      if (listener === setState) listener = null;
    };
  }, []);

  useEffect(() => {
    if (!state) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") finish(false);
      else if (e.key === "Enter") finish(true);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state?.id]);

  const finish = (ok: boolean) => {
    if (!state) return;
    state.resolve(ok);
    if (pending?.id === state.id) pending = null;
    setState(null);
  };

  if (!state) return null;

  const severity = state.severity ?? "info";
  const Icon = severity === "info" ? Info : AlertTriangle;
  const iconColor =
    severity === "danger" ? "text-rose-400" : severity === "warning" ? "text-amber-400" : "text-sky-400";
  const confirmVariant = severity === "danger" ? "danger" : "primary";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) finish(false);
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        className="bg-[#0c0c0f] border border-[#1e1e26] rounded-2xl w-[420px] max-w-[90vw]"
        style={{
          boxShadow:
            "inset 0 1px 0 rgba(255,255,255,0.03), 0 20px 60px -10px rgba(0,0,0,0.6)",
        }}
      >
        <div className="px-5 py-4 border-b border-[#1e1e26] flex items-center gap-2.5">
          <Icon size={18} className={iconColor} />
          <h3 id="confirm-dialog-title" className="text-sm font-semibold text-zinc-100">
            {state.title}
          </h3>
        </div>
        <div className="px-5 py-4 text-sm text-zinc-300">{state.body}</div>
        <div className="px-5 py-3 border-t border-[#1e1e26] flex justify-end gap-2 bg-[#0a0a0d] rounded-b-2xl">
          <Button variant="secondary" size="sm" onClick={() => finish(false)}>
            {state.cancelLabel ?? "취소"}
          </Button>
          <Button autoFocus variant={confirmVariant} size="sm" onClick={() => finish(true)}>
            {state.confirmLabel ?? "확인"}
          </Button>
        </div>
      </div>
    </div>
  );
}
