import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

const MAX_LINES = 200;

/** Per-prefix accent color so [real BUY] vs [pending_tracker] vs [real cycle]
 *  are scannable at a glance. The KST timestamp prefix is stripped before
 *  matching so prefix detection works on the actual log content. */
function classifyLine(line: string): { color: string; tag: string } {
  // Strip leading "HH:MM:SS.mmm KST " (18 chars + space) for prefix detection.
  const body = line.replace(/^\d{2}:\d{2}:\d{2}\.\d{3} KST\s+/, "");
  if (body.startsWith("[real BUY]")) return { color: "text-emerald-300", tag: "BUY" };
  if (body.startsWith("[real SELL]")) return { color: "text-rose-300", tag: "SELL" };
  if (body.startsWith("[real cycle]")) return { color: "text-sky-300", tag: "CYCLE" };
  if (body.startsWith("[realcycle]")) return { color: "text-amber-300", tag: "CYCLE" };
  if (body.startsWith("[pending_tracker]")) return { color: "text-violet-300", tag: "PENDING" };
  if (body.startsWith("[realBUY]") || body.startsWith("[realSELL]"))
    return { color: "text-rose-400", tag: "WARN" };
  if (body.startsWith("[order_executor]")) return { color: "text-yellow-300", tag: "ORDER" };
  return { color: "text-zinc-400", tag: "INFO" };
}

/** Real-time log panel. Subscribes to backend `live:log` events (broadcast
 *  from `live_log!` in file_logger.rs), keeps a 200-line ring buffer, and
 *  auto-scrolls to the bottom unless the user has scrolled up to read older
 *  lines. Lives at the bottom of LiveTradingPage. */
export default function LiveLogPanel() {
  const [lines, setLines] = useState<string[]>([]);
  const [paused, setPaused] = useState(false);
  const [open, setOpen] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const promise = listen<string>("live:log", (e) => {
      setLines((prev) => {
        const next = [...prev, e.payload];
        return next.length > MAX_LINES ? next.slice(-MAX_LINES) : next;
      });
    });
    return () => { promise.then((unlisten) => unlisten()); };
  }, []);

  // Auto-scroll to bottom on new lines unless user paused.
  useEffect(() => {
    if (paused) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines, paused]);

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-xl">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="w-full flex items-center justify-between px-4 py-2.5 text-sm hover:bg-zinc-800/50 rounded-t-xl"
      >
        <div className="flex items-center gap-2">
          <span className="font-medium text-zinc-200">Live log</span>
          <span className="text-xs text-zinc-500">({lines.length} / {MAX_LINES})</span>
          {paused && <span className="text-xs text-amber-400">paused</span>}
        </div>
        <div className="flex items-center gap-2">
          <span
            role="button"
            tabIndex={0}
            onClick={(e) => { e.stopPropagation(); setPaused((v) => !v); }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault(); e.stopPropagation(); setPaused((v) => !v);
              }
            }}
            className="text-xs px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800 text-zinc-300 cursor-pointer select-none"
          >
            {paused ? "Resume" : "Pause"}
          </span>
          <span
            role="button"
            tabIndex={0}
            onClick={(e) => { e.stopPropagation(); setLines([]); }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault(); e.stopPropagation(); setLines([]);
              }
            }}
            className="text-xs px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800 text-zinc-300 cursor-pointer select-none"
          >
            Clear
          </span>
          <span className="text-zinc-500 text-xs">{open ? "▾" : "▸"}</span>
        </div>
      </button>
      {open && (
        <div
          ref={scrollRef}
          className="font-mono text-[11px] leading-relaxed h-64 overflow-y-auto px-4 py-2 border-t border-zinc-800 bg-[#0a0a0c]"
        >
          {lines.length === 0 ? (
            <div className="text-zinc-600 text-center py-12">
              아직 로그가 없습니다. cycle이 정각에 트리거되면 여기에 실시간으로 누적됩니다.
            </div>
          ) : (
            lines.map((line, i) => {
              const { color } = classifyLine(line);
              return <div key={i} className={color}>{line}</div>;
            })
          )}
        </div>
      )}
    </div>
  );
}
