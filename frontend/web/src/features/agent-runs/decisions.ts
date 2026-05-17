// frontend/web/src/features/agent-runs/decisions.ts
import type { RunSpan } from "@/api/types-agent-runs";

export type DecisionRef = { i: number };

/** Dedup + sort decision_idx values across a span list. */
export function deriveDecisions(spans: RunSpan[]): DecisionRef[] {
  const seen = new Set<number>();
  const out: DecisionRef[] = [];
  for (const s of spans) {
    if (s.decision_idx != null && !seen.has(s.decision_idx)) {
      seen.add(s.decision_idx);
      out.push({ i: s.decision_idx });
    }
  }
  return out.sort((a, b) => a.i - b.i);
}
