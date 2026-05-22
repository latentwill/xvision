---
track: agent-graph-composition
lane: foundation
wave: agent-graph-2026-05-22
worktree: (none — superseded)
branch: (none — superseded)
base: (none)
status: archived
depends_on: []
blocks: []
stacking: none
allowed_paths: []
forbidden_paths: []
interfaces_used: []
parallel_safe: true
parallel_conflicts: []
verification: []
acceptance:
  - This placeholder is superseded by the capability-first spec at `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md` (merged via PR #518 on 2026-05-22).
---

# Scope (superseded)

This contract was a deferred placeholder for the original
agent-graph-composition intake row (from
`team/intake/2026-05-21-eval-honesty-and-agent-graph.md`). It said
"needs spec" and listed structural concerns the spec must resolve.

The spec landed on 2026-05-22 as PR #518:
`docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`.

The spec converges this intake row with the board-v2.md
"Capability-first agent model" research item. Operator decisions
on all 8 open questions were locked in the spec's "Operator
decisions (2026-05-22)" section.

# Successor contracts

The spec decomposes into 5 phase contracts + 1 deferred UI spec:

- **Phase A** — `team/contracts/agent-graph-capability-schema.md` (ready 2026-05-22) — schema + storage + migration 033
- **Phase B** — `agent-graph-capability-dispatch` (to be decomposed after Phase A merges) — unified `dispatch_capability` seam; rewrites `pipeline.rs`, lifts eval-executor onto same seam, validator update, edge predicate evaluator
- **Phase C** — `agent-graph-filter-capability` (after Phase B) — LLM Filter dispatcher, granularity runtime, in-memory signal cache
- **Phase D** — `agent-graph-unified-recorder` (after Phase B) — structural close of F-11(f); single capability-gated `Recorder` trait
- **Phase E** — `agent-graph-template-capabilities` (after Phase A) — starter templates declare `capabilities`; flips `validate_draft_succeeds_for_fresh_template` to expected-pass
- **Phase F (deferred)** — `docs/superpowers/specs/2026-05-XX-capability-editor-ui.md` (to be authored when Phase A+B+E are in main) — inline-only capability editor in AgentForm per Decision 7

This placeholder is archived. Future agent-graph-composition work routes through the phase contracts.
