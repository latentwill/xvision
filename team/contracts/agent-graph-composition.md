---
track: agent-graph-composition
lane: foundation
wave: eval-honesty-tail-2026-05-22
worktree: .worktrees/agent-graph-composition
branch: task/agent-graph-composition
base: origin/main
status: deferred
depends_on: []
blocks: []
stacking: none
allowed_paths: []
forbidden_paths: []
interfaces_used:
  - xvision_engine::strategies::agent_ref::AgentRef (add `kind: AgentKind` field)
  - xvision_engine::strategies::agent_ref::PipelineKind (existing — extend Graph semantics)
  - xvision_engine::eval::executor (post-trait-extraction unified Executor — confirmed shipped via #487)
  - xvision_engine::tools (Filter signal emission seam)
parallel_safe: false
parallel_conflicts: []
verification: []
acceptance: []
---

# Scope

Formalize `kind` (`trader` / `filter` / `critic` / `intern`) on
`AgentRef` with per-kind I/O contracts; per-Filter `granularity`
field on `AgentRef` (decoupled from other agents' cadence); Filter
emits user-named signals into downstream agents' briefings;
strategy can declare graph edges that short-circuit downstream calls
based on Filter output.

**Deferred — needs spec.** This is the largest unbuilt item from
the eval-honesty intake. The original intake's `depends_on:
executor-refactor` is satisfied (executor refactor shipped via
`executor-trait-extraction` #487 + `live-bar-source-alpaca` #489 +
`live-eval-launch-and-freeze` #497), and `xvision-filters` crate
also shipped via the filter-v1 wave (#485 → #496). But the
capability shape, per-kind I/O contracts, and graph short-circuit
semantics need a written spec before contracts open.

Pairs naturally with board-v2.md's "Capability-first agent model"
research note. Likely those two converge into a single capability +
graph composition spec — they're answering the same question from
different angles.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
row "Agent-graph composition: formalize `kind` on `AgentRef`..."

# Out of scope (until spec)

Until the spec lands, this contract is a placeholder. Do not open
implementation work against it. The spec must specify:

- Capability schema (where `kind` lives vs `role`, vs the future
  capability flags from board-v2.md's "Capability-first" item)
- Per-kind I/O contracts (what does a Filter return; how does that
  get into a downstream briefing; what does Critic produce)
- `granularity` semantics — bar-level, minute-level, decision-level
- Graph edge declaration shape — DAG, allowed cycles, short-circuit rules
- Migration path from current `Strategy { agents: Vec<AgentRef> }` +
  `PipelineKind { Single, Sequential, Graph }`

Expected spec location: `docs/superpowers/specs/2026-05-XX-capability-first-agent-model-and-graph-composition.md`.

# Notes

Before opening, confirm with operator:
- Do we still want explicit `kind` labels, or does the
  capability-first refactor make them implicit?
- Does the post-filter-v1 `xvision-filters` runtime already cover
  the "Filter emits signals into downstream briefings" half, leaving
  only the graph-edge short-circuit work?
