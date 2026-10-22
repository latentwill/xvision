# Plan 2 — Multi-Agent Routing and Named Handoffs

> **For Hermes:** use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this task-by-task.

**Goal:** Expand xvision’s multi-agent workflow so agents can be named, chained, branched, and routed with explicit handoffs — while keeping the existing `Single` / `Sequential` behavior as the compatibility baseline and preserving clear provenance for every decision.

**Architecture:** Build on xvision’s existing `PipelineDef` / `PipelineEdge` model instead of inventing a parallel routing system. The implementation should add a stable node identity layer, explicit handoff envelopes, and optional router / aggregator behavior on top of the current graph execution path. Sequential mode remains the simplest default: legacy strategies keep loading as `Single`, and multi-agent graphs remain additive. The key is explicit provenance — every run should answer who produced what, who consumed it, and which edge or router made that happen.

**Tech Stack:** Rust 2021 workspace (`xvision-engine`, `xvision-dashboard`, `xvision-cli`), current strategy graph / pipeline types, capability dispatch, serde/ts-rs, prompt templates, trace / compare surfaces.

**Reference docs:**
- `crates/xvision-engine/src/strategies/*` — current strategy / pipeline model
- `crates/xvision-engine/src/agent/pipeline.rs` — execution and slot resolution
- `crates/xvision-engine/src/agent/dispatch_capability.rs` — capability-based dispatch
- `crates/xvision-engine/src/agent/filter_dispatch.rs` — gating and filter flow
- `crates/xvision-dashboard/wiki/xvision-vs-agentic-trading-research.md` — gap analysis bridge doc
- `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`
- `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`
- `docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md`

---

## Brainstorm: routing patterns worth considering

The gap analysis suggests a few useful routing shapes:

- **Static chain** — `Researcher -> Trader -> Risk -> Execution`
  - best for simple sequential workflows
  - easy to reason about
  - not very adaptive

- **Fan-out / reduce** — one evidence producer feeds multiple specialists, then an aggregator merges them
  - good for regime/news/portfolio/risk decomposition
  - strong for compare/eval because each branch is inspectable

- **Conditional router** — a lightweight routing step chooses the next agent based on regime, asset, or risk state
  - useful for adaptive workflows
  - must stay deterministic and transparent

- **Capability graph** — edges route capability-compatible agents rather than hard-coded role names
  - best long-term fit with xvision’s capability-first direction

Recommended path:
1. Preserve sequential mode as the simplest default.
2. Add stable node IDs and named handoffs.
3. Add router / aggregator only as explicit graph nodes or capabilities.
4. Record lineage keys in trace and compare output so routing stays inspectable.

---

## Why this matters

The current system already has multiple agents inside a strategy, but the routing semantics are still too implicit for the workflow suggested by the gap analysis. The user wants more than “more prompts”; they want named agents that can feed one another with clear provenance. That means the routing layer has to answer:

- what the agent is called,
- what artifact it produced,
- what the next agent consumed,
- why the handoff occurred,
- and whether the path was sequential, branched, or routed.

If this remains vague, the routing layer becomes harder to debug than the current pipeline rather than more useful.

---

## Proposed shape

### Identity model

Use three distinct identifiers:

- **node ID** — immutable strategy-graph identity used for routing, traces, and compare
- **display name** — user-facing label that can be renamed
- **agent ID / binding** — the underlying model / provider binding

This avoids the common bug where a renamed agent breaks provenance or old runs become unreadable.

### Routing modes

Support the following modes explicitly:

- **Single** — existing legacy baseline; one agent or one decision source
- **Sequential** — a linear chain of named agents
- **Graph** — a broader graph with fan-out, fan-in, and optional router / aggregator nodes

### Handoff envelope

Every cross-agent handoff should be a typed envelope with at least:

- handoff ID
- producer node ID
- consumer node ID
- artifact type
- schema/version
- payload or payload reference
- allowed pass-through fields
- provenance metadata
- router / edge decision reason, if any

The envelope should be visible both at runtime and in traces.

### Router / aggregator semantics

If the plan introduces router and aggregator nodes, they must be constrained:

- router inputs must be declared, not implicit
- router decisions must be deterministic or at least reproducible from recorded inputs
- no-match behavior must be explicit
- aggregator conflict resolution must be defined up front
- provenance must survive merges

---

## Implementation plan

### Task 1: Add stable node identity to strategy graphs

**Objective:** Separate human labels from routing identity.

**Files:**
- Modify: `crates/xvision-engine/src/strategies/*`
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`

**Work:**
- Add a stable `node_id` / `ref_id` for each agent node in a strategy graph.
- Keep `display_name` editable and human-facing.
- Keep underlying agent/provider bindings separate from both.
- Preserve backward compatibility with existing role-based strategies.

**Verification:**
- Renaming a node does not break routing or traces.
- Existing strategies still load.
- The UI can show names and identities clearly.

---

### Task 2: Formalize explicit handoff artifacts

**Objective:** Make upstream-to-downstream communication visible and typed.

**Files:**
- Modify: `crates/xvision-engine/src/agent/pipeline.rs`
- Modify: `crates/xvision-engine/src/agent/*`
- Modify: `crates/xvision-core/src/trading.rs` if needed for shared payload types

**Work:**
- Define a handoff envelope with versioning and provenance metadata.
- Store producer / consumer node IDs and a payload summary.
- Make the downstream agent read a named artifact rather than an implicit shared context blob.
- Keep the payload minimal and explicit.

**Verification:**
- The trace shows a concrete handoff from A to B.
- The downstream agent can reproduce what it consumed.
- The envelope serializes and round-trips.

---

### Task 3: Preserve `Single` / `Sequential` as the compatibility baseline

**Objective:** Keep old strategies runnable while adding richer routing later.

**Files:**
- Modify: `crates/xvision-engine/src/agent/pipeline.rs`
- Modify: `crates/xvision-engine/src/strategies/mod.rs`
- Add tests next to the pipeline code

**Work:**
- Keep `Single` as the default serialized / parsed mode for legacy or empty strategies.
- Keep `Sequential` as the simplest multi-agent wiring.
- Do not require router or aggregator nodes for a basic strategy.
- Make the new graph model additive, not mandatory.

**Verification:**
- Existing strategy serialization still works.
- A simple sequential strategy still runs without extra routing primitives.
- Regression tests cover old and new loading behavior.

---

### Task 4: Add optional router and aggregator nodes

**Objective:** Support branching and consolidation when the workflow actually needs it.

**Files:**
- Modify: `crates/xvision-engine/src/agent/*`
- Modify: `crates/xvision-engine/src/strategies/*`
- Modify: `crates/xvision-dashboard/src/routes/authoring.tsx`

**Work:**
- Add a router node that chooses the next agent from declared candidates.
- Add an aggregator node that merges multiple upstream artifacts into a downstream artifact.
- Keep both nodes optional.
- Record the router decision and the aggregation policy in traces.

**Verification:**
- A fan-out/fan-in workflow can be expressed and executed.
- The router choice is visible in traces.
- Aggregation does not discard provenance.

---

### Task 5: Add capability-based routing rules

**Objective:** Let strategy graphs route by capability and context, not just names.

**Files:**
- Modify: `crates/xvision-engine/src/agent/dispatch_capability.rs`
- Modify: `crates/xvision-engine/src/agent/filter_dispatch.rs`
- Modify: `crates/xvision-engine/src/strategies/*`

**Work:**
- Allow edges to specify a capability compatibility rule.
- Keep role labels as fallback behavior.
- Use capability matching to support future specialist agents without rewiring the whole graph.

**Verification:**
- A route can be chosen by capability.
- Name-based routing still works.
- Capability mismatches fail clearly.

---

### Task 6: Surface routing in the dashboard and compare views

**Objective:** Make multi-agent workflows legible to the user.

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`
- Modify: `crates/xvision-dashboard/src/routes/eval_runs.rs`
- Modify: `frontend/web/src/routes/*` strategy and compare views

**Work:**
- Show named nodes and handoff edges in strategy detail.
- Show which node produced which evidence in eval traces.
- Make compare output distinguish sequential vs graph-routed vs branched workflows.
- Add explicit lineage fields rather than reconstructing routing from text.

**Verification:**
- The user can see the routing graph.
- Per-node provenance is inspectable.
- Compare output makes routing differences obvious.

---

### Task 7: Add tests for provenance and routing correctness

**Objective:** Prevent hidden handoffs and accidental graph ambiguity.

**Files:**
- Add tests adjacent to pipeline, strategy, and routing modules

**Work:**
- Test node identity stability.
- Test handoff envelope serialization and round-trip.
- Test sequential compatibility.
- Test router no-match / aggregator conflict behavior.

**Verification:**
- Tests cover both happy path and failure path.
- No silent fallthrough.
- Provenance remains stable.

---

## Acceptance criteria

- Agents can be named in a user-visible way.
- One agent can explicitly feed another agent.
- `Single` and `Sequential` continue to work as compatibility modes.
- Optional routers and aggregators can be added without breaking old strategies.
- Handoff provenance is visible in traces and compare output.
- Capability-based routing is possible, not just role-based routing.

---

## Risks and concerns

- A router can become a hidden super-agent if it can inspect too much or branch silently.
- If node identity is not stable, old traces will become unreadable.
- Too much routing machinery can make simple strategies harder to understand.
- If provenance is not surfaced in the data model, compare output will be brittle.

Mitigation:
- Keep routing primitives minimal and explicit.
- Keep `Single` / `Sequential` as the baseline.
- Make lineage keys first-class.
- Prefer explicit handoff artifacts over implicit shared context.

---

## High-level recommendation

Use the gap analysis to justify a **transparent graph**, not a hidden orchestrator. The right xvision routing model is one where each agent has a stable identity, each edge is explicit, and every downstream agent can be told exactly what it is consuming.
