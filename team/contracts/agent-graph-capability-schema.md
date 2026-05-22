---
track: agent-graph-capability-schema
lane: foundation
wave: agent-graph-2026-05-22
worktree: .worktrees/agent-graph-capability-schema
branch: task/agent-graph-capability-schema
base: origin/main
status: ready
depends_on: []
blocks:
  - agent-graph-capability-dispatch
  - agent-graph-filter-capability
  - agent-graph-unified-recorder
  - agent-graph-template-capabilities
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agents/model.rs
  - crates/xvision-engine/src/agents/capability.rs
  - crates/xvision-engine/src/agents/store.rs
  - crates/xvision-engine/src/strategies/agent_ref.rs
  - crates/xvision-engine/migrations/033_agent_slot_capabilities.sql
  - crates/xvision-engine/migrations/033_agent_slot_capabilities.down.sql
  - crates/xvision-engine/tests/agent_slot_capabilities.rs
  - crates/xvision-engine/tests/strategy_pipeline_edge_predicate.rs
  - team/MANIFEST.md
forbidden_paths:
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/eval/executor/**
  - crates/xvision-observability/**
  - frontend/web/**
  - crates/xvision-cli/**
  - crates/xvision-mcp/**
interfaces_used:
  - xvision_engine::agents::model::AgentSlot (add `capabilities: BTreeSet<Capability>` field)
  - xvision_engine::agents::store::AgentStore (insert + load round-trip the new column)
  - xvision_engine::strategies::agent_ref::AgentRef (add `activates: Option<Capability>` field; serde-default = None)
  - xvision_engine::strategies::agent_ref::PipelineEdge (add `condition: Option<EdgePredicate>` field; serde-default = None — unconditional)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --check
  - cargo clippy -p xvision-engine --tests -- -D warnings
  - cargo test -p xvision-engine --test agent_slot_capabilities
  - cargo test -p xvision-engine --test strategy_pipeline_edge_predicate
  - cargo test -p xvision-engine
  - cargo build --workspace  # confirm no other crate breaks from the new fields
acceptance:
  - `Capability` enum exists at `crates/xvision-engine/src/agents/capability.rs` with closed variants `{Trader, Filter, Critic, Intern, Router}` and ts-rs derive
  - `AgentSlot.capabilities: BTreeSet<Capability>` exists with `#[serde(default = "default_capabilities")]` returning `BTreeSet::from([Capability::Trader])` for back-compat
  - SQLite migration 033 adds `agent_slot_capabilities` (or `agent_slots.capabilities`) — operator's choice of normalized join table vs JSON column; spec recommends JSON for v1 simplicity
  - `AgentStore::insert_slot` writes the capability set; `load` round-trips it; on rows without the column (legacy pre-migration rows), `capabilities` defaults to `{Trader}`
  - `AgentRef.activates: Option<Capability>` (NEW field; `#[serde(default)]`) — when `None`, runtime picks the slot's first capability (which is `Trader` for legacy rows)
  - `PipelineEdge.condition: Option<EdgePredicate>` (NEW field; `#[serde(default)]`) — when `None`, edge is unconditional
  - `EdgePredicate` enum closed `{Eq, Neq, Gte, Lte, In, All, Any, Not}` per spec Decision 5; ts-rs derived
  - Tests: round-trip serialize/deserialize for AgentSlot + AgentRef + PipelineEdge with and without the new fields; storage insert+load preserves the set; legacy JSON without the new fields still parses (back-compat regression guard)
  - `cargo build --workspace` passes — every Rust struct-literal site outside `allowed_paths` keeps compiling because the new fields all have `#[serde(default)]` AND a Rust-side `Default` impl on the parent struct OR explicit constructor helpers (use whatever pattern AgentSlot already uses for `memory_mode`)
  - team/MANIFEST.md migration registry updated: row 033 marked reserved → merged when this lands
---

# Scope

Phase A of `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md` (merged via PR #518). Schema + storage groundwork only — no dispatch logic, no runtime behavior change. The schema lands so Phase B can build the unified `dispatch_capability` seam on top.

This contract implements Decisions 1, 2, 4, 5, 8 from the spec. Decisions 3, 6, 7 are runtime-side and land in later phases. Operator decisions locked 2026-05-22 (in the spec's "Operator decisions" section) — no decisions remain open for this contract.

# What lands in this PR

1. **`Capability` enum** — closed `{Trader, Filter, Critic, Intern, Router}` at `crates/xvision-engine/src/agents/capability.rs` (NEW). Derives `Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize`. Behind the `ts-export` feature, derives `ts_rs::TS` with `#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]` so the frontend gets the type for free.

2. **`AgentSlot.capabilities: BTreeSet<Capability>`** — additive field on `AgentSlot` in `crates/xvision-engine/src/agents/model.rs`. Default = `{Trader}` (preserves today's behavior — every existing slot is implicitly a Trader). Uses the same `#[serde(default = "...")]` + ts-rs pattern as the surrounding fields.

3. **`AgentRef.activates: Option<Capability>`** — additive field on `AgentRef` in `crates/xvision-engine/src/strategies/agent_ref.rs`. `None` = the runtime picks the slot's first capability in `BTreeSet` order. For legacy rows that means `Trader`.

4. **`PipelineEdge.condition: Option<EdgePredicate>`** — additive field on `PipelineEdge`. `None` = unconditional edge (current behavior). The `EdgePredicate` enum lives alongside `PipelineEdge` in `agent_ref.rs`; closed set per Decision 5: `Eq { signal_field: String, value: serde_json::Value }`, `Neq { ... }`, `Gte { ... }`, `Lte { ... }`, `In { signal_field: String, values: Vec<Value> }`, `All(Vec<EdgePredicate>)`, `Any(Vec<EdgePredicate>)`, `Not(Box<EdgePredicate>)`. Predicates evaluate against the upstream agent's `FilterSignal.payload` — Phase B implements the evaluator; this contract only persists the shape.

5. **Migration 033** — `crates/xvision-engine/migrations/033_agent_slot_capabilities.sql` + `.down.sql`. Recommended shape: add `capabilities TEXT NOT NULL DEFAULT '["trader"]'` to the `agent_slots` table (JSON column, parsed as `BTreeSet<Capability>` at load time). Alternative: normalized join table `agent_slot_capabilities (slot_id, capability)`. Pick JSON for v1 — smaller change, no migration-time foreign-key drama, and capabilities-per-slot is small enough (≤5) that JSON read overhead is negligible. Operator can revisit if it ever becomes a real bottleneck.

6. **`AgentStore` round-trip** — `insert_slot` and the slot-load query parse and serialize the column. The store layer is the source of truth; the in-memory `AgentSlot` is what every other call site sees.

7. **Tests** — `tests/agent_slot_capabilities.rs` (NEW):
   - Round-trip serde — `AgentSlot { capabilities: {Trader, Critic} }` ↔ JSON
   - Back-compat — legacy JSON without `capabilities` field deserializes with default `{Trader}`
   - Migration apply — fresh DB after migration 033 has the column; rows inserted before migration default correctly on next read
   - Store round-trip — insert a slot with `{Trader, Critic}`, load it back, assert the set is preserved
   - `AgentRef.activates` round-trip; `None` legacy default
   - `tests/strategy_pipeline_edge_predicate.rs` (NEW) — round-trip the 8 `EdgePredicate` variants; `None` legacy default on `PipelineEdge.condition`; predicate parser doesn't accept unknown variant strings

# What this PR explicitly does NOT do

- No `dispatch_capability` seam (Phase B)
- No Filter granularity runtime (Phase C)
- No unified Recorder trait (Phase D)
- No starter-template `capabilities` retrofit (Phase E)
- No UI rendering of capabilities or `activates` (Phase F)
- No edge-predicate evaluation logic (Phase B's job; this contract persists the shape only)
- No deprecation of `AgentRef.role` (Decision 1: role stays as display-only label forever)
- No engine-side behavior change AT ALL — this is pure schema + storage. The runtime continues to dispatch via the existing role-string path until Phase B lands.

# Migration number reservation

Reserves migration **033** in `team/MANIFEST.md` (migration 032 was conditionally reserved for `memory-provenance-in-decisions-trace`; this contract takes 033 to keep the registry monotonic regardless of whether 032 ever lands).

# Hard rules

- Do NOT edit `crates/xvision-engine/src/agent/pipeline.rs` or `execute.rs` (forbidden — Phase B's territory).
- Do NOT edit `crates/xvision-engine/src/eval/executor/**` (forbidden).
- Do NOT touch `crates/xvision-observability/**` (forbidden).
- Do NOT touch `frontend/web/**` (Phase F).
- Do NOT touch `crates/xvision-cli/**` or `crates/xvision-mcp/**` (no CLI/MCP exposure of capabilities in this phase; if a CLI verb needs an update for the field add, push back to the conductor — the right shape is a follow-up contract).
- No try/catch silencing; no `#[allow(...)]` to mute clippy.
- No comments documenting the operator decisions inline — those live in the spec.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/agent-graph-capability-schema -b task/agent-graph-capability-schema origin/main
```

Set `CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"` before cargo invocations.

# Notes

This is the contract that **unblocks** every other Phase B–F contract. Land cleanly first. The spec is at `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`; cross-reference it for any ambiguity. The operator-decisions section at the bottom of the spec is authoritative.
