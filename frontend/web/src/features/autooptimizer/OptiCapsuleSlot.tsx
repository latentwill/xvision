// frontend/web/src/features/autooptimizer/OptiCapsuleSlot.tsx
//
// WS-11a — the /optimizer route's OPTI trace surface. This wires the EXISTING
// cycle SSE subscription through the OPTI reducer (`projectOptiRows`) into the
// floating `OptiCapsule`.
//
// CRITICAL — single subscription: the slot does NOT open its own EventSource.
// `OptimizerHome` already holds the one `useCycleEventStream()` subscription
// (the single `/api/autooptimizer/events` connection) and passes its result in
// as props. This keeps exactly one EventSource on the optimizer surface, per
// the WS-11a contract.
//
// Scope ownership: this slot writes ONLY `byScope.opti` (the active cycle id),
// never the eval/live slices. The shared agent-run capsule (`StripDockSlot`)
// already bails out on the `opti` scope, so the two capsules never collide.

import { useEffect, useMemo } from "react";
import type { EventRow } from "./hooks/useCycleEventStream";
import { projectOptiRows } from "./opti-trace-reducer";
import { OptiCapsule } from "./OptiCapsule";
import { useTraceDock } from "@/stores/trace-dock";

export type OptiCapsuleSlotProps = {
  /** The buffered cycle events from `OptimizerHome`'s single SSE subscription. */
  events: EventRow[];
  /** The active cycle id from the same subscription (null when idle). */
  activeCycleId: string | null;
  /** Whether the cycle is in-flight (drives the pulsing running tone). */
  isRunning: boolean;
};

export function OptiCapsuleSlot({
  events,
  activeCycleId,
  isRunning,
}: OptiCapsuleSlotProps) {
  const setActiveRun = useTraceDock((s) => s.setActiveRun);
  const setHeight = useTraceDock((s) => s.setHeight);

  // The cycle id to scope rows to: the live stream's active cycle while
  // running, else the most recent cycle id seen in the buffer (so a just-
  // finished cycle still shows its trace until the operator navigates away).
  const cycleId = useMemo(() => {
    if (activeCycleId) return activeCycleId;
    for (let i = events.length - 1; i >= 0; i--) {
      const cid = events[i].cycle_id;
      if (typeof cid === "string" && cid) return cid;
    }
    return null;
  }, [activeCycleId, events]);

  // Project the buffered events for this cycle into trace rows. Filtering to
  // the focused cycle keeps a multi-cycle session buffer from cross-nesting
  // rows under the wrong root.
  const rows = useMemo(() => {
    if (!cycleId) return [];
    const scoped = events.filter((e) => !e.cycle_id || e.cycle_id === cycleId);
    return projectOptiRows(scoped);
  }, [events, cycleId]);

  // Mirror the focused cycle id into the opti scope slice so the rest of the
  // dock machinery (selection, the expanded TraceDock) can read it. Writes
  // ONLY byScope.opti.
  useEffect(() => {
    setActiveRun("opti", cycleId, isRunning ? "live" : "post-hoc");
  }, [cycleId, isRunning, setActiveRun]);

  if (!cycleId || rows.length === 0) return null;

  return (
    <OptiCapsule
      rows={rows}
      cycleId={cycleId}
      running={isRunning}
      onExpandDock={() => setHeight("working")}
    />
  );
}
