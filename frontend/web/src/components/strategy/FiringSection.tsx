// "When does this fire?" sub-section per AgentRef on the strategy
// editor. Phase 3 of `agent-firing-filter`.
//
// Two states:
//   - Default: "Every bar." + [Add filter →] button. Clicking opens
//     the inline composer (in-card accordion expansion — never a popup).
//   - Active: "Fires when <upstream_role>.<field> <op> <value>" with
//     [Edit] / [Remove] affordances.
//
// Hidden for refs whose `activates` is "filter" — the Filter is the
// gate, it doesn't have one. (Pre-Phase-A refs lack `activates`; those
// are treated as Trader-equivalent per the back-compat path, so the
// section renders on them.)

import { useState } from "react";

import type { Agent } from "@/api/agents";
import type { ProviderRow } from "@/api/types.gen/ProviderRow";
import {
  setStrategyPipeline,
  type AgentRef,
  type PipelineDef,
} from "@/api/strategies";

import {
  describePredicate,
  findIncomingFilterEdge,
  withoutEdge,
} from "./firingPredicate";
import { InlineFilterComposer } from "./InlineFilterComposer";

export type FiringSectionProps = {
  strategyId: string;
  /// The AgentRef this section gates. Named `agentRef` rather than
  /// `ref` because React reserves the `ref` prop.
  agentRef: AgentRef;
  refs: AgentRef[];
  pipeline: PipelineDef;
  filterCandidates: Agent[];
  providers: ProviderRow[];
  onMutated: () => void;
};

export function FiringSection({
  strategyId,
  agentRef,
  refs,
  pipeline,
  filterCandidates,
  providers,
  onMutated,
}: FiringSectionProps) {
  const [open, setOpen] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  // Refs whose position explicitly activates the Filter capability
  // are the gates themselves and don't get a "When does this fire?"
  // section.
  if (agentRef.activates === "filter") return null;

  const incoming = findIncomingFilterEdge(agentRef, pipeline, refs);

  async function removeFilter() {
    if (!incoming) return;
    setRemoving(true);
    setErr(null);
    try {
      const next = withoutEdge(
        pipeline,
        incoming.edge.from_role,
        incoming.edge.to_role,
      );
      await setStrategyPipeline(strategyId, {
        kind: next.kind,
        edges: next.edges,
      });
      onMutated();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setRemoving(false);
    }
  }

  return (
    <div
      data-testid={`firing-section-${agentRef.role}`}
      className="rounded border border-border-soft p-2.5"
    >
      <div className="text-[11px] uppercase tracking-wide text-text-3 pb-1">
        When does this fire?
      </div>
      {incoming ? (
        <ActiveFilterSummary
          edge={incoming.edge}
          upstreamRole={incoming.upstream.role}
          onEdit={() => setOpen(true)}
          onRemove={removeFilter}
          removing={removing}
        />
      ) : (
        <div className="flex items-center justify-between gap-3">
          <span className="text-[13px] text-text-2">Every bar.</span>
          <button
            type="button"
            data-testid={`firing-add-filter-${agentRef.role}`}
            className="px-2.5 py-1 rounded border border-border-soft text-[12px] text-text-2 hover:text-text"
            onClick={() => setOpen(true)}
          >
            Add filter →
          </button>
        </div>
      )}
      {err ? <div className="text-[12px] text-danger pt-2">{err}</div> : null}
      {open ? (
        <div className="pt-3">
          <InlineFilterComposer
            strategyId={strategyId}
            target={agentRef}
            pipeline={pipeline}
            filterCandidates={filterCandidates}
            existingFilterRef={incoming?.upstream}
            initialCondition={incoming?.edge.condition}
            providers={providers}
            onClose={() => setOpen(false)}
            onSaved={() => {
              setOpen(false);
              onMutated();
            }}
          />
        </div>
      ) : null}
    </div>
  );
}

function ActiveFilterSummary({
  edge,
  upstreamRole,
  onEdit,
  onRemove,
  removing,
}: {
  edge: NonNullable<ReturnType<typeof findIncomingFilterEdge>>["edge"];
  upstreamRole: string;
  onEdit: () => void;
  onRemove: () => void;
  removing: boolean;
}) {
  const desc = edge.condition ? describePredicate(edge.condition) : null;
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-[13px] text-text-2 font-mono">
        Fires when{" "}
        <span className="text-text">{upstreamRole}</span>
        {desc ? (
          <>
            <span className="text-text-3">.</span>
            <span className="text-text">{desc.signalField}</span>{" "}
            <span className="text-text-3">{opLabel(desc.op)}</span>{" "}
            <span className="text-text">{formatValue(desc.value)}</span>
          </>
        ) : (
          <span className="text-text-3"> · composite predicate (edit to see)</span>
        )}
      </span>
      <div className="flex gap-2">
        <button
          type="button"
          className="px-2.5 py-1 rounded border border-border-soft text-[12px] text-text-2 hover:text-text"
          onClick={onEdit}
        >
          Edit
        </button>
        <button
          type="button"
          className="px-2.5 py-1 rounded border border-border-soft text-[12px] text-danger disabled:opacity-50"
          onClick={onRemove}
          disabled={removing}
        >
          {removing ? "Removing…" : "Remove"}
        </button>
      </div>
    </div>
  );
}

function opLabel(op: "eq" | "neq" | "gte" | "lte"): string {
  switch (op) {
    case "eq":
      return "==";
    case "neq":
      return "!=";
    case "gte":
      return "≥";
    case "lte":
      return "≤";
  }
}

function formatValue(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return JSON.stringify(value);
}
