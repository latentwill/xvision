// frontend/web/src/features/agent-runs/span-colors.ts
//
// 5-kind palette matching the design prototype. Hex values from
// docs/superpowers/designs/2026-05-17-agent-run-observability/dock.jsx
// (KIND_DEF) and flame.jsx (KIND_COLORS).

import type { SpanKind } from "@/api/types-agent-runs";

export type SpanCategory =
  | "agent"
  | "model"
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
  model:      { hex: "#7dd3fc", label: "MODEL" },
  tool:       { hex: "#6ee7b7", label: "TOOL"  },
  // qa-trace-broker-spans: distinct rose tint for broker.call rows so
  // Buy / Sell / Close / Short submissions read as a separate column
  // on the flame graph alongside model.call.
  broker:     { hex: "#f472b6", label: "BROKR" },
  supervisor: { hex: "#d4a547", label: "SUPER" },
  artifact:   { hex: "#a78bfa", label: "ARTIF" },
};

export function categoryOf(kind: SpanKind): SpanCategory {
  if (kind === "agent.run" || kind === "agent.plan") return "agent";
  if (kind === "model.call") return "model";
  if (kind === "tool.call") return "tool";
  if (kind === "broker.call") return "broker";
  if (kind === "artifact.write") return "artifact";
  // approval.*, sandbox.exec, supervisor.review, financial.eval
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
