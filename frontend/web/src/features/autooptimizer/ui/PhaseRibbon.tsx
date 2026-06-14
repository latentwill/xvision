import type { Phase } from "../selectors/buildBoardState";

const PHASES: { key: Phase; label: string }[] = [
  { key: "propose", label: "Propose" },
  { key: "eval", label: "Eval" },
  { key: "gate", label: "Gate" },
  { key: "keep", label: "Keep" },
];

// ORDER maps each Phase to its 0-based index; PHASES[i] corresponds to ORDER[i+1].
// idle=0, propose=1, eval=2, gate=3, keep=4, done=5
const ORDER: Phase[] = ["idle", "propose", "eval", "gate", "keep", "done"];

/**
 * The four-phase progress ribbon for one optimizer cycle.
 *
 * `running` is the live signal: when true the active step pulses, and a cycle
 * that's started but has emitted no phase telemetry yet (phase="idle") reads as
 * "Starting…" rather than "No cycle running". When false the ribbon is static —
 * a finished cycle (phase="done") shows ✓ / "Cycle complete"; a paused cycle
 * freezes on its current step without a pulse.
 */
export function PhaseRibbon({
  phase,
  running = false,
}: {
  phase: Phase;
  running?: boolean;
}) {
  const idx = ORDER.indexOf(phase);
  const allDone = phase === "done";
  // Live, but the cycle hasn't reported a concrete phase yet.
  const starting = running && phase === "idle";
  // Show the "no cycle" caption only when genuinely idle (never mid-run).
  const idleCaption = !running && phase === "idle";

  return (
    <div className="flex items-center gap-2">
      {starting && (
        <span className="shrink-0 inline-flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-widest text-gold">
          <span className="h-1.5 w-1.5 rounded-full bg-gold animate-pulse" aria-hidden />
          Starting…
        </span>
      )}
      {idleCaption && (
        <span className="shrink-0 font-mono text-[10px] uppercase tracking-widest text-text-3">
          No cycle running
        </span>
      )}
      <ol className="flex flex-1 gap-1.5" aria-label="Cycle phases">
        {PHASES.map((p, i) => {
          // Position of this phase in ORDER (1-indexed)
          const pos = i + 1;
          // A phase is done if we're past it, or the cycle is fully done
          const isDone = allDone || pos < idx;
          // A phase is active if it is exactly the current position (and not "done")
          const isActive = pos === idx && !allDone;
          // Pulse the active step only while live — a paused ribbon stays still.
          const isPulsing = isActive && running;
          return (
            <li
              key={p.key}
              aria-current={isActive ? "step" : undefined}
              className={[
                "flex-1 rounded-sm px-2 py-1.5 text-center text-[10px] uppercase tracking-widest transition-colors",
                isActive
                  ? "bg-gold text-on-accent font-semibold"
                  : isDone
                    ? "bg-gold/10 text-gold"
                    : starting
                      ? "bg-surface-elev text-text-3 animate-pulse"
                      : "bg-surface-elev text-text-4",
                isPulsing ? "animate-pulse" : "",
              ].join(" ")}
            >
              {/* ✓ prefix only in the all-done state — completed cycles must read as finished */}
              {allDone ? `✓ ${p.label}` : p.label}
            </li>
          );
        })}
      </ol>
      {allDone && (
        <span className="shrink-0 rounded-sm border border-border-soft px-2 py-1 font-mono text-[10px] uppercase tracking-widest text-text-3">
          Cycle complete
        </span>
      )}
    </div>
  );
}
