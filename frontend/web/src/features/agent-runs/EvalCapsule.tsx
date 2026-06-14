// frontend/web/src/features/agent-runs/EvalCapsule.tsx
//
// Ported from docs/design/Capsule · Multi-Eval.html.
//
// Single floating capsule for an in-flight eval. When other evals are running
// concurrently on the cluster, the capsule grows downward into a stack — one
// row per eval, formatted identically. Click a non-focused row to switch which
// eval is inspected. Errored siblings auto-promote to the top and force the
// capsule open.
//
// The floating container chrome and the single-row body now live in the shared
// `CapsuleShell` module (so the dedicated LiveCapsule shares the same visual
// language). This file composes those primitives; its rendered output is
// unchanged — the EvalCapsule + StripDockSlot tests are the regression gate.

import { useEffect, useRef, useState } from "react";
import type { RetentionMode } from "../../api/types-agent-runs";
import {
  CapsuleRow,
  CapsuleShell,
  STATUS,
  type EvalCapsuleCurrentSpan,
  type EvalCapsuleFocused,
  type EvalCapsuleRow,
  type EvalCapsuleStatus,
} from "./CapsuleShell";

// Re-export the shared capsule types from their original module path so the
// existing call sites (StripDockSlot et al.) that import them from
// `./EvalCapsule` keep working unchanged.
export type {
  EvalCapsuleCurrentSpan,
  EvalCapsuleFocused,
  EvalCapsuleRow,
  EvalCapsuleStatus,
};

export type EvalCapsuleProps = {
  focused: EvalCapsuleFocused;
  /**
   * Other in-flight evals on the cluster. Each row renders identically to the
   * focused row. Errored siblings auto-promote to the top of the stack and
   * cause the capsule to expand.
   */
  siblings?: EvalCapsuleRow[];
  /** Invoked when a non-focused sibling row is clicked. */
  onSwitchFocus?: (run: EvalCapsuleRow) => void;
  /** Invoked when the user opens the trace dock (up-chevron). */
  onExpandDock?: () => void;
  /** Invoked when the user opens the focused eval's dedicated route. */
  onPopOut?: () => void;
  /**
   * Optional test hook id. Default `"run-status-strip"` preserves backwards
   * compatibility with existing selectors that targeted the legacy strip.
   */
  testId?: string;
  /**
   * Focused run's retention/fidelity (`AgentRunSummary.retention_mode`).
   * Forwarded to the focused `CapsuleRow` so the operator sees whether bodies
   * are present. Optional — omitted on legacy call sites.
   */
  retentionMode?: RetentionMode;
};

export function EvalCapsule({
  focused,
  siblings = [],
  onSwitchFocus,
  onExpandDock,
  onPopOut,
  testId = "run-status-strip",
  retentionMode,
}: EvalCapsuleProps) {
  // Errored siblings always promoted to the top of the stack.
  const errored = siblings.filter((s) => s.status === "error");
  const others = siblings.filter((s) => s.status !== "error");
  const ordered = [...errored, ...others];

  const [expanded, setExpanded] = useState(false);

  // Auto-open the capsule when a NEW error appears (assertive — right call for
  // trading). Manual collapse is respected until a new error fires.
  const lastErrorCount = useRef(errored.length);
  useEffect(() => {
    if (errored.length > lastErrorCount.current) setExpanded(true);
    lastErrorCount.current = errored.length;
  }, [errored.length]);

  const anyError = ordered.some((s) => s.status === "error") || focused.status === "error";
  const anyWarn = ordered.some((s) => s.status === "warn") || focused.status === "warn";
  const borderColor = anyError
    ? "var(--danger)"
    : anyWarn
      ? "var(--warn)"
      : "var(--border-strong)";

  const hasSiblings = ordered.length > 0;
  const errorCount = errored.length;
  const focusedTone = STATUS[focused.status] ?? STATUS.eval;

  return (
    <CapsuleShell
      testId={testId}
      tone={focused.status}
      borderColor={borderColor}
      expanded={expanded}
    >
      {/* Focused-eval row (always rendered). */}
      <div
        className="flex items-stretch"
        style={{ borderBottom: expanded ? "1px solid var(--border)" : "none" }}
      >
        <CapsuleRow
          run={focused}
          focused={true}
          currentSpan={focused.currentSpan ?? null}
          retentionMode={retentionMode}
        />

        {/* Trailing controls */}
        <div className="flex items-center gap-0.5 pr-1 shrink-0">
          {hasSiblings && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setExpanded((v) => !v);
              }}
              title={
                expanded
                  ? "Collapse"
                  : `Show ${ordered.length} other eval${ordered.length === 1 ? "" : "s"}`
              }
              aria-label={
                expanded
                  ? "Collapse other evals"
                  : `Show ${ordered.length} other eval${ordered.length === 1 ? "" : "s"}`
              }
              aria-expanded={expanded}
              className="h-7 px-2 mx-0.5 flex items-center gap-1.5 rounded-full text-[10px] font-mono tracking-[0.16em]"
              style={{
                background: anyError
                  ? "rgba(255,77,77,0.10)"
                  : expanded
                    ? "var(--gold-bg)"
                    : "var(--surface-card)",
                border: `1px solid ${
                  anyError
                    ? "var(--danger)"
                    : expanded
                      ? "var(--gold-soft)"
                      : "var(--border-strong)"
                }`,
                color: anyError
                  ? "var(--danger)"
                  : expanded
                    ? "var(--gold)"
                    : "var(--text-2)",
              }}
            >
              <span>+{ordered.length}</span>
              <span className="text-text-4">·</span>
              <span>{ordered.length === 1 ? "OTHER" : "OTHERS"}</span>
              {errorCount > 0 && !expanded && (
                <>
                  <span className="text-text-4">·</span>
                  <span style={{ color: "var(--danger)" }}>{errorCount} ERR</span>
                </>
              )}
            </button>
          )}

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

          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onPopOut && onPopOut();
            }}
            aria-label="open dedicated trace view"
            title="Open in dedicated route"
            className="h-7 w-7 flex items-center justify-center text-text-3 hover:text-text rounded-full"
            style={{ background: "transparent", border: "none", cursor: "pointer" }}
          >
            <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
              <path
                d="M6 3h7v7M13 3l-7 7M3 8v5h5"
                stroke="currentColor"
                strokeWidth="1.4"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          </button>
        </div>
      </div>

      {/* Expanded body: one row per sibling. */}
      {expanded && hasSiblings && (
        <div className="flex flex-col">
          {ordered.map((r, i) => (
            <div
              key={r.id}
              style={{ borderTop: i === 0 ? "none" : "1px solid var(--border-soft)" }}
            >
              <CapsuleRow
                run={r}
                focused={false}
                onClick={() => onSwitchFocus && onSwitchFocus(r)}
              />
            </div>
          ))}
          <div
            className="px-3 py-1.5 text-[9px] font-mono tracking-[0.18em] text-text-4 flex items-center justify-between"
            style={{
              borderTop: "1px solid var(--border-soft)",
              background: "var(--surface-card)",
            }}
          >
            <span>
              {ordered.length + 1} EVAL{ordered.length === 0 ? "" : "S"} RUNNING ON CLUSTER
            </span>
            <span>CLICK ROW → SWITCH FOCUS</span>
          </div>
        </div>
      )}

      {/* Sliver of focused tone for keyboard / a11y inspectors. */}
      <span className="sr-only">{focusedTone.label}</span>
    </CapsuleShell>
  );
}
