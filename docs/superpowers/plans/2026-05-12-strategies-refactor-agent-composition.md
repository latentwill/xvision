# Strategies refactor — agent composition

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Source:** Followup to `docs/superpowers/plans/2026-05-11-agents-page-v1.md` §Downstream impact. Agents page v1 introduced reusable agent records; this plan reshapes strategies to reference them.

---

## Goal

Replace the fixed-slot `StrategyBundle` with a **composition of N agents**.
A strategy says "these agents play these roles in my pipeline"; each
agent is an independent record (in the workspace agent library) with its
own prompt/model/skills. Single-agent strategies look like CRUD; multi-
agent strategies become explicit pipelines.

Closes the central ambiguity exposed by the agents page: "agent" was
overloaded between the immutable template (now `Agent` in the library)
and the strategy-internal slot. The slot ceases to be a fixed structural
concept — it's now just an Agent reference with a role label.

## Why now

Three forces converge:

1. **Agents page v1 created the substrate.** Reusable `Agent` records exist; nothing references them yet. The library is empty for the productive use cases until strategies start referencing them.
2. **The slot-name confusion is half-solved.** Pipeline-stage names softened from "NOT renamed" to "valid conventions" (FOLLOWUPS pending). The other half is the strategy still hardcoding those slot names.
3. **Eval refactor is blocked.** Per-agent metrics (`2026-05-12-eval-per-agent-metrics.md`) needs strategies to reference agents before it can attribute decisions.

## Architecture

```
Before (current):
  StrategyBundle {
    manifest: PublicManifest,
    regime_slot: Option<LLMSlot>,    // fixed name
    intern_slot: Option<LLMSlot>,    // fixed name
    trader_slot: Option<LLMSlot>,    // fixed name
    risk: RiskConfig,
    mechanical_params: serde_json::Value,
  }

  LLMSlot { role: String, prompt, model_requirement, allowed_tools }
       ^^^^ slot role is hardcoded to intern/trader/regime

After (this plan):
  Strategy {
    manifest: PublicManifest,
    agents: Vec<AgentRef>,           // 1+ agent references, ordered
    pipeline: PipelineDef,           // how to wire them together
    risk: RiskConfig,                // strategy-level risk, separate from agent
    mechanical_params: serde_json::Value,
  }

  AgentRef {
    agent_id: String,                // FK to agents table
    role: String,                    // user-defined role this agent plays IN THIS STRATEGY
  }

  PipelineDef {
    kind: PipelineKind,              // Sequential | Single | (future: Graph)
    edges: Vec<PipelineEdge>,        // empty for Single/Sequential
  }

  PipelineKind { Single, Sequential, Graph }
  PipelineEdge { from_role: String, to_role: String }
```

### Why role-on-AgentRef (not on Agent)

An `Agent` is a reusable template. Its slot names live inside the agent
("main", "trader", whatever). When a strategy uses an agent, it gives
that agent a **role in the strategy's pipeline** — possibly different
from any of the agent's slot names. E.g., one `Agent` might appear in
Strategy A as `analyst` and in Strategy B as `oracle`.

Role lives on the reference, not the referent.

### Single-agent strategies stay simple

```
Strategy {
  agents: vec![AgentRef { agent_id: "01HZ…", role: "trader" }],
  pipeline: PipelineDef { kind: Single, edges: vec![] },
}
```

UI for a single-agent strategy looks identical to UI for an agent —
because most of the strategy IS the agent. The strategy adds the risk
config and the mechanical params; the trader brain is the agent.

### Storage decision: filesystem stays, agents are SQL

Bundles remain filesystem-backed (`$XVN_HOME/strategies/<id>.json`)
because:

- Sealed bundles are content-addressed artifacts; filesystem JSON is the natural shape
- ERC-8004 attestations hash the bundle JSON; changing storage would invalidate them
- Bundle hashes are the FK in `eval_runs.strategy_bundle_hash`

Agents are SQL-backed (already shipped). When a strategy references an
agent, the bundle JSON carries the `agent_id` string; resolution at run
time goes through `AgentStore`. Bundle hashes don't depend on the agent's
internal state — they hash the strategy structure + referenced agent_ids.

**Implication for sealed bundles:** if an agent is mutated after a
strategy seals, the strategy's behavior changes. To preserve sealing,
either:
- Fork the agent (creates new agent_id, strategy still references old one)
- Sealed bundles record the agent's content-hash at seal time and verify on load

Sealed-bundle agent fingerprinting is **out of scope for this plan** —
it's a sealing-engine concern. v1 of this refactor accepts that
strategy-referenced agents are mutable and warns on edit if the strategy
is sealed (see Task 8).

## What changes

### Engine

- **New types** in `crates/xvision-engine/src/bundle/`:
  - `AgentRef { agent_id, role }`
  - `PipelineDef { kind, edges }`
  - `PipelineKind`, `PipelineEdge`
- **Strategy struct** replaces `StrategyBundle`'s slot fields with `agents: Vec<AgentRef>` + `pipeline: PipelineDef`. Other fields (`manifest`, `risk`, `mechanical_params`) unchanged.
- **Authoring** (`crates/xvision-engine/src/authoring/`):
  - `add_agent(bundle_id, agent_id, role)` / `remove_agent(...)` / `set_pipeline(bundle_id, def)`
  - Old `update_slot(bundle_id, slot_role, ...)` deprecated; legacy slot names map to Agent lookups for one release
- **Bundle store** unchanged structurally — different JSON shape.

### Migration of existing bundles

One-shot script:

```
For each bundle on disk:
  1. For each non-null slot (regime/intern/trader):
     - Find an existing Agent with matching prompt+model+tools, OR
     - Create a new Agent with name = "<bundle_name>__<slot_role>"
       and one slot named "main" containing the prompt/model/tools
     - Append AgentRef { agent_id, role: slot_role } to bundle.agents
  2. Set bundle.pipeline = PipelineDef { kind: Sequential, edges: [] }
     (sequential ordering matches the legacy regime → intern → trader pipeline)
  3. Remove the slot fields (or leave as deprecated until next major)
```

Run as `xvn strategy migrate-agents` CLI command — operator-triggered,
not automatic, so they can see the diff before committing.

### Dashboard API

- `GET /api/strategies` — unchanged shape (StrategySummary), new fields added (agent_count, primary_role)
- `GET /api/strategy/:id` — returns the new Strategy shape; legacy slot fields **also returned** during deprecation window for backward-compat with the existing Inspector (which gets rebuilt below)
- `PUT /api/strategy/:id/slot/:role` — kept as a deprecated compatibility shim that translates into agent operations; new clients use the new endpoints below
- **New endpoints:**
  - `POST /api/strategy/:id/agents` — add an agent reference
  - `DELETE /api/strategy/:id/agents/:role` — remove a role binding
  - `PUT /api/strategy/:id/pipeline` — set the pipeline def
  - `POST /api/strategy/:id/promote-agent/:role` — copy an inline agent to the library (for the "Promote to library" path)

### Frontend — Inspector rebuild

Current 4-column desktop layout (sidebar · bundle outline · split editor
· validation rail) becomes 3-column:

```
sidebar | strategy outline | agent editor | validation rail
  200      220               flex           280
```

Strategy outline (column 2):

- **Identity** (name, description, tags) — same as agents page identity panel
- **Agents** — list of AgentRefs with role badges. Each row links to either the inline editor in column 3 or the standalone /agents/:id page (toggle).
- **Pipeline** — visual representation of the PipelineDef:
  - Single: "1 agent"
  - Sequential: "agent A → agent B → agent C" (role names)
  - Graph: (out of v1) custom edge editor
- **Risk** — link to risk envelope editor (existing surface)

Agent editor (column 3):

- Reuses `AgentForm` component from agents-page-v1
- When editing an agent that's also in the library, an "edit-in-place affects all strategies using this agent" warning shows
- "Promote to library" affordance for inline (strategy-scoped) agents that haven't been promoted yet

### CLI

- `xvn strategy create --template <id> --agent <agent_id>:<role>` (multi-agent: repeat `--agent`)
- `xvn strategy add-agent <strategy_id> <agent_id> --role <role>`
- `xvn strategy remove-agent <strategy_id> --role <role>`
- `xvn strategy set-pipeline <strategy_id> --kind <single|sequential>`
- `xvn strategy migrate-agents [--dry-run]` (the one-shot migration above)
- Old `xvn strategy set-slot` deprecated — prints a warning + maps to the new commands.

### MCP

Same pattern: new tools for `add_agent` / `set_pipeline`, old `update_slot` keeps working with a deprecation warning.

## Backward compat strategy

Two-release window:

1. **This release** — both old (slots) and new (agents) shapes work side-by-side. Engine reads either; writes new. Frontend reads either; new Inspector writes new. Old Inspector keeps working against the deprecation shim. CLI prints deprecation warnings for slot ops.
2. **Next release** — old slot fields removed from the bundle JSON. Deprecation shim removed. Frontend Inspector rebuilt under the new model.

Eval-runs that have `strategy_bundle_hash`s referring to old-shape
bundles continue to work — the eval engine re-reads the bundle JSON;
if it's the old shape, it operates on slots as it did before. The
migration script bumps bundle hashes; the original eval-run records
keep their original hashes (they reference historical state).

## File structure

```
crates/xvision-engine/src/bundle/
├── mod.rs                          # MODIFY — export new types
├── agent_ref.rs                    # NEW — AgentRef, PipelineDef, PipelineKind, PipelineEdge
├── strategy.rs                     # NEW — Strategy (replaces StrategyBundle)
├── store.rs                        # MODIFY — load/save new shape; back-compat reader
└── migrate.rs                      # NEW — one-shot migration logic

crates/xvision-engine/src/authoring/
├── mod.rs                          # MODIFY — export new fns
├── agent_ref.rs                    # NEW — add_agent, remove_agent, promote_agent
├── pipeline.rs                     # NEW — set_pipeline
└── slot.rs                         # MODIFY (existing update_slot becomes deprecation shim)

crates/xvision-engine/src/api/strategy.rs   # MODIFY — new endpoints
crates/xvision-dashboard/src/routes/strategies.rs   # MODIFY — wire new routes

crates/xvision-cli/src/commands/strategy.rs # MODIFY — new subcommands + deprecation warnings

frontend/web/src/
├── routes/authoring.tsx             # MODIFY — Inspector rebuild
├── components/inspector/
│   ├── StrategyOutline.tsx          # NEW — column 2
│   ├── PipelineEditor.tsx           # NEW — pipeline visualization + edit
│   └── AgentSlot.tsx                # NEW — one agent reference row in the outline

docs/superpowers/plans/
└── 2026-05-12-strategies-refactor-agent-composition.md   # this file
```

## Tasks

### Task 1 — New types + migration of bundle shape

- New types in `crates/xvision-engine/src/bundle/`
- Bundle JSON gains `agents`, `pipeline` fields; old slot fields kept (deprecated marker comment)
- Store reads either shape; writes new
- Unit tests: round-trip, mixed-shape load (old → new), validation

### Task 2 — Authoring API

- `add_agent` / `remove_agent` / `set_pipeline` / `promote_agent` in `engine::authoring`
- Each emits an audit row
- Unit tests per fn

### Task 3 — Migration command

- `xvn strategy migrate-agents --dry-run` walks bundle store, computes the new shape, prints a diff
- Without `--dry-run`, writes back + emits report
- Idempotent — re-running on already-migrated bundles is a no-op
- Integration test: temp bundle store with 3 legacy bundles → migrate → verify shape

### Task 4 — Dashboard routes

- New routes per §API surface above
- Deprecation header on old slot routes (`X-Deprecated: true`)
- Integration tests for each new route

### Task 5 — CLI subcommands

- New `xvn strategy add-agent` / `remove-agent` / `set-pipeline` / `migrate-agents`
- `xvn strategy set-slot` keeps working with deprecation warning to stderr
- CLI tests

### Task 6 — Inspector rebuild

- Three-column layout (strategy outline + agent editor + validation rail)
- Strategy outline: identity, agents list, pipeline, risk
- Agent editor: reused `AgentForm` from agents-page-v1
- Promote-to-library affordance
- Smoke test: create a strategy, add two agents, set sequential pipeline, run preview

### Task 7 — Update existing strategies tests

- The `strategy.rs` API tests assume slot-shaped bundles. Update to new shape.
- Old-shape tests kept but renamed `*_legacy` until deprecation window closes.

### Task 8 — Edit-on-sealed-bundle warning

- When the operator edits an agent that's referenced by a sealed bundle, warn that the change affects historical attestations
- Engine returns a `referenced_by_sealed: bool` on `GET /api/agents/:id`
- Frontend renders the warning banner

### Task 9 — Documentation updates

- Update DESIGN.md §6.4 (Inspector) to describe the new column structure
- Update CLAUDE.md terminology table to note the strategy ↔ agent relationship
- Update HACKATHON-1-PAGER.md if it mentions slot structure

## Self-review

**Estimated effort:** 1.5–2 weeks single-engineer. Tasks 1–4 (engine + API)
~5 days; Task 5 (CLI) ~1 day; Task 6 (Inspector rebuild) ~3–4 days;
Tasks 7–9 ~2 days. Parallelizable across engineers along the engine/CLI/frontend split.

**Risk areas:**

- **Sealed-bundle integrity** — agents are mutable; sealed bundles can silently change behavior. Task 8 surfaces this; full fix is a sealing-engine concern out of scope here.
- **Eval-run replay** — eval results store `strategy_bundle_hash`. Replaying a historical run requires the bundle JSON at that hash. The new shape's hash is different; old hashes still resolve to old JSON in the store. Verified by Task 3's idempotence test.
- **MCP tool churn** — any MCP integration calling old slot tools will break at the deprecation removal. One-release deprecation window is the mitigation.
- **Inspector UX regression** — three-column → smaller working area for the agent editor. Mitigated by reusing `AgentForm` so the editor surface is already well-sized.

**What this plan does NOT solve:**

- Sealed-bundle agent fingerprinting (Task 8 only warns)
- Graph pipelines (PipelineKind::Graph is the type-system slot; no editor in v1)
- Cross-strategy agent dependency analysis ("if I edit this agent, which strategies break?")
- Skill resolution when an agent references a skill that doesn't exist in the workspace (Task 9 of skills registry plan covers part of this)

**Sequencing:**

- Tasks 1–3 (data model + migration) before everything else
- Task 4 (API) parallel with Task 5 (CLI) — both call into the new authoring fns
- Task 6 (Inspector) blocked by Task 4
- Task 7 (test updates) parallel with Tasks 4–6
- Task 8 (sealed warning) after Task 4
- Task 9 (docs) last, captures the final shape

**Open questions for the operator:**

1. **Single-vs-Sequential as default for new strategies.** Proposal: Single (one agent, no pipeline). Operator picks Sequential when adding the second agent.
2. **Inline-vs-library default for new agents created during strategy authoring.** Proposal: inline (strategy-scoped) until explicitly promoted. Avoids polluting the library with one-off configs.
3. **Migration trigger.** Proposal: opt-in via `xvn strategy migrate-agents`. Alternative: auto-migrate on first load post-upgrade. Auto is safer for users who don't read release notes; opt-in gives more control.
