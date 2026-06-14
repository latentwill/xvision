// frontend/web/src/features/autooptimizer/OptiCapsule.tsx
//
// WS-11a — the floating OPTI trace capsule for the autooptimizer cycle.
//
// Like the live capsule, the optimizer cycle is a single "run" (one cycle at a
// time), so the capsule renders ONE focused cycle row plus a compact list of
// the cycle's phase rows (parent → experiments → gates → honesty → judge →
// flywheel). It shares the floating shell + single-row body with the eval/live
// capsules via `CapsuleShell` / `CapsuleRow`, so all three speak the same
// visual language.
//
// The rows are projected from the EXISTING cycle SSE stream by the OPTI reducer
// (`opti-trace-reducer.ts`) — this component is pure render: it receives the
// already-projected `RunSpan[]` and never opens its own EventSource. The
// `/optimizer` route owns the subscription (via `useCycleEventStream`) and
// feeds it here.

import {
  CapsuleRow,
  CapsuleShell,
  STATUS,
  type EvalCapsuleFocused,
  type EvalCapsuleStatus,
} from "../agent-runs/CapsuleShell";
import { spanColorForSpan } from "../agent-runs/span-colors";
import { optiSpanLabel } from "../agent-runs/trace-labels";
import type { RunSpan } from "@/api/types-agent-runs";

export type OptiCapsuleProps = {
  /**
   * The cycle's projected trace rows (oldest-first) from
   * `projectOptiRows(events)`. Empty while idle.
   */
  rows: RunSpan[];
  /** The active cycle id, or null when idle. */
  cycleId: string | null;
  /** Whether the cycle is currently in-flight (drives the pulsing running tone). */
  running: boolean;
  /** Invoked when the user opens the trace dock (up-chevron). */
  onExpandDock?: () => void;
  /** Optional test hook id. Default `"opti-capsule"`. */
  testId?: string;
};

/** Short cycle tag for the focused row — `cyc·<first 6 of id>`. */
function cycleShort(cycleId: string): string {
  const tail = cycleId.length > 6 ? cycleId.slice(-6) : cycleId;
  return `cyc·${tail}`;
}

/** The display name for a phase row: operator-surface label, never a raw kind. */
function rowLabel(span: RunSpan): string {
  // Gate rows read best as their three-way outcome — Active / Suspect /
  // Rejected — rather than the generic "Gate evaluated" from formatEventLabel.
  // Every other row prefers the richer `span.name` the reducer already stamped
  // (formatEventLabel), falling back to the short OPTI chip label.
  if (span.kind === "opti.gate") {
    return optiSpanLabel(span) ?? span.name ?? span.kind;
  }
  return span.name || optiSpanLabel(span) || span.kind;
}

export function OptiCapsule({
  rows,
  cycleId,
  running,
  onExpandDock,
  testId = "opti-capsule",
}: OptiCapsuleProps) {
  // Idle: no cycle, nothing to show — render nothing (the page's other
  // surfaces own the idle state).
  if (!cycleId || rows.length === 0) return null;

  const cycleRoot = rows.find((r) => r.kind === "opti.cycle") ?? null;
  // Non-root rows render as the cycle's phase lines.
  const phaseRows = rows.filter((r) => r.kind !== "opti.cycle");

  // Status tone: running → pulsing eval tone; a finished cycle that had any
  // rejected/error rows reads warn, otherwise pass. Mirrors the eval/live
  // capsule's terminal-vs-live distinction.
  const hasError = rows.some((r) => r.status === "error");
  const status: EvalCapsuleStatus = running
    ? "eval"
    : hasError
      ? "warn"
      : "pass";

  const borderColor =
    status === "warn" ? "var(--warn)" : "var(--border-strong)";
  const focusedTone = STATUS[status] ?? STATUS.eval;

  // The current phase chip = the most recent non-root row. While running this
  // is the live phase the operator is watching.
  const current = phaseRows.length > 0 ? phaseRows[phaseRows.length - 1] : null;
  const currentColor = current ? spanColorForSpan(current) : null;

  const focused: EvalCapsuleFocused = {
    id: cycleId,
    kind: "opti",
    short: cycleShort(cycleId),
    status,
    spans: phaseRows.length,
    elapsed: "—",
    cost: "—",
    currentSpan:
      current && currentColor
        ? {
            color: currentColor.hex,
            label: currentColor.label,
            name: rowLabel(current),
          }
        : null,
  };

  return (
    <CapsuleShell
      testId={testId}
      tone={status}
      borderColor={borderColor}
      expanded={true}
    >
      {/* Focused cycle row (single cycle — no sibling stack). */}
      <div
        className="flex items-stretch"
        style={{ borderBottom: "1px solid var(--border)" }}
      >
        <CapsuleRow run={focused} focused={true} currentSpan={focused.currentSpan} />

        <div className="flex items-center gap-0.5 pr-1 shrink-0">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onExpandDock && onExpandDock();
            }}
            aria-label="Expand trace dock"
            title="Expand trace dock"
            className="h-7 w-7 flex items-center justify-center text-text-3 hover:text-gold rounded-full"
            style={{ background: "transparent", border: "none", cursor: "pointer" }}
          >
            <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
              <path
                d="M3 10l5-5 5 5"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
              />
            </svg>
          </button>
        </div>
      </div>

      {/* Current-phase chip — the live phase the operator is watching. */}
      {current && currentColor ? (
        <div
          data-testid="opti-capsule-current-phase"
          className="px-3 py-1 flex items-center gap-2 font-mono text-[11px]"
          style={{ background: "var(--surface-card)" }}
        >
          <span
            className={`w-1.5 h-1.5 rounded-full shrink-0 ${running ? "animate-pulse" : ""}`}
            style={{
              background: currentColor.hex,
              boxShadow: `0 0 0 3px ${currentColor.hex}22`,
            }}
          />
          <span
            className="text-[10px] tracking-[0.16em] shrink-0"
            style={{ color: currentColor.hex }}
          >
            {currentColor.label}
          </span>
          <span className="text-text truncate">{rowLabel(current)}</span>
        </div>
      ) : null}

      {/* Phase rows — one compact line per cycle phase. */}
      <div className="flex flex-col" data-testid="opti-capsule-phases">
        <div
          className="px-3 py-1 text-[9px] font-mono tracking-[0.18em] text-text-4 flex items-center justify-between"
          style={{ background: "var(--surface-card)" }}
        >
          <span>CYCLE</span>
          <span className="tabular-nums">{phaseRows.length}</span>
        </div>
        {phaseRows.length === 0 ? (
          <div className="px-3 py-1.5 text-[10px] font-mono tracking-[0.12em] text-text-4">
            no phases yet
          </div>
        ) : (
          phaseRows.map((r, i) => {
            const color = spanColorForSpan(r);
            return (
              <div
                key={r.span_id}
                data-testid="opti-capsule-phase-row"
                className="h-7 px-3 flex items-center gap-2 font-mono text-[11px] whitespace-nowrap"
                style={{ borderTop: i === 0 ? "none" : "1px solid var(--border-soft)" }}
              >
                <span
                  className="w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ background: color.hex }}
                />
                <span
                  className="text-[9px] tracking-[0.14em] shrink-0"
                  style={{ color: color.hex }}
                >
                  {color.label}
                </span>
                <span className="text-text-2 truncate">{rowLabel(r)}</span>
                {typeof r.attributes.delta_day === "number" ? (
                  <span className="text-text-4 tabular-nums ml-auto shrink-0">
                    ΔSharpe {(r.attributes.delta_day as number).toFixed(2)}
                  </span>
                ) : null}
              </div>
            );
          })
        )}
      </div>

      {/* a11y: surface the cycle root + tone for screen readers. */}
      <span className="sr-only">
        {cycleRoot ? cycleRoot.name : "Optimizer cycle"} · {focusedTone.label}
      </span>
    </CapsuleShell>
  );
}
