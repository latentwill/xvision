import { useTraceDock } from "@/stores/trace-dock";

export function TopbarModeToggle() {
  const { mode, setActiveRun, activeRunId } = useTraceDock();
  if (!activeRunId) return null;
  const isLive = mode === "live";
  const set = (next: "live" | "post-hoc") => setActiveRun(activeRunId, next);
  return (
    <div
      data-testid="topbar-mode-toggle"
      className="flex items-center gap-1 p-0.5"
      style={{ background: "var(--surface-elev)", border: "1px solid var(--border)", borderRadius: 4 }}
    >
      <button
        type="button"
        aria-pressed={!isLive}
        onClick={() => set("post-hoc")}
        className="h-6 px-2.5 text-[10px] font-mono tracking-[0.16em]"
        style={{
          background: !isLive ? "var(--gold-bg)" : "transparent",
          color: !isLive ? "var(--gold)" : "var(--text-3)",
          borderRadius: 4,
        }}
      >
        POST-HOC
      </button>
      <span className="text-text-4 text-[10px] px-0.5">⇄</span>
      <button
        type="button"
        aria-pressed={isLive}
        onClick={() => set("live")}
        className="h-6 px-2.5 text-[10px] font-mono tracking-[0.16em] flex items-center gap-1.5"
        style={{
          background: isLive ? "rgba(111,143,184,0.18)" : "transparent",
          color: isLive ? "#bcd1ea" : "var(--text-3)",
          border: isLive ? "1px solid rgba(111,143,184,0.45)" : "1px solid transparent",
          borderRadius: 4,
        }}
      >
        {isLive ? <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: "var(--info)" }} /> : null}
        LIVE
      </button>
    </div>
  );
}
