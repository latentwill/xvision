# Intake — 2026-05-21 — memory safety + observability (post-V2D follow-ups)

This intake is the *complement* to
`team/intake/2026-05-21-v2d-agent-memory.md`. V2D ships the in-process
`xvision-memory` crate, the per-slot `MemoryMode` toggle, the
dispatcher wiring, and the eval-review surface. This intake covers the
three follow-up tracks that V2D explicitly deferred but the operator
triage on 2026-05-21 promoted from "follow-up" to named work.

Reading order:

- V2D intake (foundation memory work) lands first.
- These three tracks layer on top once `v2d-eval-review-memory-surface`
  and `v2d-dispatcher-wiring` are in.

The triage that produced this intake also locked the *kills* (items
the platform will not build) in `team/decisions.md` D5. Anyone
reaching for "but what about cross-namespace blending / mem0 adapters /
embedder swap CLI?" should start there.

## Source

- `team/intake/2026-05-21-v2d-agent-memory.md` "Out of this intake"
  section — three items below were filed there as deferred. Promoted
  to tracks here.
- `team/decisions.md` D5 — the kill bucket. These tracks survive the
  triage; the killed items don't reappear.
- Operator pass 2026-05-21 — safety concerns about destructive `xvn
  memory forget` flagged as the highest-value follow-up.

## Raw items → tracks

| Raw item | Track | Lane | Notes |
|---|---|---|---|
| Soft-delete + grace-period model for `xvn memory forget`: `forget` marks rows with `forgotten_at` rather than DELETE; a janitor hard-deletes rows whose `forgotten_at` is older than `XVN_MEMORY_FORGET_GRACE_DAYS` (default 14); `xvn memory undo-forget --namespace <ns> --since <when>` restores any row whose `forgotten_at` falls inside the grace window | `memory-forget-undo-snapshot` | leaf | Depends on V2D `xvision-memory` crate landing. Single-crate change. Default grace window picked to give an operator a working week + a weekend to notice an accidental forget. No new SQLite migration: the crate owns its schema and can add the column on next open. |
| Bind `memory_recall` events to the specific `decision_id` they fed into, not just the run id; store the `(decision_id, memory_item_ids[])` mapping so the trace UI can answer "which memories influenced this decision" | `memory-provenance-in-decisions-trace` | foundation | Depends on V2D `v2d-dispatcher-wiring` (which emits `memory_recall` events) and `v2d-eval-review-memory-surface` (which renders them). Today V2D's events are run-level. This track threads `decision_id` through the recall path so the per-decision panel in the eval review surface can show provenance, and so future "memory-aware findings" can correlate. |
| Memory-aware findings in eval review: when a decision's outcome is judged (good / bad / inconclusive), emit a `Finding` that names the memory items most likely to have driven the decision based on the provenance mapping. Severity follows the outcome — bad outcomes driven by stale memory get `warning`; good outcomes get `info` | `memory-aware-eval-findings` | leaf | Depends on `memory-provenance-in-decisions-trace`. Pairs naturally with the eval-honesty wave (#448–#452) — same shape of finding, same review-time emission seam. Pure observability: no policy decisions about memory recall behavior, only surfacing of what already happened. |

## Dependency graph

```
[V2D wave]                          ← foundation; lands first
    │
    ├─→ memory-forget-undo-snapshot         (leaf, parallel-safe with the two below)
    │
    └─→ memory-provenance-in-decisions-trace (foundation for the next)
            │
            └─→ memory-aware-eval-findings   (leaf)
```

Sequencing: `memory-forget-undo-snapshot` is independent and can ship
anytime after V2D. The other two stack — provenance is the data
prerequisite for findings.

## Out of this intake

These items are out **specifically because they live in `decisions.md`
D5 as "will not build"** — do not regenerate them as tracks:

- Cross-namespace recall blending.
- Embedder configuration UI.
- Memory diff CLI.
- mem0 / Honcho / mempalace adapters.
- `cortex-http` sidecar.
- Cross-host memory sharing.
- Embedding model swap migration CLI.

These items are V3 candidates and remain out:

- Tool-driven memory (`memory_recall` / `memory_write` as agent tools).
- TTL / time decay / LRU eviction.

## Verification (per track)

- **`memory-forget-undo-snapshot`:** unit tests at
  `crates/xvision-memory/tests/` covering (a) `forget` sets
  `forgotten_at`, doesn't DELETE; (b) `query` skips rows with non-null
  `forgotten_at`; (c) the janitor hard-deletes rows older than the
  grace window; (d) `undo-forget` restores rows inside the grace
  window and is a no-op outside it; (e) `XVN_MEMORY_FORGET_GRACE_DAYS=0`
  collapses to the prior behavior (immediate hard-delete) for users who
  opt out. `cargo test -p xvision-memory` is the gate.
- **`memory-provenance-in-decisions-trace`:** integration test at
  `crates/xvision-engine/tests/agent_memory_dispatch.rs` extending the
  V2D test — assert the emitted `memory_recall` event carries
  `decision_id` and that the item ids match the recall set. Then a
  query-side test that the eval-review run-detail loader can join
  decisions to their recall events.
- **`memory-aware-eval-findings`:** integration test that a run with a
  bad-outcome decision driven by a known stale memory item emits one
  `warning` finding with the expected kind / body. Audit acceptance:
  cite the V2D landing run id once V2D's first non-toy backtest runs.

## Why these three and not others

The V2D intake's "Out of this" list is long. Operator triage on
2026-05-21 promoted *only* these three because:

1. **Forget undo** is a safety net for a destructive verb V2D ships. It
   should land alongside or shortly after V2D, not be deferred. Cost is
   one column + a janitor pass; benefit is operators can experiment
   with `forget` without fear.
2. **Per-decision provenance** is the durable substrate every future
   memory-observability story depends on. Without it, you can see "this
   run recalled 12 items" but not "decision M3 was driven by item
   X." Cheap to add now; expensive to retrofit later.
3. **Memory-aware findings** is the natural product of (2) and the
   eval-honesty wave that just shipped (#448–#452). Same finding
   shape, same emission seam, immediate operator value.

Everything else from V2D's deferred list is either killed (D5) or
genuinely better in V3. This intake is the short list of things that
*aren't*.
