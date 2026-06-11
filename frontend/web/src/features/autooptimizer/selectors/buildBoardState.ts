import type { CycleNodeDetail, CycleProgressEvent } from "../api";

// "queued": not yet assigned by this selector — no event currently produces it.
// Consumed downstream by LineageRiver's ghost-fan filter (`state === "queued"`).
export type BoardCardState = "queued" | "evaluating" | "kept" | "rejected" | "suspect";
export type BoardCard = {
  hash: string;
  label: string | null;
  state: BoardCardState;
  delta: number | null;
  writer: string | null;
};
// "keep": not yet assigned by this selector — no event currently produces it.
// Consumed downstream by PhaseRibbon which orders phases including "keep".
export type Phase = "idle" | "propose" | "eval" | "gate" | "keep" | "done";
export type BoardState = { phase: Phase; cards: BoardCard[]; cycleId: string | null };

export function buildBoardState(events: CycleProgressEvent[]): BoardState {
  const cards = new Map<string, BoardCard>();
  let phase: Phase = "idle";
  let cycleId: string | null = null;

  for (const e of events) {
    const kind = e.type ?? e.event_type ?? e.kind ?? "";
    const x = e as Record<string, unknown>; // flattened wire fields (progress.rs)
    const hash = (e.child_hash ?? e.bundle_hash ?? null) as string | null;

    if (kind === "cycle_started") {
      phase = "propose";
      cycleId = e.cycle_id ?? null;
      cards.clear();
    }

    if (kind === "mutation_proposed" && hash) {
      cards.set(hash, {
        hash,
        label: null,
        state: "evaluating",
        delta: null,
        writer: (x.mutator_model as string) || null,
      });
      phase = "eval";
    }

    if (kind === "mutation_gated" && hash) {
      const prev = cards.get(hash) ?? {
        hash,
        label: null,
        state: "evaluating" as const,
        delta: null,
        writer: null,
      };
      const state: BoardCardState =
        x.outcome === "suspect"
          ? "suspect"
          : x.passed === true || x.outcome === "kept"
            ? "kept"
            : "rejected";
      cards.set(hash, {
        ...prev,
        state,
        delta: typeof x.delta_day === "number" ? x.delta_day : null,
      });
      phase = "gate";
    }

    if (kind === "honesty_check_run" && x.passed === false) {
      for (const c of cards.values()) {
        if (c.state === "kept") {
          cards.set(c.hash, { ...c, state: "suspect" });
        }
      }
    }

    if (kind === "cycle_finished") {
      phase = "done";
    }
  }

  return { phase, cards: [...cards.values()], cycleId };
}

/**
 * Node-derived board fallback (Task 10 replay edge case): when a cycle's
 * persisted event log is empty (pruned by `prune_old_events`, run before
 * event persistence shipped, or an older backend lacking the endpoint),
 * derive board cards directly from the cycle's lineage nodes so the board
 * is never blank and `?exp=` deep links still have cards to expand.
 *
 * Status mapping mirrors the operator-surface terminology lock:
 * active → kept, quarantined → suspect, everything else → rejected.
 * Delta is null-safe (nodes may carry `delta_day` from the gate verdict);
 * writer is unknown without the event log.
 */
export function boardFromNodes(nodes: CycleNodeDetail[]): BoardCard[] {
  return nodes.map((n) => {
    const delta = n.delta_day;
    return {
      hash: n.bundle_hash,
      label: null,
      state:
        n.status === "active"
          ? ("kept" as const)
          : n.status === "quarantined"
            ? ("suspect" as const)
            : ("rejected" as const),
      delta: typeof delta === "number" ? delta : null,
      writer: null,
    };
  });
}
