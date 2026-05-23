// Helpers + types for the strategy editor's "When does this fire?"
// section and the inline Filter composer. Phase 3 of
// `agent-firing-filter`. Tiny, focused module — kept separate from the
// React components so it's straightforward to unit-test.

import type {
  AgentRef,
  EdgePredicate,
  PipelineDef,
  PipelineEdge,
} from "@/api/strategies";

export type ScalarOp = "eq" | "neq" | "gte" | "lte";

export const SCALAR_OPS: ReadonlyArray<{ op: ScalarOp; label: string }> = [
  { op: "eq", label: "equals" },
  { op: "neq", label: "is not" },
  { op: "gte", label: "≥" },
  { op: "lte", label: "≤" },
];

/// Build an EdgePredicate from the composer's flat form fields.
/// `value` is parsed: numeric strings become numbers, true/false become
/// booleans, otherwise stays a string.
export function buildPredicate(
  op: ScalarOp,
  signalField: string,
  rawValue: string,
): EdgePredicate {
  const value = parseValue(rawValue);
  switch (op) {
    case "eq":
      return { eq: { signal_field: signalField, value } };
    case "neq":
      return { neq: { signal_field: signalField, value } };
    case "gte":
      return { gte: { signal_field: signalField, value } };
    case "lte":
      return { lte: { signal_field: signalField, value } };
  }
}

function parseValue(raw: string): unknown {
  const trimmed = raw.trim();
  if (trimmed === "true") return true;
  if (trimmed === "false") return false;
  if (trimmed === "") return "";
  const n = Number(trimmed);
  if (!Number.isNaN(n) && Number.isFinite(n) && /^-?\d+(\.\d+)?$/.test(trimmed)) {
    return n;
  }
  return trimmed;
}

/// Inverse of `buildPredicate` for the read-only summary view —
/// returns `null` if the predicate is something the composer can't
/// describe in flat (op, field, value) form (an `all`/`any`/`not`/`in`
/// composite was authored elsewhere).
export function describePredicate(
  pred: EdgePredicate,
): { op: ScalarOp; signalField: string; value: unknown } | null {
  if ("eq" in pred) return { op: "eq", signalField: pred.eq.signal_field, value: pred.eq.value };
  if ("neq" in pred)
    return { op: "neq", signalField: pred.neq.signal_field, value: pred.neq.value };
  if ("gte" in pred)
    return { op: "gte", signalField: pred.gte.signal_field, value: pred.gte.value };
  if ("lte" in pred)
    return { op: "lte", signalField: pred.lte.signal_field, value: pred.lte.value };
  return null;
}

/// Return the incoming PipelineEdge that gates `ref` on a Filter
/// agent, or `null` if `ref` fires every bar. Considers only edges
/// where `condition` is non-null — unconditional edges (today's
/// sequential default) do not appear here.
export function findIncomingFilterEdge(
  ref: AgentRef,
  pipeline: PipelineDef,
  refs: AgentRef[],
): { edge: PipelineEdge; upstream: AgentRef } | null {
  for (const edge of pipeline.edges ?? []) {
    if (edge.to_role !== ref.role || !edge.condition) continue;
    const upstream = refs.find((r) => r.role === edge.from_role);
    if (!upstream) continue;
    return { edge, upstream };
  }
  return null;
}

/// Compose a new pipeline with `edge` added (replacing any existing
/// edge with the same `(from_role, to_role)` pair). Promotes the
/// pipeline kind from "single"/"sequential" to "graph" when needed —
/// the strategy editor uses sequential-by-position today, but
/// conditional edges require an explicit graph.
export function withAddedEdge(
  pipeline: PipelineDef,
  edge: PipelineEdge,
): PipelineDef {
  const existing = (pipeline.edges ?? []).filter(
    (e) => !(e.from_role === edge.from_role && e.to_role === edge.to_role),
  );
  return { kind: "graph", edges: [...existing, edge] };
}

/// Drop the (from_role → to_role) edge from `pipeline`. If the result
/// has no remaining edges and the prior kind was "graph", caller may
/// want to demote to "sequential"; we leave that to the caller since
/// the demotion choice depends on the post-removal agent count.
export function withoutEdge(
  pipeline: PipelineDef,
  fromRole: string,
  toRole: string,
): PipelineDef {
  return {
    kind: pipeline.kind,
    edges: (pipeline.edges ?? []).filter(
      (e) => !(e.from_role === fromRole && e.to_role === toRole),
    ),
  };
}
