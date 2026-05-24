// frontend/web/src/features/agent-runs/EvalCapsule.tsx
//
// Ported from docs/design/Capsule · Multi-Eval.html.
//
// Single floating capsule for an in-flight eval. When other evals are running
// concurrently on the cluster, the capsule grows downward into a stack — one
// row per eval, formatted identically. Click a non-focused row to switch which
// eval is inspected. Errored siblings auto-promote to the top and force the
// capsule open.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";

export type EvalCapsuleStatus = "eval" | "pass" | "warn" | "error" | "queued";

export type EvalCapsuleCurrentSpan = {
  color: string;     // hex / css color (typically span_colors)
  label: string;     // category label, e.g. "MODEL"
  name: string;      // span name, e.g. "model.call claude-haiku"
  elapsed?: string;  // pre-formatted elapsed for active span (e.g. "880ms")
};

export type EvalCapsuleRow = {
  /** Stable identifier — used for keying and as the target of onSwitchFocus. */
  id: string;
  /** Short `strategy·scenario` tag (e.g. `mr·flash`). Never the hex run-id. */
  short: string;
  status: EvalCapsuleStatus;
  /** Span count, pre-formatted-friendly. Use `"—"` when not yet known. */
  spans: number | string;
  /** Pre-formatted elapsed string. Use `"—"` when not yet known. */
  elapsed: string;
  /** Pre-formatted cost string (e.g. `"$0.18"`). Use `"—"` when unknown. */
  cost: string;
};

export type EvalCapsuleFocused = EvalCapsuleRow & {
  currentSpan?: EvalCapsuleCurrentSpan | null;
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
};

type StatusToken = { tint: string; label: string; pulse: boolean };

const STATUS: Record<EvalCapsuleStatus, StatusToken> = {
  eval:   { tint: "var(--info)",   label: "RUNNING",   pulse: true  },
  pass:   { tint: "var(--gold)",   label: "COMPLETED", pulse: false },
  warn:   { tint: "var(--warn)",   label: "WARN",      pulse: false },
  error:  { tint: "var(--danger)", label: "ERROR",     pulse: true  },
  queued: { tint: "var(--text-3)", label: "QUEUED",    pulse: false },
};

function EvalLine({
  run,
  focused,
  currentSpan,
  onClick,
}: {
  run: EvalCapsuleRow;
  focused: boolean;
  currentSpan?: EvalCapsuleCurrentSpan | null;
  onClick?: () => void;
}): ReactNode {
  const tok = STATUS[run.status] ?? STATUS.eval;
  return (
    <div
      className="relative h-9 w-full flex items-center gap-3 px-3 text-left transition-colors"
      style={{
        background: focused ? "rgba(0,230,118,0.06)" : "transparent",
        borderLeft: `2px solid ${focused ? "var(--gold)" : "transparent"}`,
        cursor: focused ? "default" : "pointer",
        border: "none",
        borderLeftWidth: 2,
        borderLeftStyle: "solid",
        borderLeftColor: focused ? "var(--gold)" : "transparent",
      }}
      onMouseEnter={(e) => {
        if (!focused) e.currentTarget.style.background = "var(--surface-hover)";
      }}
      onMouseLeave={(e) => {
        if (!focused) e.currentTarget.style.background = "transparent";
      }}
    >
      {!focused && onClick && (
        <button
          type="button"
          onClick={onClick}
          aria-label={`Switch focus to eval run ${run.short}`}
          className="absolute inset-0 z-0 cursor-pointer border-0 bg-transparent p-0"
        />
      )}
      <span
        className={`relative z-10 pointer-events-none inline-block w-1.5 h-1.5 rounded-full shrink-0 ${tok.pulse ? "animate-pulse" : ""}`}
        style={{ background: tok.tint, boxShadow: `0 0 0 3px ${tok.tint}22` }}
      />

      <span className="relative z-10 pointer-events-none flex items-center gap-2 shrink-0">
        <span className="text-[10px] font-mono tracking-[0.18em] text-text-3">EVAL</span>
        {/*
          F-6 (qa-round-7): the short `strategy·scenario` tag routes to the
          dedicated eval-inspector for this run. Keep it as a sibling of the
          switch-focus button overlay so the focused row remains navigable and
          middle-click / cmd-click keep native link behavior.
        */}
        <Link
          to={`/eval-runs/${encodeURIComponent(run.id)}`}
          onClick={(e) => e.stopPropagation()}
          className="relative z-20 pointer-events-auto text-[11px] font-mono hover:underline"
          style={{ color: tok.tint }}
          aria-label={`Open eval run ${run.short}`}
        >
          {run.short}
        </Link>
        <span className="text-[10px] font-mono tracking-[0.18em] text-text-3">· {tok.label}</span>
      </span>

      <span className="relative z-10 pointer-events-none w-px h-4 shrink-0" style={{ background: "var(--border)" }} />

      <span className="relative z-10 pointer-events-none text-text font-mono text-[11px] shrink-0">
        <span className="text-text-3">spans </span>
        <span className="tabular-nums">{run.spans}</span>
        <span className="text-text-4 mx-2">·</span>
        <span className="tabular-nums">{run.elapsed}</span>
        <span className="text-text-4 mx-2">·</span>
        <span className="tabular-nums">{run.cost}</span>
      </span>

      {focused && currentSpan && (
        <>
          <span className="relative z-10 pointer-events-none w-px h-4 shrink-0" style={{ background: "var(--border)" }} />
          <span className="relative z-10 pointer-events-none flex items-center gap-1.5 min-w-0 max-w-[260px]">
            <span
              className="w-1.5 h-1.5 rounded-full animate-pulse shrink-0"
              style={{ background: currentSpan.color, boxShadow: `0 0 0 3px ${currentSpan.color}22` }}
            />
            <span
              className="text-[10px] font-mono tracking-[0.16em] shrink-0"
              style={{ color: currentSpan.color }}
            >
              {currentSpan.label}
            </span>
            <span className="text-[11px] font-mono text-text truncate">{currentSpan.name}</span>
            {currentSpan.elapsed != null && (
              <span className="text-[10px] font-mono tabular-nums text-text-3 shrink-0">
                {currentSpan.elapsed}
              </span>
            )}
          </span>
        </>
      )}

      {!focused && (
        <span className="relative z-10 pointer-events-none ml-auto text-[9px] font-mono tracking-[0.18em] text-text-4 shrink-0">
          SWITCH →
        </span>
      )}
    </div>
  );
}

export function EvalCapsule({
  focused,
  siblings = [],
  onSwitchFocus,
  onExpandDock,
  onPopOut,
  testId = "run-status-strip",
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
    <div
      data-testid={testId}
      data-tone={focused.status}
      className="fixed left-1/2 -translate-x-1/2 z-40 select-none whitespace-nowrap flex flex-col overflow-hidden"
      style={{
        bottom: 14,
        background: "var(--surface-elev)",
        border: `1px solid ${borderColor}`,
        borderRadius: expanded ? 12 : 999,
        boxShadow:
          "0 14px 40px rgba(0,0,0,0.55), 0 0 0 1px rgba(0,0,0,0.4)",
        backdropFilter: "blur(8px)",
        maxWidth: "calc(100vw - 32px)",
        minWidth: 520,
        transition: "border-radius 180ms ease",
      }}
    >
      {/* Focused-eval row (always rendered). */}
      <div
        className="flex items-stretch"
        style={{ borderBottom: expanded ? "1px solid var(--border)" : "none" }}
      >
        <EvalLine
          run={focused}
          focused={true}
          currentSpan={focused.currentSpan ?? null}
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
              <EvalLine
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
    </div>
  );
}
