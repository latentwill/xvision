// MemoryTab — per-agent Memory surface for V2D Observations + Patterns.
//
// Mounted on `/agents/<id>` as a sibling to the AgentForm. Two
// sub-tabs:
//
//   - Patterns      — agent-scoped + global Patterns, with an
//                     "+ Add Pattern" modal. Operator-editable per
//                     V2D intake Decision 6 (add + delete only).
//   - Observations  — agent-scoped Observations, read-only with
//                     scenario_id / run_id filters. Per V2D intake
//                     Decision 5, individual Observations are not
//                     deletable from the UI — bulk-only via "Forget
//                     all memory for this agent".
//
// The "Forget all memory" button at the bottom of the tab triggers a
// hand-rolled AlertDialog (no Radix dependency in this repo); on
// confirm it calls DELETE /api/memory?agent=<id> and invalidates the
// memory list keys so the tab re-renders empty.
//
// Phase 4 (V2D follow-up v1.1) lifted the actual list / modal /
// forget-dialog logic into `@/features/memory/MemorySurface` so the
// workspace-level `/memory` page can share it. This file is now a
// thin wrapper that pins the surface to `mode="agent"` and threads
// the optional `?pattern=<id>` deep-link.

import { MemorySurface } from "@/features/memory/MemorySurface";

export {
  // Re-export the helper so external callers (Phase 3 imported it from
  // here) don't need to chase the relocation.
  useMemoryItemCount,
} from "@/features/memory/MemorySurface";

export function MemoryTab({
  agentId,
  highlightPatternId,
}: {
  agentId: string;
  highlightPatternId?: string | null;
}) {
  return (
    <MemorySurface
      mode="agent"
      agentId={agentId}
      highlightPatternId={highlightPatternId ?? null}
    />
  );
}
