// frontend/web/src/features/agent-runs/span-colors.ts
//
// 5-kind palette matching the design prototype. Hex values from
// docs/superpowers/designs/2026-05-17-agent-run-observability/dock.jsx
// (KIND_DEF) and flame.jsx (KIND_COLORS).

import type { SpanKind } from "@/api/types-agent-runs";

export type SpanCategory =
  | "agent"
  | "decision"
  | "model"
  | "reasoning"
  | "tool"
  | "broker"
  | "supervisor"
  | "artifact";

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

/** rgba helper for opacity-tinted backgrounds — matches the prototype's hexA(). */
export function withAlpha(hex: string, a: number): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${a})`;
}
