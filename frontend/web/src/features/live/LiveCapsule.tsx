// frontend/web/src/features/live/LiveCapsule.tsx
//
// Dedicated floating capsule for a LIVE run. Unlike the eval capsule (which can
// grow into a sibling stack of concurrent cluster evals), a live run is a
// single run — so the live capsule renders exactly ONE focused row plus a
// compact orders section listing the run's `broker.call` spans.
//
// It shares the floating shell + the single-row body primitive with the eval
// capsule via `CapsuleShell` / `CapsuleRow`, so both capsules speak the same
// visual language. The orders section is the live-specific addition: one
// compact line per broker submit (`SIDE SYMBOL qty @ price → outcome`),
// rendered off the `broker_call` payload already on the trace spans (the same
// payload the SpanInspector's `BrokerCallDetailRows` expands in full).
//
// The orders section is also the natural seam for WS-4 (real venue labels,
// order-state / partial-fill chips, signing rows): today it renders only what
// a `broker.call` span carries — including `broker_call.venue` as-is, which is
// `"live"` until WS-4 differentiates. Do NOT invent venues or order states here.

import {
  CapsuleRow,
  CapsuleShell,
  STATUS,
  type EvalCapsuleFocused,
} from "../agent-runs/CapsuleShell";
import type { BrokerCallDetail, RetentionMode, RunSpan } from "@/api/types-agent-runs";

export type LiveCapsuleProps = {
  /**
   * The focused live run. Same shape the eval capsule's focused row uses;
   * callers should pass `kind: "live"` so the row reads LIVE and the short
   * tag routes to the live inspector (`/live/runs/:id`).
   */
  run: EvalCapsuleFocused;
  /**
   * The run's `broker.call` spans. The slot passes
   * `q.data.spans.filter((s) => s.kind === "broker.call")`. Each renders as
   * one compact order line off its `broker_call` payload.
   */
  brokerSpans: RunSpan[];
  /** Invoked when the user opens the trace dock (up-chevron). */
  onExpandDock?: () => void;
  /** Invoked when the user opens the focused run's dedicated route. */
  onPopOut?: () => void;
  /** Optional test hook id. Default `"live-capsule"`. */
  testId?: string;
  /**
   * The run's retention/fidelity (`AgentRunSummary.retention_mode`).
   * Forwarded to `CapsuleShell` so the operator sees whether bodies are
   * present. Optional — omitted on legacy call sites.
   */
  retentionMode?: RetentionMode;
};

/** Compact numeric formatter for the order line — trims trailing zeros. */
function fmtNum(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "—";
  // Up to 6 sig digits after the point, trailing zeros stripped, so
  // `1.5` shows as `1.5` and `142.6` as `142.6` (not `1.500000`).
  return String(Number(n.toFixed(6)));
}

/** Theme-token tone for a broker outcome, mirroring the inspector's mapping. */
function outcomeTone(outcome: BrokerCallDetail["outcome"]): string {
  if (outcome === "filled") return "var(--gold)";
  if (outcome === "cancelled") return "var(--text-3)";
  if (outcome != null) return "var(--danger)"; // rejected / failed
  return "var(--info)"; // in-progress
}

/**
 * One compact order line: `SIDE SYMBOL qty @ price → outcome (venue)`.
 *
 * `price` shows the fill price for filled orders (what actually executed),
 * falling back to the intended price otherwise — so an in-flight / rejected
 * order still shows the price it was submitted at. Venue is rendered as-is
 * from the span (`"live"` today; WS-4 differentiates).
 */
function OrderLine({ detail }: { detail: BrokerCallDetail }) {
  const price =
    detail.outcome === "filled" && detail.fill_price != null
      ? detail.fill_price
      : detail.intended_price;
  const outcome = detail.outcome ?? "in_progress";
  return (
    <div
      data-testid="live-capsule-order"
      className="h-7 px-3 flex items-center gap-2 font-mono text-[11px] whitespace-nowrap"
    >
      <span
        className="uppercase tracking-[0.12em] shrink-0"
        style={{
          color: detail.side === "sell" || detail.side === "short"
            ? "var(--danger)"
            : "var(--gold)",
        }}
      >
        {detail.side}
      </span>
      <span className="text-text shrink-0">{detail.symbol}</span>
      <span className="text-text-2 tabular-nums shrink-0">{fmtNum(detail.qty)}</span>
      <span className="text-text-4 shrink-0">@</span>
      <span className="text-text-2 tabular-nums shrink-0">{fmtNum(price)}</span>
      <span className="text-text-4 shrink-0">→</span>
      <span
        className="tracking-[0.04em] shrink-0"
        style={{ color: outcomeTone(detail.outcome) }}
      >
        {outcome}
      </span>
      <span className="text-text-4 ml-auto shrink-0 tracking-[0.16em] text-[10px]">
        {detail.venue}
      </span>
    </div>
  );
}

export function LiveCapsule({
  run,
  brokerSpans,
  onExpandDock,
  onPopOut,
  testId = "live-capsule",
  retentionMode,
}: LiveCapsuleProps) {
  // Status drives the border + the row pill. Running (eval tone) pulses;
  // terminal tones (pass/warn/error) are frozen. Error/warn tint the border
  // so a failed live run is unmissable, matching the eval capsule's logic.
  const borderColor =
    run.status === "error"
      ? "var(--danger)"
      : run.status === "warn"
        ? "var(--warn)"
        : "var(--border-strong)";
  const focusedTone = STATUS[run.status] ?? STATUS.eval;

  // Only the broker.call spans that actually carry a payload can render an
  // order line; defend against a partially-hydrated span without `broker_call`.
  const orders = brokerSpans.filter(
    (s): s is RunSpan & { broker_call: BrokerCallDetail } =>
      s.broker_call != null,
  );

  return (
    <CapsuleShell
      testId={testId}
      tone={run.status}
      borderColor={borderColor}
      // Live capsule is always boxed (it has a body), never a bare pill.
      expanded={true}
      retentionMode={retentionMode}
    >
      {/* Focused live run row (single run — no sibling stack). */}
      <div
        className="flex items-stretch"
        style={{ borderBottom: "1px solid var(--border)" }}
      >
        <CapsuleRow run={run} focused={true} currentSpan={run.currentSpan ?? null} />

        {/* Trailing controls — expand-dock + pop-out (no sibling toggle). */}
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

      {/* Orders: one compact line per broker.call span. */}
      <div className="flex flex-col" data-testid="live-capsule-orders">
        <div
          className="px-3 py-1 text-[9px] font-mono tracking-[0.18em] text-text-4 flex items-center justify-between"
          style={{ background: "var(--surface-card)" }}
        >
          <span>ORDERS</span>
          <span className="tabular-nums">{orders.length}</span>
        </div>
        {orders.length === 0 ? (
          <div className="px-3 py-1.5 text-[10px] font-mono tracking-[0.12em] text-text-4">
            no orders yet
          </div>
        ) : (
          orders.map((s, i) => (
            <div
              key={s.span_id}
              style={{ borderTop: i === 0 ? "none" : "1px solid var(--border-soft)" }}
            >
              <OrderLine detail={s.broker_call} />
            </div>
          ))
        )}
      </div>

      {/* Sliver of focused tone for keyboard / a11y inspectors. */}
      <span className="sr-only">{focusedTone.label}</span>
    </CapsuleShell>
  );
}
