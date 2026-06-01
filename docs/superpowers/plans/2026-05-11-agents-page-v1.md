# Agents page v1 — minimum useful surface

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Source:** Distilled 2026-05-11 from the design-space exploration at `docs/superpowers/research/2026-05-11-agents-page-design-space.md`. That research artifact ran the full feature surface; this plan ships the minimum.

---

## Goal

Ship the **Agents** page as a workspace-level library + escape-valve
authoring surface. Single-agent is the default. Multi-slot composition is
opt-in and entirely user-defined. Templates exist as examples; nothing
enforces specific slot names.

The page replaces the leftover "intern as a top-level concept" scaffolding
from the xianvec era (currently leaking into Home + Settings → Providers)
with a clean "agents are first-class entities, providers are a workspace
default" separation.

## Placement — View C (hybrid)

Operator decision 2026-05-11: agents are **inline by default, with a
library page kept as a nav item for future expansion**.

- **Inline authoring** in `/authoring/:strategy_id` (the Inspector) is the
  canonical path for most users. You build a strategy; agents inside it
  are the default unit of work. Single-agent strategies look like classic
  CRUD; multi-slot agents inside a strategy are opt-in.
- **`/agents` page** is the library. It lists every agent that exists in
  the workspace — whether created inline-in-a-strategy or standalone.
  It's also an escape valve for power users / autooptimizer to manage
  shared definitions without entering a strategy. Empty-state by default
  for first-time users; populates as they create agents.
- **Name kept as "Agents"** for future expansion: autooptimizer
  mutations (SLF9), ERC-8004 attestations (SLF3), marketplace listings
  (Plan 5), reputation leaderboards (F34) all need a stable workspace
  surface; the Agents page becomes that home as features arrive.

The data model is the same either way — every agent has an `Agent` row,
discoverable from the library. There's no "scope" field; an agent is
either referenced by zero strategies (library-only) or N strategies
(deployed). The "deployed in strategies" cross-reference is the canonical
linkage.

## What v1 ships

Three routes plus a sidebar nav entry:

```
/agents                 Library — table of agents with status pill, deployed-in count
/agents/new             Standalone-create form (escape valve)
/agents/:agent_id       Detail view — Identity | Behavior (slots) | Cross-refs
```

The Inspector hook for inline authoring is **stubbed** in v1 (Task 7).
Full Inspector rebuild that makes agents the in-strategy primitive is
part of the downstream strategies refactor — flagged in §Downstream
impact below.

Identity + behavior + cross-refs only. No determinism controls, no
lineage tree, no attestation surface, no marketplace, no live cycles.

## What v1 deliberately defers

Each is an explicit non-goal:

- **Temperature / top_p / sampling controls** — operator decision: too confusing before user has shipped one agent. Re-introduce post-v1 as an "Advanced" expander.
- **Lineage view, fork-with-diff** — needs versioned bundle storage. Until then, "fork" is a copy with no parent linkage.
- **Sealed state + ERC-8004 attestation** — depends on SLF3 (NFT mint on `ab_compare` startup). Out of v1.
- **AutoOptimizer mutation review surface** — depends on SLF9 (evening Karpathy loop). Out of v1.
- **Live cycle / forward-paper / kill switch UX** — depends on the live daemon (Plan 2c). Out of v1.
- **Variance scan, adversarial probe, replay** — Tier-2 features per research artifact. Out of v1.
- **Skill marketplace** — Tier-3, depends on the marketplace contract.
- **Bulk operations** — Tier-3.

## v1 surface — what's actually on the page

### `/agents` (list view)

A table with one row per agent. Columns:

- Name (link to detail)
- Description (one-line summary)
- Status pill: **Draft** / **Validated** / **In use** / **Archived** (simplified four-state vocabulary — see §Status below)
- Last run (timestamp + result, or `—` if never run)
- Deployed in (count of strategies this agent appears in, with link)
- Skills (count + first 1–2 names)
- Updated_at

Top-of-page: **`+ New agent`** button. Filter by status; search by name.

### `/agents/new` and `/agents/:agent_id` (detail view)

Three sections, vertically stacked:

#### 1. Identity

- **Name** (required, unique within workspace)
- **Description** (one-line)
- **Tags** (free-form, comma-separated; optional)

#### 2. Behavior — the slot panel

A list of **named slots**. By default an agent has **one** slot named
`main`. Operator can rename `main` or add additional slots — slot names
are user-defined free text. No enforced vocabulary; "intern" / "trader"
/ "risk" / "executor" are template conventions, not requirements.

Per slot:

- **Slot name** (free text; default `main`; must be unique within agent)
- **Provider + model** (single dropdown, sourced from `/api/settings/providers` — workspace default highlighted but overridable per slot)
- **System prompt** (textarea; resizable; monospace if `> 4 lines`)
- **Skills** (multi-select from available skill registry — see §Skills below)
- **Max tokens** (number input; reasonable per-provider default)

Slot panel actions:

- **`+ Add slot`** — appends a new named slot below
- **`Remove`** (per slot, hidden when only one slot exists — you can't have a zero-slot agent)
- **`Duplicate`** (copies the slot's config under a new name)

#### 3. Cross-references (read-only on first iteration)

- **Deployed in strategies** — list of `strategy_id` → name → link to `/strategies/:id`. Empty state: "Not deployed in any strategy yet."
- **Recent runs** — last N (5 default) eval-runs that touched this agent. Each row: `run_id` short, run name, started_at, verdict pill (Sharpe / return at-a-glance). Link to `/eval/runs/:id`.

### Status vocabulary (v1 simplified)

The research artifact identifies a 9-state machine. V1 collapses it to
four states with simpler transitions:

```
Draft        -> Validated   : passes validation (token budget OK, every slot has a non-empty prompt)
Validated    -> Draft       : operator edits any field
Validated    -> In use      : referenced by at least one strategy (computed, not stored)
In use       -> Archived    : operator explicitly archives (irreversible from UI; restorable via CLI)
Any          -> Archived    : explicit archive action
```

States not in v1: Testing, Sealed, Forward-paper, Live, Halted, Retired,
Mutated. These re-enter when their dependencies (live daemon, ERC-8004
mint, autooptimizer loop) ship.

### Skills (what they are in v1)

A **skill** in v1 is a named entry in a workspace-level skill registry.
Each skill has:

- `skill_id` (ULID)
- `name`
- `description`
- `kind` — `tool` (MCP-style callable) | `prompt_fragment` (prepended to system prompt) | `evaluator` (post-decision check)
- `config_schema` (optional JSON schema for skill-specific config)

The Agents page consumes skills via a multi-select picker. **No skill
authoring on this page** — skills are managed at `/settings/skills`
(stub for v1 — if no skills exist, the picker is empty and labeled
"No skills configured yet. Add some at Settings → Skills.").

If `/settings/skills` doesn't exist by ship time, the picker is hidden
entirely and skills come back in v1.1.

---

## API surface

New endpoints. Add to `crates/xvision-dashboard/src/routes/agents.rs`.

```
GET    /api/agents                    list, with optional ?status=&q=&limit=&page=
POST   /api/agents                    create draft
GET    /api/agents/:id                full agent record
PUT    /api/agents/:id                update (Draft only — Validated requires explicit re-edit which moves it back to Draft)
DELETE /api/agents/:id                archive (soft delete)
POST   /api/agents/:id/validate       run validators; returns Vec<ValidationDiagnostic>
GET    /api/agents/:id/strategies     strategies that reference this agent (denormalized)
GET    /api/agents/:id/runs           recent eval-runs that included this agent
GET    /api/agents/templates          list of starter templates (single-agent + a couple of multi-slot examples)
```

`Agent` record shape (TypeScript view; Rust mirror in `xvision-engine`):

```ts
interface Agent {
  agent_id: string;            // ULID
  name: string;
  description: string;
  tags: string[];
  slots: AgentSlot[];          // length >= 1
  created_at: string;
  updated_at: string;
  archived: boolean;
}

interface AgentSlot {
  name: string;                // user-defined, free text, e.g. "main", "trader", "risk_check"
  provider: string;            // provider name (resolves via /api/settings/providers)
  model: string;
  system_prompt: string;
  skill_ids: string[];
  max_tokens: number;
}
```

Validation rules (returned by `/api/agents/:id/validate`):

- Name non-empty + unique within workspace
- At least one slot
- Every slot has non-empty name + provider + model + system_prompt
- Slot names unique within agent
- Total token budget across slots ≤ provider's context limit (informational; warn not error)
- Skills referenced exist in registry (or registry is empty)

## Downstream impact (BIG — flagged for operator)

This v1 is the thin slice. The underlying domain-model shift has broader
implications the operator explicitly flagged.

### Strategies refactor

Current model: `StrategyBundle` with fixed slot names (intern / trader /
risk / executor) per the CLAUDE.md terminology table.

New model implied by the Agents page: a **strategy is a composition of N
agents**. Each strategy specifies which agents play which role in its
decision pipeline; slot names are per-agent (user-defined), not
per-strategy.

This affects:

- `xvision-engine/src/strategies/` — the bundle struct is replaced with `Strategy { agents: Vec<AgentRef>, pipeline: PipelineDef }`
- Migrations — existing `strategies` table needs an `agents` association table
- `/strategies` and `/authoring/:id` (Inspector) — currently shows the 4-column bundle outline tree (intern / trader / risk / executor). Becomes a graph of agent references with user-defined edge semantics
- Backward compatibility — existing sealed bundles need a one-way migration into the new shape (each fixed slot becomes a generated agent)

### Eval refactor

Eval currently runs a `StrategyBundle` and produces `BacktestResult`
keyed by `cycle_id` (one set of metrics per decision cycle).

New model:

- Eval runs a Strategy (which references N agents)
- `BacktestResult` should track per-agent metrics alongside per-cycle metrics
- The "compare runs" UI needs to handle the case where two runs use different agents (currently it assumes the same slot vocabulary)

### Pipeline-stage names

Per CLAUDE.md terminology, intern / trader / risk / executor are
**roles in the processing pipeline and NOT renamed**. Under the new
model, they remain **valid conventions** for users to adopt — example
templates seed agents with these names — but **are no longer enforced**.
This is a softening, not a rename. Documentation in CLAUDE.md needs an
addendum clarifying the shift from "hardcoded pipeline roles" to
"convention-only role names."

### Sequencing recommendation

1. **Ship Agents page v1** — standalone, doesn't touch strategies or eval. Operator can create + name + parameterize agents; they sit in the workspace as drafts that aren't yet referenced anywhere.
2. **Refactor strategies** to reference agents — new `Strategy` shape, migration of existing bundles, Inspector UX rebuild.
3. **Refactor eval** to track per-agent metrics — `BacktestResult` shape change, dashboard run-detail page update.

Each step is independently shippable. Together they're 4–8 weeks of
work. V1 alone is 1 week.

---

## File structure

```
crates/xvision-engine/src/api/
└── agents.rs                                # NEW — agents domain API

crates/xvision-engine/src/agents/            # NEW
├── mod.rs
├── model.rs                                 # Agent, AgentSlot structs
├── store.rs                                 # AgentStore — SQLx CRUD
└── validate.rs                              # ValidationDiagnostic generation

crates/xvision-data/src/migrations/
└── 20260511000020_agents_table.sql          # NEW — agents + agent_slots tables

crates/xvision-dashboard/src/routes/
└── agents.rs                                # NEW — HTTP routes (list, get, create, update, archive, validate, strategies, runs)

frontend/web/src/
├── routes/
│   ├── agents.tsx                           # NEW — /agents list
│   ├── agents.$id.tsx                       # NEW — /agents/:agent_id detail
│   └── agents.new.tsx                       # NEW — /agents/new draft form
├── api/
│   └── agents.ts                            # NEW — TS client
└── components/agent/                        # NEW
    ├── AgentList.tsx
    ├── AgentDetail.tsx
    ├── SlotPanel.tsx
    ├── SlotForm.tsx
    └── DeployedInStrategies.tsx

docs/superpowers/research/
└── 2026-05-11-agents-page-design-space.md   # already shipped — full design space

docs/superpowers/plans/
└── 2026-05-11-agents-page-v1.md             # this file
```

---

## Tasks

### Task 1 — Data model + migration

**Files:**
- Create: `crates/xvision-data/src/migrations/20260511000020_agents_table.sql`
- Create: `crates/xvision-engine/src/agents/{mod.rs, model.rs}`

- [ ] **Step 1: Migration**

```sql
CREATE TABLE agents (
    agent_id     TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    description  TEXT NOT NULL DEFAULT '',
    tags_json    TEXT NOT NULL DEFAULT '[]',
    archived     INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE agent_slots (
    agent_id      TEXT NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    slot_index    INTEGER NOT NULL,                  -- ordering
    name          TEXT NOT NULL,
    provider      TEXT NOT NULL,
    model         TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    skill_ids_json TEXT NOT NULL DEFAULT '[]',
    max_tokens    INTEGER NOT NULL DEFAULT 4096,
    PRIMARY KEY (agent_id, slot_index)
);

CREATE INDEX idx_agents_archived ON agents(archived);
CREATE INDEX idx_agents_name ON agents(name);
```

- [ ] **Step 2: Rust structs**

```rust
// crates/xvision-engine/src/agents/model.rs
pub struct Agent {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub slots: Vec<AgentSlot>,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct AgentSlot {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
    pub skill_ids: Vec<String>,
    pub max_tokens: u32,
}
```

- [ ] **Step 3: Apply migration and verify**

```sh
sqlite3 /tmp/xvn-agents-test.db < crates/xvision-data/src/migrations/20260511000020_agents_table.sql
sqlite3 /tmp/xvn-agents-test.db ".schema agents agent_slots"
```

- [ ] **Step 4: Commit**

### Task 2 — Engine store + API surface

**Files:**
- Create: `crates/xvision-engine/src/agents/{store.rs, validate.rs}`
- Create: `crates/xvision-engine/src/api/agents.rs`

- [ ] **Step 1: Failing test for round-trip**

```rust
#[sqlx::test]
async fn agent_round_trips(pool: SqlitePool) {
    let store = AgentStore::new(pool);
    let id = store.create(NewAgent {
        name: "BTC mean-rev v1".into(),
        description: "Buys dips on the 15m chart".into(),
        slots: vec![AgentSlot {
            name: "main".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            system_prompt: "You are a mean-reversion trader...".into(),
            skill_ids: vec![],
            max_tokens: 4096,
        }],
        tags: vec![],
    }).await.unwrap();

    let loaded = store.get(&id).await.unwrap();
    assert_eq!(loaded.name, "BTC mean-rev v1");
    assert_eq!(loaded.slots.len(), 1);
    assert_eq!(loaded.slots[0].name, "main");
}
```

- [ ] **Step 2: Implement `AgentStore` with CRUD + slot-list serialization**

- [ ] **Step 3: Implement validators**

```rust
pub struct ValidationDiagnostic {
    pub code: String,        // "name_empty", "slot_name_duplicate", etc.
    pub severity: Severity,  // Error | Warning | Info
    pub message: String,
    pub field: Option<String>,
}

pub fn validate_agent(agent: &Agent) -> Vec<ValidationDiagnostic> { /* ... */ }
```

Rules per the §API surface section.

- [ ] **Step 4: Wire engine API**

```rust
// crates/xvision-engine/src/api/agents.rs
pub async fn list(ctx: &ApiContext, req: ListAgentsRequest) -> Result<ListAgentsResponse> { /* ... */ }
pub async fn create(ctx: &ApiContext, req: CreateAgentRequest) -> Result<Agent> { /* ... */ }
pub async fn get(ctx: &ApiContext, id: &str) -> Result<Agent> { /* ... */ }
pub async fn update(ctx: &ApiContext, id: &str, req: UpdateAgentRequest) -> Result<Agent> { /* ... */ }
pub async fn archive(ctx: &ApiContext, id: &str) -> Result<()> { /* ... */ }
pub async fn validate(ctx: &ApiContext, id: &str) -> Result<Vec<ValidationDiagnostic>> { /* ... */ }
pub async fn deployed_in(ctx: &ApiContext, id: &str) -> Result<Vec<StrategyRef>> { /* ... */ }
pub async fn recent_runs(ctx: &ApiContext, id: &str, limit: u32) -> Result<Vec<RunRef>> { /* ... */ }
```

`deployed_in` returns empty in v1 (strategies don't reference agents
yet — that's the downstream refactor). Hook is there so the UI can call
it without knowing it's empty by design.

- [ ] **Step 5: Run tests; commit**

### Task 3 — Dashboard HTTP routes

**Files:**
- Create: `crates/xvision-dashboard/src/routes/agents.rs`
- Modify: `crates/xvision-dashboard/src/server.rs` — register the new routes

- [ ] **Step 1: Failing integration test**

```rust
#[tokio::test]
async fn create_then_get_agent() {
    let app = test_app().await;
    let create_res = app.post("/api/agents")
        .json(&json!({
            "name": "test-agent",
            "description": "",
            "tags": [],
            "slots": [{
                "name": "main",
                "provider": "anthropic",
                "model": "claude-sonnet-4-6",
                "system_prompt": "You are a trader.",
                "skill_ids": [],
                "max_tokens": 4096,
            }]
        }))
        .await;
    assert_eq!(create_res.status(), 201);
    let id = create_res.json::<Value>()["agent_id"].as_str().unwrap().to_string();

    let get_res = app.get(&format!("/api/agents/{}", id)).await;
    assert_eq!(get_res.status(), 200);
}
```

- [ ] **Step 2: Implement axum handlers wrapping the engine API**

- [ ] **Step 3: Run tests; commit**

### Task 4 — Frontend list view

**Files:**
- Create: `frontend/web/src/routes/agents.tsx`
- Create: `frontend/web/src/api/agents.ts`
- Create: `frontend/web/src/components/agent/AgentList.tsx`
- Modify: `frontend/web/src/App.tsx` — add `/agents` route
- Modify: shell `Sidebar.tsx` — add Agents nav item

- [ ] **Step 1: TS client**

Generate or hand-write from the Rust `xvision-engine::api::agents` types.

- [ ] **Step 2: `AgentList` component**

Columns: name (link), description, status pill (Draft/Validated/In use/Archived), last run, deployed-in count, skills count, updated_at. Search bar + status filter at top. `+ New agent` button.

- [ ] **Step 3: Empty state**

When no agents exist: large illustration + copy "Agents are reusable templates that compose into strategies. Start with a single-slot agent." + primary CTA `+ New agent`.

- [ ] **Step 4: Smoke**

Run dev server, navigate to `/agents`, confirm empty state + create flow round-trip.

- [ ] **Step 5: Commit**

### Task 5 — Frontend detail view + slot panel

**Files:**
- Create: `frontend/web/src/routes/agents.$id.tsx`
- Create: `frontend/web/src/routes/agents.new.tsx`
- Create: `frontend/web/src/components/agent/{AgentDetail.tsx, SlotPanel.tsx, SlotForm.tsx, DeployedInStrategies.tsx}`

- [ ] **Step 1: Identity section** — name, description, tags
- [ ] **Step 2: Slot panel**

Renders slots as a vertical stack of expandable cards. Each card has:
- Header: slot name (editable inline) + provider/model summary + remove icon (hidden when only one slot)
- Body (expanded by default for the first slot): provider+model dropdown, system prompt textarea, skills multi-select, max_tokens input
- Footer: `Duplicate slot` button

Below the last slot: `+ Add slot` ghost button.

**The default agent has one slot named `main`.** Operator can rename it
or add more.

- [ ] **Step 3: Cross-references section**

- "Deployed in strategies" — call `/api/agents/:id/strategies`. Empty state: "Not deployed in any strategy yet."
- "Recent runs" — call `/api/agents/:id/runs?limit=5`. Empty state: "No runs yet."

- [ ] **Step 4: Save / validate flow**

Save calls `PUT /api/agents/:id`. After save, calls `POST /api/agents/:id/validate`. Surfaces warnings inline; surfaces errors as a save block.

- [ ] **Step 5: Commit**

### Task 6 — Starter templates + onboarding

**Files:**
- Modify: `crates/xvision-engine/src/api/agents.rs` — add `templates()` endpoint
- Create: `crates/xvision-engine/src/agents/templates.rs` — bundled starter templates

- [ ] **Step 1: Three starter templates**

- **Single-prompt trader** — one slot named `main` with a baseline trader system prompt
- **Two-stage analyst → executor** — two slots demonstrating multi-slot composition with conventional names
- **Risk-checked trader** — three slots: `trader`, `risk_check`, `executor` — showing one possible convention

Each is a JSON file in `crates/xvision-engine/src/agents/templates/` and is loaded at startup.

- [ ] **Step 2: `/api/agents/templates` endpoint** — returns the list

- [ ] **Step 3: Frontend template picker on `/agents/new`** — three cards above the empty form; clicking a card pre-fills the form

- [ ] **Step 4: Smoke + commit**

### Task 7 — Sidebar wire-up + Home tile

**Files:**
- Modify: `frontend/web/src/components/shell/Sidebar.tsx` — add Agents nav item between Strategies and Eval
- Modify: `frontend/web/src/routes/home.tsx` — add a small "Agents" tile showing count + quick `+ New agent` link

- [ ] **Step 1: Sidebar add**
- [ ] **Step 2: Home tile**
- [ ] **Step 3: Commit**

---

## Self-review

**Spec coverage** — seven tasks, ordered by dependency:

| Task | Touches | Ships |
|---|---|---|
| 1 | `xvision-data` migration + `xvision-engine` model | Tables + structs |
| 2 | `xvision-engine` store + API + validators | CRUD round-trip + validation |
| 3 | `xvision-dashboard` HTTP routes | REST surface |
| 4 | Frontend `/agents` list view | List + create flow |
| 5 | Frontend detail view + slot panel | Edit flow with slots |
| 6 | Starter templates | Onboarding affordance |
| 7 | Sidebar + Home tile | Discoverability |

**v1 scope discipline:**

- No temperature / sampling controls (operator decision)
- No lineage, no fork-with-diff, no Sealed state, no live cycles
- Skills picker degrades gracefully when registry is empty
- `deployed_in` returns empty until the strategies refactor lands — UI handles the empty case as the canonical path

**Backward compatibility:**

- New tables only; no changes to `strategies` / `eval_runs` / `cycles` tables in v1
- No CLAUDE.md terminology table changes in v1 (the softening happens during the downstream strategies refactor)

**Type/name consistency:**

- `agent_id` (per CLAUDE.md — pre-mint local ULID)
- Slot `name` is user-defined free text — not constrained to `intern` / `trader` / `risk` / `executor`
- Existing terminology stays valid as convention

**Dependencies:**

- Skills page (`/settings/skills`) — soft dependency; v1 ships with skills picker hidden if registry is empty
- Workspace provider settings — hard dependency; uses existing `/api/settings/providers` surface
- LLM dispatch path — hard dependency; existing `xvision-dashboard::llm_dispatch` covers it

**Open questions for the operator** (flag before execution):

1. **Slot ordering** — does the order of slots have implied semantics in v1 (e.g. "first slot is the primary"), or is it purely visual? Proposal: purely visual; if there's a primary it's named, not positional.
2. **Skill picker degrade** — if no skills registry exists at all, hide the multi-select entirely or show a disabled placeholder linking to `/settings/skills`? Proposal: hide, with a single-line explainer.
3. **`name` uniqueness scope** — workspace-wide (one operator) or per-tenant when we add multi-user? Proposal: workspace-wide for v1; revisit at multi-tenant.
4. **`max_tokens` default** — same per all providers, or provider-specific? Proposal: provider-specific defaults read from `/api/settings/providers` metadata (4096 baseline; 8192 for Claude/GPT-class).

**Effort estimate:** 1 week for a single engineer doing Tasks 1–5
(skipping templates + sidebar polish until the core round-trip works).
Add 2–3 days for Tasks 6–7. Total: ~1.5 weeks if done linearly; can
parallelize Tasks 1–3 (engine) from Tasks 4–5 (frontend) once Task 3
defines the API contract.

**What this plan does NOT solve** (gracefully deferred to the downstream refactor):

- Strategies stop being fixed-slot bundles — see "Downstream impact" above
- Eval tracks per-agent metrics — same
- Migration of existing sealed bundles into the new shape — same
- Pipeline-stage names soften from enforced to conventional — CLAUDE.md amendment

These are flagged as 4–8 weeks of follow-up work that the operator
explicitly anticipated when scoping this v1.
