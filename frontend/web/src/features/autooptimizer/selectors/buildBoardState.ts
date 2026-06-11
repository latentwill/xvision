import type { CycleProgressEvent } from "../api";

export type BoardCardState = "queued" | "evaluating" | "kept" | "rejected" | "suspect";
export type BoardCard = {
  hash: string;
  label: string | null;
  state: BoardCardState;
  delta: number | null;
  writer: string | null;
};
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
