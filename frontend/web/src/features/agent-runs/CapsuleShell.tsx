// frontend/web/src/features/agent-runs/CapsuleShell.tsx
//
// Shared chrome for the floating run capsules (eval + live). Two primitives
// are extracted here so the EvalCapsule and the dedicated LiveCapsule share
// the same shell and the same single-row body WITHOUT duplicating the visual
// language:
//
//   * `CapsuleShell` — the floating fixed-position container/pill. Parameterised
//     by border colour + the rounded-vs-boxed `expanded` flag; renders its
//     children verbatim. EvalCapsule composes its focused row + trailing
//     controls + expandable sibling stack inside it; LiveCapsule composes its
//     single focused row + compact orders section inside it.
//
//   * `CapsuleRow` — one run row (status dot, EVAL/LIVE prefix, short-tag
//     `<Link>`, `spans·elapsed·cost·pnl`, current-span chip). This is the
//     former private `EvalLine`, lifted out unchanged so the eval capsule's
//     rendered DOM is byte-for-byte identical (regression-gated by the
//     existing EvalCapsule tests).
//
// CRITICAL: do not change the rendered output of `CapsuleRow` / the shell
// container — the EvalCapsule + StripDockSlot tests are the regression gate.

import type { ReactNode } from "react";
import { Link } from "react-router-dom";

import type { RetentionMode } from "../../api/types-agent-runs";

export type EvalCapsuleStatus = "eval" | "pass" | "warn" | "error" | "queued";

/**
 * Operator-facing fidelity token per retention mode. Tells the operator
 * at a glance whether prompt/response/tool bodies are present on this
 * run (the badge sits beside the capsule rows so it's always visible,
 * not buried in the inspector).
 *
 * Mapping (WS-5 Gap 2):
 *   - `hash_only`  → "hash-only" — only hashes/counts stored, no bodies.
 *   - `redacted`   → "redacted"  — bodies present but secret-scrubbed.
 *   - `full_debug` → "full"      — raw bodies on disk.
 */
const FIDELITY_TOKEN: Record<
  RetentionMode,
  { label: string; tint: string; title: string }
> = {
  hash_only: {
    label: "hash-only",
    tint: "var(--text-3)",
    title: "Hash-only retention — no prompt/response/tool bodies stored on disk",
  },
  redacted: {
    label: "redacted",
    tint: "var(--warn)",
    title: "Redacted retention — secret-scrubbed bodies stored on disk",
  },
  full_debug: {
    label: "full",
    tint: "var(--info)",
    title: "Full retention — raw prompt/response/tool bodies stored on disk",
  },
};

/**
 * Small fidelity chip surfacing the run's retention mode. Matches the
 * capsule's `text-[10px] font-mono tracking` chip language (same family
 * as the EVAL/LIVE prefix + status label) and is keyed by
 * `data-fidelity` for deep-linking / testing.
 */
export function FidelityBadge({
  retentionMode,
}: {
  retentionMode: RetentionMode;
}): ReactNode {
  const tok = FIDELITY_TOKEN[retentionMode] ?? FIDELITY_TOKEN.hash_only;
  return (
    <span
      data-testid="capsule-fidelity-badge"
      data-fidelity={retentionMode}
      title={tok.title}
      className="inline-flex items-center rounded-full px-2 py-0.5 text-[9px] font-mono tracking-[0.18em] uppercase shrink-0"
      style={{
        color: tok.tint,
        background: "color-mix(in srgb, currentColor 12%, transparent)",
        border: "1px solid color-mix(in srgb, currentColor 30%, transparent)",
      }}
    >
      {tok.label}
    </span>
  );
}

export type EvalCapsuleCurrentSpan = {
  color: string;     // hex / css color (typically span_colors)
  label: string;     // category label, e.g. "MODEL"
  name: string;      // span name, e.g. "model.call claude-haiku"
  elapsed?: string;  // pre-formatted elapsed for active span (e.g. "880ms")
};

export type EvalCapsuleRow = {
  /** Stable identifier — used for keying and as the target of onSwitchFocus. */
  id: string;
  /**
   * Capsule prefix label. `"live"` ⇒ the row is a live-money run: prefix
   * reads LIVE in the gold tint and the short tag routes to the live
   * inspector (`/live/runs/:id`). `"opti"` ⇒ an autooptimizer cycle row
   * (WS-11a): prefix reads OPTI and the short tag links to the cycle detail
   * route. Defaults to `"eval"` (prefix EVAL, routes to `/eval-runs/:id`) so
   * existing call sites are unchanged.
   */
  kind?: "eval" | "live" | "opti";
  /** Short `strategy·scenario` tag (e.g. `mr·flash`). Never the hex run-id. */
  short: string;
  status: EvalCapsuleStatus;
  /** Span count, pre-formatted-friendly. Use `"—"` when not yet known. */
  spans: number | string;
  /** Pre-formatted elapsed string. Use `"—"` when not yet known. */
  elapsed: string;
  /** Pre-formatted cost string (e.g. `"$0.18"`). Use `"—"` when unknown. */
  cost: string;
  /**
   * QA30: pre-formatted PnL string (e.g. `"+1.42%"`, `"-0.18%"`, or `"$+12.50"`
   * depending on the producer). Use `"—"` when unknown — common for evals that
   * haven't produced an equity sample yet. Optional so existing call sites
   * that haven't been updated render as before (no PnL slot shown).
   */
  pnl?: string;
};

export type EvalCapsuleFocused = EvalCapsuleRow & {
  currentSpan?: EvalCapsuleCurrentSpan | null;
};

type StatusToken = { tint: string; label: string; pulse: boolean };

export const STATUS: Record<EvalCapsuleStatus, StatusToken> = {
  eval:   { tint: "var(--info)",   label: "RUNNING",   pulse: true  },
  pass:   { tint: "var(--gold)",   label: "COMPLETED", pulse: false },
  warn:   { tint: "var(--warn)",   label: "WARN",      pulse: false },
  error:  { tint: "var(--danger)", label: "ERROR",     pulse: true  },
  queued: { tint: "var(--text-3)", label: "QUEUED",    pulse: false },
};

/**
 * Pick a CSS color for a PnL string. Sign-first lookup so we don't have to
 * parse the producer's formatting (the caller may render as `+1.42%`, `-$12`,
 * `0.00%`, or `—`). Falls back to the neutral text colour when no sign is
 * present.
 */
export function pnlTone(pnl: string): string {
  const t = pnl.trim();
  if (t.startsWith("+")) return "var(--gold)";
  if (t.startsWith("-")) return "var(--danger)";
  return "var(--text)";
}

/**
 * One capsule run row. Lifted verbatim from EvalCapsule's former private
 * `EvalLine` so the eval capsule's DOM is unchanged. Live-money rows get a
 * LIVE prefix in the gold tint and route to the live inspector; everything
 * else keeps the EVAL prefix + eval inspector.
 */
export function CapsuleRow({
  run,
  focused,
  currentSpan,
  onClick,
  retentionMode,
}: {
  run: EvalCapsuleRow;
  focused: boolean;
  currentSpan?: EvalCapsuleCurrentSpan | null;
  onClick?: () => void;
  /**
   * Retention/fidelity of the focused run (`AgentRunSummary.retention_mode`).
   * When provided AND this is the focused row, the `FidelityBadge` renders
   * INLINE in the row's chip cluster (trailing the spans·cost·pnl text) so the
   * operator always sees whether bodies are present — without the badge ever
   * stacking as a separate row that adds vertical height (the collapsed-pill
   * layout bug it used to cause). Sibling rows never render their own badge.
   */
  retentionMode?: RetentionMode;
}): ReactNode {
  const tok = STATUS[run.status] ?? STATUS.eval;
  // Live-money rows get a LIVE prefix in the gold tint and route to the
  // live inspector; everything else keeps the EVAL prefix + eval inspector.
  const isLiveMoney = run.kind === "live";
  // WS-11a: OPTI cycle rows get an OPTI prefix and link to the cycle detail
  // route. The optimizer cycle is not an agent-run, so it never uses the
  // eval / live inspector links.
  const isOpti = run.kind === "opti";
  const prefixLabel = isOpti ? "OPTI" : isLiveMoney ? "LIVE" : "EVAL";
  const linkTo = isOpti
    ? `/optimizer/cycle/${encodeURIComponent(run.id)}`
    : isLiveMoney
      ? `/live/runs/${encodeURIComponent(run.id)}`
      : `/eval-runs/${encodeURIComponent(run.id)}`;
  const prefixEmphasised = isLiveMoney || isOpti;
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
        <span
          className={`text-[10px] font-mono tracking-[0.18em] ${prefixEmphasised ? "font-semibold" : "text-text-3"}`}
          style={
            isLiveMoney
              ? { color: "var(--gold)" }
              : isOpti
                ? { color: "var(--info)" }
                : undefined
          }
          data-testid="capsule-kind-label"
        >
          {prefixLabel}
        </span>
        {/*
          F-6 (qa-round-7): the short `strategy·scenario` tag routes to the
          dedicated inspector for this run — live inspector for live-money
          rows, eval inspector otherwise. Keep it as a sibling of the
          switch-focus button overlay so the focused row remains navigable and
          middle-click / cmd-click keep native link behavior.
        */}
        <Link
          to={linkTo}
          onClick={(e) => e.stopPropagation()}
          className="relative z-20 pointer-events-auto text-[11px] font-mono hover:underline"
          style={{ color: tok.tint }}
          aria-label={isOpti ? `Open optimizer cycle ${run.short}` : `Open eval run ${run.short}`}
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
        {run.pnl ? (
          <>
            <span className="text-text-4 mx-2">·</span>
            <span className="text-text-3">pnl </span>
            <span
              className="tabular-nums"
              style={{ color: pnlTone(run.pnl) }}
            >
              {run.pnl}
            </span>
          </>
        ) : null}
      </span>

      {/*
        WS-5 Gap 2 (inline placement): the focused run's fidelity badge sits
        INLINE here in the chip cluster — `shrink-0`, on the single horizontal
        line — instead of being injected by CapsuleShell as a separate stacked
        row above the body (which added a near-empty second line and broke the
        collapsed pill). Only the focused row carries it.
      */}
      {focused && retentionMode ? (
        <span className="relative z-10 pointer-events-none shrink-0">
          <FidelityBadge retentionMode={retentionMode} />
        </span>
      ) : null}

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

/**
 * The floating, fixed-position capsule container. Both capsules render their
 * body inside it. Parameterised only by the bits that vary between eval and
 * live: the test hook id, the `data-tone` attribute, the resolved border
 * colour, and whether the pill is rounded (collapsed) or boxed (`expanded`).
 *
 * The container markup matches the eval capsule's former inline `<div>` so the
 * eval DOM is unchanged.
 */
export function CapsuleShell({
  testId,
  tone,
  borderColor,
  expanded = false,
  children,
}: {
  testId: string;
  tone: string;
  borderColor: string;
  expanded?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      data-testid={testId}
      data-tone={tone}
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
      {children}
    </div>
  );
}
