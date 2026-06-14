// frontend/web/src/features/agent-runs/span-colors.ts
//
// 5-kind palette matching the design prototype. Hex values from
// docs/superpowers/designs/2026-05-17-agent-run-observability/dock.jsx
// (KIND_DEF) and flame.jsx (KIND_COLORS).

import type { RunSpan, SpanKind } from "@/api/types-agent-runs";

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
  | "opti_rejected";

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
};

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
  // approval.*, sandbox.exec, supervisor.review, financial.eval,
  // state.transition, recovery.attempt, skill.invoke, ipc.notification
  return "supervisor";
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
 * Resolve a span's swatch, preferring outcome-aware tints where the kind alone
 * is ambiguous. An `opti.gate` row tints by `attributes.outcome` (kept /
 * suspect / rejected); every other span falls back to the kind-based color.
 *
 * SpanTree uses this so the OPTI gate rows render in their Active/Suspect/
 * Rejected tone rather than a single flat gate colour.
 */
export function spanColorForSpan(span: RunSpan): CategoryStyle {
  if (span.kind === "opti.gate") {
    const outcome = (span.attributes as { outcome?: unknown }).outcome;
    if (outcome === "kept" || outcome === "suspect" || outcome === "rejected") {
      return optiGateColor(outcome);
    }
  }
  return spanColor(span.kind);
}

/** rgba helper for opacity-tinted backgrounds — matches the prototype's hexA(). */
export function withAlpha(hex: string, a: number): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${a})`;
}
