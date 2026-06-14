// frontend/web/src/features/agent-runs/span-colors.ts
//
// 5-kind palette matching the design prototype. Hex values from
// docs/superpowers/designs/2026-05-17-agent-run-observability/dock.jsx
// (KIND_DEF) and flame.jsx (KIND_COLORS).

import type { RunSpan, SpanKind } from "@/api/types-agent-runs";
import { engineEventStyle } from "./engine-event-kinds";

export type SpanCategory =
  | "agent"
  | "decision"
  | "model"
  | "reasoning"
  | "tool"
  | "broker"
  | "supervisor"
  | "artifact"
  // WS-11a OPTI scope categories (autooptimizer cycle trace). The cycle root +
  // its phase rows (parent/experiment/honesty/flywheel) get cool, neutral
  // tints; the three gate outcomes get the Active/Suspect/Rejected tones from
  // the terminology lock (Active=positive gold, Suspect=warn, Rejected=muted).
  | "opti_cycle"
  | "opti_phase"
  | "opti_kept"
  | "opti_suspect"
  | "opti_rejected"
  // WS-11b: the candidate's nested eval-run drill-link node. A distinct teal
  // so it reads as a "go look at the actual run" affordance under the
  // experiment, separate from the phase rows.
  | "opti_eval_run"
  // WS-8 typed fallback: a span kind we don't recognise lands here instead of
  // being silently bucketed into `supervisor` (which read as a confident
  // "SUPER" badge). An `unknown`-category row still renders — it just shows the
  // raw kind — so a new/unforeseen kind is never dropped.
  | "unknown";

type CategoryStyle = {
  hex: string;     // canonical color used everywhere (bar, dot, badge)
  label: string;   // SHORT uppercase tag shown in inspector + strip (5 chars max)
};

export const CATEGORY_STYLES: Record<SpanCategory, CategoryStyle> = {
  agent:      { hex: "#a39a85", label: "AGENT" },
  // QA30: `agent.decision` spans previously fell into the supervisor
  // catch-all and rendered as "SUPER" green, which confused operators
  // — the user expected the three primary span kinds (decision, tool,
  // model) to be visually first-class. Give decision its own swatch +
  // 5-char DECDE label so the trace dock reads as the producer wired it.
  decision:   { hex: "#fbbf24", label: "DECDE" },
  model:      { hex: "#7dd3fc", label: "MODEL" },
  // WS-17: chain-of-thought (`decision.reasoning`). A softer indigo tint
  // so it reads as a distinct child band under the blue `decision.model`
  // call it nests beneath, without colliding with the MODEL swatch.
  reasoning:  { hex: "#a5b4fc", label: "REASN" },
  tool:       { hex: "#6ee7b7", label: "TOOL"  },
  // qa-trace-broker-spans: distinct rose tint for broker.call rows so
  // Buy / Sell / Close / Short submissions read as a separate column
  // on the flame graph alongside model.call.
  broker:     { hex: "#f472b6", label: "BROKR" },
  supervisor: { hex: "#00E676", label: "SUPER" },
  artifact:   { hex: "#a78bfa", label: "ARTIF" },
  // WS-11a OPTI scope. The cycle root reads as a calm slate so the colourful
  // phase/gate rows nest visibly beneath it. Phase rows (parent / experiment /
  // honesty / flywheel) share one cyan-leaning tint. The three gate outcomes
  // map to the lock's Active/Suspect/Rejected tones.
  opti_cycle:    { hex: "#94a3b8", label: "CYCLE" },
  opti_phase:    { hex: "#67e8f9", label: "PHASE" },
  opti_kept:     { hex: "#fbbf24", label: "ACTIV" }, // Active (kept) — positive
  opti_suspect:  { hex: "#f59e0b", label: "SUSPT" }, // Suspect — warn
  opti_rejected: { hex: "#6b7280", label: "REJCT" }, // Rejected — muted
  // WS-11b: the candidate's eval-run drill-link. Teal, distinct from the
  // cyan phase tint, so the "open the actual run" node stands out.
  opti_eval_run: { hex: "#2dd4bf", label: "EVRUN" },
  // WS-8 typed fallback. Neutral slate, distinct from every confident family
  // swatch — a fallback row reads as "uncategorised", not as a known kind.
  unknown:    { hex: "#64748b", label: "EVENT" },
};

/**
 * Span kinds that fall under the `supervisor` category. Enumerated explicitly
 * (rather than via a catch-all `else`) so a genuinely-unknown kind resolves to
 * the `unknown` typed fallback instead of being mislabeled as "SUPER". WS-8.
 */
const SUPERVISOR_KINDS: ReadonlySet<string> = new Set([
  "approval.request",
  "approval.response",
  "sandbox.exec",
  "supervisor.review",
  "financial.eval",
  "ipc.notification",
  "skill.invoke",
  "recovery.attempt",
  "state.transition",
]);

export function categoryOf(kind: SpanKind): SpanCategory {
  if (kind === "agent.run" || kind === "agent.plan") return "agent";
  // QA30: agent.decision is its own first-class category, not a
  // supervisor sub-kind. Decision spans carry the buy/sell/hold action
  // + positions + price/asset on close and need to read as the
  // producer-stamped action they are.
  if (kind === "agent.decision") return "decision";
  // WS-17 span taxonomy: the decision-producing model call + its
  // chain-of-thought. `model.call` is kept as a legacy alias so older
  // exports still colour correctly.
  if (kind === "decision.model" || kind === "model.call") return "model";
  if (kind === "decision.reasoning" || kind === "model.reasoning") return "reasoning";
  // F-4 validate brackets are tool-adjacent — keep them in the tool
  // column so a flame-graph reader sees one continuous tool band per
  // call rather than three differently-coloured slices.
  if (
    kind === "tool.call" ||
    kind === "tool.validate_input" ||
    kind === "tool.validate_output"
  )
    return "tool";
  if (kind === "broker.call") return "broker";
  if (kind === "artifact.write") return "artifact";
  // WS-11a OPTI scope: the autooptimizer cycle trace rows. The cycle root gets
  // its own slate swatch; every phase row shares the cyan PHASE tint. Gate
  // rows resolve their three-way Active/Suspect/Rejected tone from
  // `attributes.outcome` via `spanColorForSpan` — `categoryOf` (kind-only)
  // can't see the outcome, so a bare opti.gate defaults to the phase tint.
  if (kind === "opti.cycle") return "opti_cycle";
  // WS-11b: the candidate's eval-run drill-link node gets its own teal swatch.
  if (kind === "opti.eval-run") return "opti_eval_run";
  if (
    kind === "opti.parent" ||
    kind === "opti.experiment" ||
    kind === "opti.honesty" ||
    kind === "opti.flywheel" ||
    kind === "opti.gate" ||
    kind === "opti.judge"
  )
    return "opti_phase";
  // F-4 observability infrastructure spans (state.transition,
  // recovery.attempt) plus the pre-existing approval / sandbox /
  // supervisor / financial.eval kinds all fall under supervisor.
  if (SUPERVISOR_KINDS.has(kind)) return "supervisor";
  // WS-8: anything left — including `engine.event` (resolved per-span by
  // `categoryOfSpan`) and any future/unforeseen kind — lands in the typed
  // `unknown` fallback rather than masquerading as "SUPER".
  return "unknown";
}

export function spanColor(kind: SpanKind): CategoryStyle {
  return CATEGORY_STYLES[categoryOf(kind)];
}

/**
 * Swatch for an OPTI gate row, keyed off its three-way outcome (the kind alone
 * is ambiguous — see `categoryOf`). Active (kept) = positive gold, Suspect =
 * warn amber, Rejected = muted grey, per the terminology lock.
 */
export function optiGateColor(
  outcome: "kept" | "suspect" | "rejected",
): CategoryStyle {
  if (outcome === "kept") return CATEGORY_STYLES.opti_kept;
  if (outcome === "suspect") return CATEGORY_STYLES.opti_suspect;
  return CATEGORY_STYLES.opti_rejected;
}

/**
 * Whether `kind` is a recognised SpanKind (vs. a typed fallback). Used by the
 * WS-8 parity test and by renderers that want to flag uncategorised rows.
 * `engine.event` is recognised (it resolves per-span) even though a bare
 * `categoryOf` lookup buckets it as `unknown`.
 */
export function isKnownSpanKind(kind: SpanKind | string): boolean {
  if (kind === "engine.event") return true;
  return categoryOf(kind as SpanKind) !== "unknown";
}

/**
 * Span-aware category. Identical to {@link categoryOf} for ordinary spans, but
 * for `engine.event` rows it derives the visual family from the carried
 * `attributes.engine_event_kind` (risk / order / regime / memory / …) so the
 * engine-event timeline reads as its true family instead of a flat blob.
 *
 * Engine-event families don't map 1:1 onto SpanCategory (e.g. `risk`, `order`,
 * `memory` have no span equivalent), so engine-event color/label come from the
 * engine-event palette via {@link spanColorForSpan}. This function returns a
 * coarse `SpanCategory` only for the filter machinery; `unknown` here just
 * means "not one of the span categories" (the row still renders).
 */
export function categoryOfSpan(span: Pick<RunSpan, "kind" | "attributes">): SpanCategory {
  if (span.kind === "engine.event") {
    // Engine events live in their own family taxonomy; for the SpanCategory
    // filter they all read as `decision`-adjacent lifecycle, but the precise
    // color comes from `spanColorForSpan`. Keep them out of `unknown` so the
    // filter machinery treats a known engine event as categorised.
    const ek = engineEventKindOf(span);
    return ek ? "decision" : "unknown";
  }
  return categoryOf(span.kind);
}

/**
 * Span-aware color + label. For `engine.event` rows the swatch comes from the
 * engine-event family palette (`engine-event-kinds.ts`); for everything else
 * it's the span palette. Unknown kinds (span or engine) render the neutral
 * fallback swatch — never blank.
 */
export function spanColorForSpan(
  span: Pick<RunSpan, "kind" | "attributes">,
): CategoryStyle {
  if (span.kind === "opti.gate") {
    const outcome = (span.attributes as { outcome?: unknown }).outcome;
    if (outcome === "kept" || outcome === "suspect" || outcome === "rejected") {
      return optiGateColor(outcome);
    }
  }
  if (span.kind === "engine.event") {
    const ek = engineEventKindOf(span);
    if (ek) return engineEventStyle(ek);
    return CATEGORY_STYLES.unknown;
  }
  return spanColor(span.kind);
}

/** Read the carried `EngineEvent.kind` off an `engine.event` span. */
export function engineEventKindOf(
  span: Pick<RunSpan, "attributes">,
): string | null {
  const v = (span.attributes ?? {})["engine_event_kind"];
  return typeof v === "string" && v.length > 0 ? v : null;
}

/** rgba helper for opacity-tinted backgrounds — matches the prototype's hexA(). */
export function withAlpha(hex: string, a: number): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${a})`;
}
