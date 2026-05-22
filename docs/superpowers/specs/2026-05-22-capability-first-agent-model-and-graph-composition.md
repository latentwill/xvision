# Capability-first agent model + graph composition

**Date:** 2026-05-22
**Surface:** `crates/xvision-engine/src/{agents,strategies,agent}/**`, `crates/xvision-filters/**`, frontend agent + strategy editors, `xvn strategy validate` / `xvn agent` CLI verbs.
**Status:** Draft for operator review (spec only — no implementation in this PR).
**Replaces:** the deferred `team/contracts/agent-graph-composition.md` placeholder, and the "Capability-first agent model" research note in `team/board-v2.md`.
**Related:**
- `team/contracts/agent-graph-composition.md` — the deferred contract this spec unblocks.
- `team/intake/2026-05-21-eval-honesty-and-agent-graph.md` row "Agent-graph composition: formalize `kind`…" — original intake.
- `team/board-v2.md` "Capability-first agent model" research note — the second half this spec absorbs.
- `team/intake/2026-05-19-eval-traces-end-to-end-audit.md` F-11(f) — the empty-recorder-tables finding folded into Phase D.
- `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md` — the `Strategy { agents: Vec<AgentRef> }` refactor this spec extends.
- `crates/xvision-engine/src/strategies/agent_ref.rs` — current `AgentRef` + `PipelineKind` shape.
- `crates/xvision-engine/src/agent/pipeline.rs` — current `run_agent_pipeline` dispatcher (role-string-gated).
- `crates/xvision-filters/src/runtime.rs` — `RuntimeFilter` + `ActivationDecision` substrate this spec wires into agent graphs.

## Goal

Define one capability model that resolves three problems at once:

1. **Role-string overloading.** `AgentRef.role` is a free-text label today. The dispatcher branches on `canonical_role(role) == "trader"` to pick the response schema; the validator requires a slot named `trader`; the seeded templates name slots `risk_check` / `executor` / `equities_trader` / `router` etc. with no enforcement. A typo (`"traderr"`) downgrades a trader to a no-op JSON-passthrough. A user-facing label decides engine behavior — that's the bug class this spec eliminates.

2. **`PipelineKind::Graph` is a hatch with no runtime.** The on-disk JSON can carry `kind: "graph"` and a `Vec<PipelineEdge>` of `{from_role, to_role}` since the strategies refactor, but no editor authors them, no runtime short-circuits on them, and `Filter`-output-conditioned routing is unrepresentable.

3. **Empty recorder tables for eval-driven runs (F-11(f)).** The harness emits `tool_calls`, `events`, `supervisor_notes`, `approvals`, `sandbox_results`, `checkpoints`, `artifacts`; the eval executor takes a parallel path and emits none of them. The asymmetry is the role/capability confusion in another form — there's no single place to ask "did this agent produce a tool call?" because there's no single typed contract for "agent producing X".

The fix is one shape: an agent declares which **capabilities** it can perform (Trader, Filter, Critic, Intern, Router). The dispatcher selects behavior by capability, not by role string. The strategy graph routes outputs by typed channel, not by string match. The recorder pipeline is one capability-gated path that both harness and eval-executor invoke.

## Decisions

These are the load-bearing calls the spec locks. The operator should accept or reject them at review time before any phase opens. Items where the spec deliberately leaves the call open are listed in **Open questions** at the bottom.

1. **`capabilities: BTreeSet<Capability>` lives on `AgentSlot`, not on `AgentRef`.** Capabilities are intrinsic to the agent (the prompt + tools + skill set determines whether the agent can act as a trader, not the strategy it's wired into). The strategy picks **which** of the agent's capabilities is **active** in this pipeline position via a new `AgentRef.activates: Capability` field. The same agent can appear twice in the same strategy under different active capabilities if it declares both.

2. **`Capability` is a closed enum.** `{ Trader, Filter, Critic, Intern, Router }` for v1. Closed because the dispatcher must select a typed I/O contract per capability — an open string set defers the schema decision to runtime, which is the bug class this spec exits. Adding a sixth capability is a deliberate engine change with a per-capability I/O contract added in the same PR.

3. **`AgentRef.role` is retained as a free-text display label.** No deprecation. The role label is what shows up on the strategy diagram, the run trace, the eval review surface — operators name slots `"momentum_trader"` and `"vol_regime_filter"` and that survives. Validation no longer reads `role`; it reads `activates`. The legacy slot-name-must-be-`"trader"` check in `validate_strategy` is replaced by "at least one `AgentRef` has `activates: Capability::Trader`".

4. **Per-capability I/O contracts are typed Rust enums, not free-form JSON.** A `Trader` produces `TraderDecision`. A `Filter` produces `FilterSignal { name, payload, granularity }`. A `Critic` produces `Critique`. An `Intern` produces `InternObservation`. A `Router` produces `RouteSelection { to: BTreeSet<String> }` — the role-label names of the downstream `AgentRef`s to invoke. All four go through a single `AgentOutput` sum type the dispatcher and recorder share.

5. **Filter granularity lives on `AgentRef`, decoupled from other agents' cadence.** `AgentRef.granularity: FilterGranularity = { Bar, Minute, Decision }` is meaningful only when `activates == Filter`; ignored otherwise. Bar = re-evaluate on every new bar. Minute = re-evaluate on a fixed 1-minute tick (bar-aligned by truncation). Decision = re-evaluate only when a downstream Trader is about to be invoked. The runtime caches the last `FilterSignal` per (filter ref, granularity) and re-fires it into downstream briefings until the next eval point invalidates it.

6. **Filter signals reach downstream briefings via a typed `signals: BTreeMap<String, FilterSignal>` field on the dispatcher's accumulated context, keyed by the producing `AgentRef.role`.** The downstream agent sees the signal under its producer's role label — `signals["regime_filter"].payload = {"regime": "high_vol"}`. The trader briefing JSON is built by the dispatcher; it adds a top-level `filter_signals` object verbatim. The agent's `system_prompt` references the signal by the role name the operator chose — symmetric with how `accumulated[format!("{role_key}_output")]` already works for sequential pipelines.

7. **Graph edges declare short-circuits via predicates over named signals; no in-edge code is run.** `PipelineEdge` extends to `{ from_role, to_role, condition: Option<EdgePredicate> }` where `EdgePredicate` is a typed comparison against a `FilterSignal` payload field (`signal: "regime_filter", field: "regime", op: "eq", value: "high_vol"`). No JS / DSL / shell-out — just a closed predicate enum the validator and editor both understand. `condition: None` is an unconditional edge; matches the current `PipelineEdge` shape exactly for back-compat.

8. **Edges are DAG-only for v1.** The validator rejects cycles. Re-entrancy / loops are out of scope until a v2 design proves a use case (the operator's "agent-walks-back" research note is the natural place that would surface). Default fall-through when no outgoing edge matches: continue to the next `AgentRef` in strategy order — same as today's Sequential behavior. This means `kind: Graph` with zero edges behaves identically to `kind: Sequential` — the simplest possible migration shape.

9. **The dispatcher is rewritten around capability dispatch in Phase B and is a single function used by both harness and eval executor.** Today `crates/xvision-engine/src/agent/pipeline.rs::run_agent_pipeline` is the harness path; `crates/xvision-engine/src/eval/executor/**` calls a parallel dispatch. Phase B collapses both onto one `dispatch_capability(&Capability, &Briefing, &mut Context) -> AgentOutput` seam. This is the structural fix that closes F-11(f) — when both surfaces share the dispatch call, they share the recorder call.

10. **Unified capability-gated recorder.** Phase D ships `CapabilityRecorder` — a `&dyn Recorder` the dispatcher hands to each capability handler. Every emission (`tool_call`, `event`, `supervisor_note`, `approval`, `sandbox_result`, `checkpoint`, `artifact`) goes through the recorder regardless of which surface invoked the dispatch. The harness wraps it in OTel spans; the eval executor wraps it in trace blobs. Both surface variants now fill the same recorder tables — the operator's eval-review panel stops being a half-truth.

11. **Migration is back-compat-only in the spec phase; one SQLite migration in Phase A for the new column.** No on-disk strategy JSON is invalidated. Strategies authored before this spec land continue to parse, validate, and run. A populator runs at first read against existing `AgentSlot` rows: if the slot's role label `canonical_role(role) == "trader"`, populate `capabilities = {Trader}` and the corresponding `AgentRef.activates = Trader`. All other strategies get `capabilities = {Trader}` by default — a deliberate choice that mirrors today's "if you don't have a trader, you can't run" validator, so no existing strategy gains a capability it didn't already implicitly have. Operators set richer capability sets through the agent editor when they're ready.

12. **`default-capability-set` ships on every starter template in Phase E.** Every entry in `crates/xvision-engine/src/agents/templates.rs::builtin_templates()` declares an explicit `capabilities` field on each `AgentSlot` and the matching `AgentRef.activates` on the strategy's `agents` list. The `validate_draft_succeeds_for_fresh_template` test (expected-fail on `main` as of 2026-05-20 per PR #369) flips to expected-pass when Phase E lands. The default-capability-set on `single-trader` is `{Trader}`; on `risk-checked-trader` it's `{Trader}` + `{Critic}` + `{Trader}` across the three slots (the risk_check slot is a Critic, the executor slot is a second Trader-capable agent the operator can swap); on `regime-aware-trader` it's `{Filter}` + `{Trader}`; etc.

## Scope

This spec covers:

- The shape of `Capability`, `AgentSlot.capabilities`, `AgentRef.activates`.
- The four / five typed I/O contracts (`TraderDecision`, `FilterSignal`, `Critique`, `InternObservation`, `RouteSelection`).
- How `FilterGranularity` is honored by the runtime — bar / minute / decision.
- The extended `PipelineEdge` shape with typed edge predicates.
- The migration path from `Strategy { agents, pipeline.kind }` as it stands today.
- The unified capability-gated recorder seam (closes F-11(f)).
- The default-capability-set on starter templates (closes the
  `validate_draft_succeeds_for_fresh_template` expected-fail).
- The 5-phase implementation skeleton the conductor decomposes from.
- Open questions for operator resolution.

This spec does NOT cover:

- Cross-agent async coordination / message bus / fan-out workers — agent dispatch stays per-decision, single-threaded inside one cycle.
- Per-cycle conditional re-routing based on a Trader's own `TraderDecision` output (i.e. "if the trader says hold, route to the critic and re-decide"). Defer to a v2 spec — the v1 graph is Filter-conditioned only.
- Inter-strategy composition (strategy-of-strategies). Deferred to V2C marketplace work where strategies become first-class on-chain objects.
- The dashboard UI for the capability editor or the edge editor. A follow-up `2026-05-XX-capability-editor-ui.md` will scope the agent-editor and strategy-pipeline-editor surfaces. v1 ships JSON-only edge authoring.
- Cortex memory's relationship to capability — V2D landed `memory_mode` per slot and is independent of capability. A Filter-capable slot can also use memory.
- Re-entrancy / loops / iterative refinement. v1 is DAG-only.
- `Algorithm`-trait pipeline-stage code (the rule-based eval baseline trait under `xvision-eval`). That trait is a separate substrate for non-LLM arms in A/B compare; the agent-graph rebuild is LLM-pipeline-only.

## Why this shape

Before locking into the decisions above, two alternatives were considered.

**Alternative 1 — `kind` on `AgentRef` (per-strategy override).** Each strategy declares "this agent acts as a trader here, as a critic there." Rejected because it duplicates the type contract at every wire site. Two strategies referencing the same agent could declare incompatible kinds, and the agent's prompt — which is what actually determines what the agent can produce — has no veto. The operator's "trade is a capability, not an identity" framing (board-v2.md) means the agent owns its capability set; the strategy picks which to activate. That's decision 1.

**Alternative 2 — single `Capability` per agent (no set).** Simpler shape, one field. Rejected because it loses the "a router can also be a trader" composability the operator named as the v1 goal. A multi-role agent (e.g. a generalist that the operator wants to use as either a Trader in one strategy or a Critic in another) would need duplicated agent records. With `BTreeSet`, one agent record covers both — the strategy picks via `activates`. The cost is one extra field per `AgentRef`; the benefit is that agents become first-class reusable across role positions.

**Why `AgentRef.role` survives as a label rather than getting deprecated.** The label is what the operator authors and the UI renders. The capability is what the engine selects on. Forcing the label to match a capability name (`"trader"`, `"filter"`, …) collapses the namespace and breaks the multi-trader case (the `multi-asset-router-with-traders` template has three trader-capable slots named `equities_trader`, `crypto_trader`, `fx_trader`). Display labels and engine semantics being separate fields is correct shape; collapsing them is the bug.

## Type shapes

The structs and enums below are illustrative. The exact field set is locked by this spec; the exact module placement is up to the implementing track.

```rust
// crates/xvision-engine/src/agents/capability.rs (new)

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Produces a TraderDecision (action + conviction + justification).
    /// Consumed by risk + executor.
    Trader,
    /// Produces a FilterSignal at the granularity declared on the AgentRef.
    /// Consumed as a named signal in downstream briefings + edge predicates.
    Filter,
    /// Produces a Critique (approve / reject / suggest_modification + rationale)
    /// against a prior agent's output. Pure observer; never produces a
    /// trading action of its own.
    Critic,
    /// Produces an InternObservation (free-form structured note added to the
    /// briefing for downstream agents). Pure observer; cheap; often run on
    /// every bar to seed context.
    Intern,
    /// Produces a RouteSelection (set of downstream role labels to invoke).
    /// Coordinates other capabilities; never itself emits a TraderDecision.
    Router,
}

// crates/xvision-engine/src/agent/output.rs (new)

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentOutput {
    Trader(TraderDecision),
    Filter(FilterSignal),
    Critic(Critique),
    Intern(InternObservation),
    Router(RouteSelection),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSignal {
    /// The producing AgentRef.role — same label the downstream briefing
    /// will see under `filter_signals[name]`.
    pub name: String,
    /// Free-form JSON payload. Edge predicates reference fields by string
    /// key (see EdgePredicate).
    pub payload: serde_json::Value,
    /// The granularity at which this signal was produced.
    pub granularity: FilterGranularity,
    /// Bar timestamp this signal was computed on. Stale-signal detection.
    pub ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterGranularity {
    /// Re-evaluate on every new bar from the scenario / live feed.
    #[default]
    Bar,
    /// Re-evaluate on the closest minute-aligned tick. Requires a 1-minute
    /// bar feed to be available; falls back to Bar with a runtime warning
    /// if the strategy's primary timeframe is coarser than 1m.
    Minute,
    /// Re-evaluate only when a downstream Trader is about to be invoked.
    /// Useful for expensive filter LLM calls — the trader's cadence
    /// determines the filter's cadence.
    Decision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Critique {
    /// The role label of the AgentRef this critique evaluates.
    pub target_role: String,
    pub verdict: CritiqueVerdict,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CritiqueVerdict { Approve, Reject, SuggestModification }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternObservation {
    /// Producer's role label (the briefing surface uses this as the field name).
    pub label: String,
    /// Free-form JSON note. Briefing includes it under `intern_notes[label]`.
    pub note: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteSelection {
    /// Role labels of the downstream AgentRefs to invoke this cycle.
    /// Order is preserved; duplicates dedup'd by the dispatcher.
    pub to: Vec<String>,
    pub rationale: String,
}
```

`AgentSlot` gains exactly one field:

```rust
pub struct AgentSlot {
    // … (all existing fields preserved) …

    /// Capabilities this slot can act as. Strategies pick which one is
    /// active per AgentRef via AgentRef.activates. Persisted as a JSON
    /// array on `agent_slots.capabilities` (migration N+1, see Phase A).
    /// Defaults to `{Trader}` on deserialize for back-compat with rows
    /// authored before migration N+1.
    #[serde(default = "default_capabilities")]
    pub capabilities: BTreeSet<Capability>,
}

fn default_capabilities() -> BTreeSet<Capability> {
    let mut s = BTreeSet::new();
    s.insert(Capability::Trader);
    s
}
```

`AgentRef` gains two fields:

```rust
pub struct AgentRef {
    pub agent_id: String,
    pub role: String,             // unchanged — display label

    /// The capability this AgentRef activates on the referenced agent.
    /// Validation: the referenced AgentSlot must declare this capability
    /// in its `capabilities` set.
    /// Defaults to Trader on deserialize so legacy strategy JSON parses.
    #[serde(default = "default_activates")]
    pub activates: Capability,

    /// Filter-only: how often the runtime re-evaluates this filter.
    /// Ignored when `activates != Filter`.
    #[serde(default, skip_serializing_if = "is_default_granularity")]
    pub granularity: FilterGranularity,
}

fn default_activates() -> Capability { Capability::Trader }
fn is_default_granularity(g: &FilterGranularity) -> bool { *g == FilterGranularity::Bar }
```

`PipelineEdge` extends:

```rust
pub struct PipelineEdge {
    pub from_role: String,
    pub to_role: String,

    /// When `Some`, the edge fires only if the predicate evaluates true
    /// against the named FilterSignal in the dispatcher's signal map.
    /// When `None`, the edge is unconditional — same shape as today.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<EdgePredicate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EdgePredicate {
    /// `filter_signals[signal].payload[field] == value`
    Eq    { signal: String, field: String, value: serde_json::Value },
    /// `filter_signals[signal].payload[field] != value`
    Neq   { signal: String, field: String, value: serde_json::Value },
    /// Numeric `>=` against a Number-typed payload field.
    Gte   { signal: String, field: String, value: f64 },
    Lte   { signal: String, field: String, value: f64 },
    /// `filter_signals[signal].payload[field] in [values…]`
    In    { signal: String, field: String, values: Vec<serde_json::Value> },
    /// Conjunction of sub-predicates.
    All   { all: Vec<EdgePredicate> },
    /// Disjunction.
    Any   { any: Vec<EdgePredicate> },
    /// Negation.
    Not   { not: Box<EdgePredicate> },
}
```

The predicate set is deliberately tiny. The operator's request is "short-circuit on Filter output," not "embed a query language." Any predicate that needs more than `eq` / `gte` / `in` belongs in the producing Filter's logic, not in the edge.

## Per-capability I/O contracts

### `Trader`

**Consumes** (from the dispatcher-built briefing):
- `bar_history: Vec<Ohlcv>` (slice size honors `AgentSlot.bar_history_limit`)
- `portfolio_state: PortfolioState` (positions, cash, equity)
- `scenario: ScenarioHeader` (symbol, timeframe, fees, slippage model)
- `filter_signals: BTreeMap<String, FilterSignal>` — every signal the strategy's Filter-capable agents have produced this cycle, keyed by producer role.
- `intern_notes: BTreeMap<String, serde_json::Value>` — every Intern observation produced upstream this cycle.
- `critique: Option<Critique>` — if a Critic ran before this Trader in the graph and produced a verdict referencing this Trader's role.

**Produces:** `AgentOutput::Trader(TraderDecision)` — `{ action, conviction, justification, target_size_pct, stop_pct, take_profit_pct, … }`. Same shape as today; no schema change.

**Routing:** Output goes to the risk layer (`xvision-engine/src/risk/**`) and then to the executor (`xvision-engine/src/eval/executor/**` for eval; broker dispatch for live). Output does NOT feed back into any downstream agent in v1 — graph edges don't fork off a TraderDecision (deferred to v2).

**Failure mode:** If the provider call fails or the JSON doesn't parse to `TraderDecision`, the harness's existing recovery state machine (per `2026-05-15-xvn-agent-run-system-spec.md`) takes over: retry / fail-the-cycle / synthetic-hold per agent-run-system rules. No new failure mode per capability — the recovery seam is shared.

### `Filter`

**Consumes:**
- `bar_history: Vec<Ohlcv>` (full window — Filters need indicator math context).
- `scenario: ScenarioHeader`
- Optionally `portfolio_state` if `AgentSlot.system_prompt` references it. (Filters can be position-aware — that's the `wake_when_in_position` knob in `xvision-filters` today.)
- NO upstream `filter_signals` from other Filters this cycle. Filters do not consume each other's output; that's a deliberate restriction to keep the per-bar evaluation order simple. If an operator wants "filter A AND filter B," they author one Filter with both conditions, or they use an edge predicate's `EdgePredicate::All` over two named signals on the downstream Trader.

**Produces:** `AgentOutput::Filter(FilterSignal { name, payload, granularity, ts })`. The dispatcher inserts this into the cycle's `filter_signals` map under `name`.

**Routing:** No downstream agent invocation by the Filter itself. The Filter's output sits in the signal map; downstream agents read it; graph edges may short-circuit invocation of downstream agents based on edge predicates that reference it.

**Granularity handling:**
- `Bar` — runtime calls `dispatch_capability(Capability::Filter, …)` on every new bar.
- `Minute` — runtime calls on every 1m tick; falls back to `Bar` with a `granularity_fallback` event if the scenario timeframe is coarser than 1m.
- `Decision` — runtime defers the call until a downstream Trader is about to be invoked. If the Trader is gated by an edge whose predicate references this Filter's signal, the runtime evaluates the Filter first, then the edge predicate, then the Trader (or skip).

**Failure mode:** If a Filter dispatch fails, the cycle treats the filter as producing `FilterSignal::unknown(name)` — payload is `null`, edge predicates resolve to false (Edge::All collapses to false; Edge::Any falls through to other clauses; Edge::Not against null is false; etc.). The cycle does not abort. A `findings.filter_dispatch_failed` row is emitted for the eval review surface. Recovery is best-effort per cycle; persistent failure across cycles trips a per-run finding.

**Substrate:** `xvision-filters` is the runtime today. It already evaluates a Filter (the DSL one) on every bar and emits an `ActivationDecision`. The capability-typed `Filter` agent (which is an LLM, not a DSL filter) is the LLM-driven sibling. Both produce `FilterSignal` shapes (the DSL filter maps `ActivationDecision::Active { transition }` to `FilterSignal { payload: {"active": true, "transition": "trip"} }`). The runtime contract: a `FilterSignal` produced this cycle has the same shape whether it came from an LLM Filter agent or the existing DSL `xvision-filters` runtime. That's the seam that lets the two coexist without a separate edge-predicate variant.

### `Critic`

**Consumes:**
- The current cycle's briefing (same shape a Trader would see).
- The output of one or more upstream agents in the same cycle. Wiring: the Critic's `AgentSlot.system_prompt` references the upstream role label; the dispatcher passes `target_role` and the matching prior `AgentOutput` into the briefing.

**Produces:** `AgentOutput::Critic(Critique { target_role, verdict, rationale })`.

**Routing:** Output goes into the cycle's `critique_for: BTreeMap<String, Critique>` map keyed by `target_role`. If a downstream Trader's role matches a Critic's `target_role`, the Trader's briefing surfaces the critique under `critique: Some(…)`. Otherwise the Critique is recorded but inert this cycle — useful for eval review even when the strategy doesn't act on it.

**Failure mode:** Same as Trader recovery seam. Critic failures do not abort the cycle; the downstream Trader sees `critique: None`.

### `Intern`

**Consumes:**
- The current cycle's briefing (lightweight — Interns often skip `bar_history` or use a short slice via `bar_history_limit`).
- Filter signals if the operator wires them in via the prompt.

**Produces:** `AgentOutput::Intern(InternObservation { label, note })`.

**Routing:** Output goes into the cycle's `intern_notes: BTreeMap<String, serde_json::Value>` map keyed by `label`. Downstream agents (Trader, Critic, even another Intern) see it on their briefing under `intern_notes[label]`.

**Failure mode:** Intern failures are non-fatal — the downstream `intern_notes` map simply omits that label. Eval review surfaces the failure as a recorder event.

**Why Intern is its own capability and not just an unguarded LLM call:** The Intern contract is the seam for "context-enrichment" agents that produce side data without producing trading actions. Today operators jam these into a slot named `intern` and rely on the dispatcher's sequential accumulation — fragile, role-string-dependent, and the output ends up as a JSON-string in `accumulated["intern_output"]` rather than a typed observation. Capability-first makes it a first-class shape.

### `Router`

**Consumes:**
- The current cycle's briefing.
- Optionally the Filter signal map (Routers often condition on regime).

**Produces:** `AgentOutput::Router(RouteSelection { to: Vec<String>, rationale })`.

**Routing:** The dispatcher consumes the `to` list and invokes only those `AgentRef`s downstream of the Router this cycle. AgentRefs NOT in `to` are skipped — same shape as graph edges with edge predicates, but driven by an LLM decision rather than a static Filter-signal comparison. This is the v1 mechanism for the `multi-asset-router-with-traders` template.

**Failure mode:** Router failure falls back to "invoke every downstream AgentRef" — the no-route-decision equivalent. A `findings.router_dispatch_failed` row records the failure.

**Why Router is distinct from "edges that depend on a Filter signal":** A Filter is bounded (its output is a structured signal; the runtime can re-fire the cached signal). A Router is an LLM call whose decision is the routing — it must run before the routed agents, and its output is itself a routing instruction, not a signal that an edge predicate consumes. Keeping them distinct avoids confusing "Filter that gates downstream invocation" (Filter + edge) from "Router that decides among downstream invocations" (Router, no edge needed).

## Graph runtime semantics

Each strategy cycle runs the following steps:

1. **Collect** all `AgentRef`s in `Strategy.agents`. Their order is the **fallback order** — used when no Router or edge selects a different path.

2. **Run all `Filter`-capability agents first**, in `Strategy.agents` order. Honor each one's `granularity` field:
   - `Bar`: always re-evaluate.
   - `Minute`: re-evaluate if the current bar's minute differs from the cached signal's minute.
   - `Decision`: defer until step 4.

   Each Filter that runs writes its `FilterSignal` into `signals[ref.role]`. Filters that cache (Minute / Decision) re-emit the cached `FilterSignal` into the map so downstream consumers always see the freshest value at this cycle's first-bar moment.

3. **Run any `Intern`-capability agents** in `Strategy.agents` order. Each writes its `InternObservation` into `intern_notes[ref.role]`.

4. **Build the traversal queue** for this cycle:

   a. If a `Router`-capability agent exists, run it first (after Filter + Intern so it can condition on signals + notes). Apply its `RouteSelection.to` as the active downstream set.

   b. Else, start with the set of `AgentRef`s of capability `Trader` or `Critic` not already run.

5. **Traverse** the resulting set in `Strategy.agents` order, applying edge predicates:

   - For each candidate `AgentRef` X in the traversal queue:
     - Find all edges with `to_role == X.role`.
     - If any of those edges has `condition: Some(p)` and `p` evaluates false against `signals`, skip X (the edge gated X out).
     - Otherwise, dispatch X. Any `Decision`-granularity Filters X depends on (referenced in X's briefing-prompt or in an edge predicate targeting X) are evaluated lazily here.

   - After dispatch, write X's `AgentOutput` to the cycle's output channel:
     - `Trader` → cycle trader decision (only one Trader output per cycle reaches risk; if multiple Traders ran, the LAST in `Strategy.agents` order wins, unless a Router selected a single one).
     - `Critic` → `critique_for[X.target_role]`.
     - `Intern` → already written in step 3 (no Interns in this loop).
     - `Filter` → already written in step 2 (Filters don't appear in this loop; their dispatch is step 2 + lazy in step 5 for `Decision` granularity only).
     - `Router` → already processed in step 4a.

6. **Hand off** the chosen `TraderDecision` to risk + executor. If no Trader produced output (e.g. all Traders gated out by edges), emit a `trader_skipped_by_graph` finding and synthesize a `hold` decision — same shape as the existing `trader-noop-skip` behavior. Eval review surfaces the gate as the cause.

7. **Recorder pipeline** (Phase D): every step above goes through `CapabilityRecorder`. The recorder is the single seam that emits `tool_calls`, `events`, `supervisor_notes`, `approvals`, `sandbox_results`, `checkpoints`, `artifacts` to the harness AND the eval store. Today these are emitted only on the harness path; Phase D wires both surfaces through the same recorder.

## Migration path

The migration story is "no operator action required for any existing strategy or agent." Both backwards compatibility constraints are real — pre-mint marketplace strategies and pre-spec workspace strategies must continue to run.

### Strategy JSON on disk

Every existing strategy JSON file has either:

- **Pre-refactor shape**: `trader_slot` / `intern_slot` / `regime_slot` populated, `agents: []`, `pipeline: {kind: single}`. These survive — the dispatcher's legacy path runs them under the existing role-string dispatch in pipeline.rs. Phase B's new capability dispatch is gated behind `agents.len() > 0`; the legacy path is preserved unchanged.

- **Post-refactor shape (current)**: `agents: [AgentRef { agent_id, role }, …]`, `pipeline: { kind, edges }`. These deserialize cleanly under the extended `AgentRef` shape:
  - `activates` missing → defaults to `Trader` (matches today's behavior — every AgentRef is implicitly a trader-or-pass-through).
  - `granularity` missing → defaults to `Bar`.
  - `pipeline.edges[].condition` missing → `None`, an unconditional edge (matches today's shape).

No on-disk JSON is rewritten. The serializer omits defaulted fields via `skip_serializing_if`, so a strategy saved before this spec round-trips byte-identically until the operator deliberately edits the capability or granularity.

### `agent_slots` SQLite column

Phase A adds one migration (number TBD by the conductor at implementation time — do not reserve a number in this spec):

```sql
ALTER TABLE agent_slots ADD COLUMN capabilities TEXT NOT NULL DEFAULT '["trader"]';
```

Existing rows backfill to `["trader"]`. The store layer parses this column into `BTreeSet<Capability>` on read. The default mirrors today's "every slot is implicitly trader-eligible" assumption, so no existing agent loses functionality.

### Validation

`crates/xvision-engine/src/strategies/validate.rs::validate_strategy` is updated in Phase B:

- The existing "must have a trader_slot OR an agent with role `trader`" check becomes "must have an AgentRef whose `activates == Trader`, OR a legacy `trader_slot` populated."
- A new check rejects `AgentRef { activates: C }` when the referenced `AgentSlot.capabilities` does not contain `C`. The dispatcher must never dispatch an agent to a capability it doesn't declare.
- A new check rejects edges whose `condition` references a signal name that no Filter-capability AgentRef in the strategy produces.
- A new check rejects cycles in the resulting DAG (`from_role → to_role` graph).
- The legacy `trader_slot` requirement is retained on pre-refactor strategies; for `agents`-populated strategies, the capability check supersedes it.

### Dispatcher

Phase B replaces the role-string switch in `pipeline.rs` with a capability switch. The switch is dispatched off `agent_ref.activates`, not off `canonical_role(role)`. Operators who renamed their `"trader"` slot to `"momentum_trader"` get correct behavior immediately on first run after Phase B; today they silently get pass-through JSON because the role string doesn't match `"trader"`.

### Eval executor

Phase B also lifts the parallel eval executor onto the same dispatcher. The eval path stops maintaining its own `(role) -> behavior` table; it calls the unified `dispatch_capability` seam. This is the root-cause fix for F-11(f) — the recorder asymmetry exists because the dispatchers were separate.

### CLI

`xvn agent edit` gains a `--capabilities` flag (Phase E) and a `--add-capability` / `--remove-capability` pair for granular edits. `xvn strategy edit` gains `--activates` on the AgentRef-add subcommand and `--granularity` for Filter activations.

No CLI verb is renamed. The `xvn strategy` verb still manages strategies; `xvn agent` still manages agents — same surface, with extra subcommands.

### Frontend

UI surfaces land in a follow-up `2026-05-XX-capability-editor-ui.md` spec — not in this one. v1 ships JSON-only edge authoring. The agent editor and strategy diagram already render `role` labels; they continue to render those, plus a "capabilities" chip on agents and an "activates" badge on AgentRefs. Edge predicates remain JSON-edit-only until the dedicated UI spec lands.

### What does NOT get deprecated in this spec

- `AgentRef.role` (display label).
- `xvn strategy` / `xvn agent` CLI verbs.
- `Strategy.regime_slot` / `intern_slot` / `trader_slot` (legacy LLMSlot fields kept until every workspace strategy is known to be migrated, then deprecated in a future cleanup spec).
- `PipelineKind::{Single, Sequential, Graph}` — all three remain; the new shape lives on Graph, but Single + Sequential continue to behave as today.
- `xvision-filters` DSL Filter runtime — independent substrate, continues to emit `FilterSignal`-shaped output for graph predicates.

## Unified capability-gated recorder (Phase D)

The F-11(f) finding from the 2026-05-19 eval-traces audit observed:

> tool_calls, events, supervisor_notes, approvals, sandbox_results, checkpoints, artifacts are all empty for eval-driven runs because the harness side and the eval-executor side maintain parallel emission paths.

The root cause is that today the dispatcher is two functions, not one. Each maintains its own emission seam. The harness emits OTel spans + recorder rows; the eval executor emits trace blobs only.

The Phase D fix is structural, not piecemeal:

1. Phase B's `dispatch_capability` seam is the single entry point both surfaces share.
2. Phase D defines a `Recorder` trait:

   ```rust
   pub trait Recorder: Send + Sync {
       fn record_tool_call(&self, call: ToolCall);
       fn record_event(&self, event: AgentEvent);
       fn record_supervisor_note(&self, note: SupervisorNote);
       fn record_approval(&self, approval: Approval);
       fn record_sandbox_result(&self, result: SandboxResult);
       fn record_checkpoint(&self, checkpoint: Checkpoint);
       fn record_artifact(&self, artifact: Artifact);
   }
   ```

3. Two implementors ship in Phase D:
   - `HarnessRecorder` — wraps OTel span emission + writes to the `xvn.db` recorder tables, same shape as today's harness path.
   - `EvalRecorder` — writes to the eval trace blob store AND mirrors into `xvn.db` recorder tables. The mirror is the F-11(f) fix: the eval review panel reads from the recorder tables, so eval-driven runs become indistinguishable from harness-driven runs on the review surface.

4. The `dispatch_capability` signature takes `&dyn Recorder`. Each capability handler emits through it. There is no code path that emits to one surface and not the other — that's the structural invariant Phase D enforces.

5. A regression test in Phase D pins the invariant: a synthetic eval run that invokes one Trader, one Filter, and one Critic must produce non-empty rows in **every** recorder table that the harness path produces non-empty rows in. The test fails CI if either side regresses to the asymmetric state.

This is the only way F-11(f) closes without yet another piecemeal "wire up tool_call emission on the eval path" PR — which the operator already noted in board-v2.md as "yet another role-shaped emission layer." Phase D removes the asymmetry at the source.

## Default-capability-set on starter templates (Phase E)

The QA carryover from PR #369: `validate_draft_succeeds_for_fresh_template` currently fails because the canonical strategy template ships no trader agent. The validator requires "at least one trader," and the fresh template's agents list is empty.

Phase E retrofits every entry in `crates/xvision-engine/src/agents/templates.rs::builtin_templates()` with explicit capability declarations:

| Template | Slots | Capability declarations |
|---|---|---|
| `single-trader` | 1 slot (`trader`) | `{Trader}` |
| `risk-checked-trader` | 3 slots (`trader`, `risk_check`, `executor`) | `{Trader}`, `{Critic}`, `{Trader}` |
| `momentum-trader-only` | 1 slot (`trader`) | `{Trader}` |
| `mean-reversion-trader` | 1 slot (`trader`) | `{Trader}` |
| `multi-asset-router-with-traders` | 5 slots (`router`, `equities_trader`, `crypto_trader`, `fx_trader`, `aggregator`) | `{Router}`, `{Trader}`, `{Trader}`, `{Trader}`, `{Critic}` |
| `regime-aware-trader` | 2 slots (`regime_filter`, `trader`) | `{Filter}`, `{Trader}` |
| `news-reader-plus-trader` | 2 slots (`news_reader`, `trader`) | `{Intern}`, `{Trader}` |
| `paper-confirmed-live-trader` | 2 slots (`paper_trader`, `live_executor`) | `{Trader}`, `{Critic}` |

The strategy templates that consume these agents also gain explicit `AgentRef.activates` matching the slot's primary capability. The freshly-created template instance is now valid out of the box: `validate_strategy` finds at least one AgentRef with `activates: Trader`, no missing-capability violations, no edge-predicate-references-unknown-signal violations.

Phase E flips `validate_draft_succeeds_for_fresh_template` from expected-fail to expected-pass. The expected-fail annotation on the test is removed in the same PR.

Operators authoring their own templates from scratch declare capabilities explicitly — there is no implicit-capability inference path. The default-of-`{Trader}` on `serde(default)` exists only to keep pre-spec serialized agents parseable; it is not the recommended authoring path. The agent editor in the future UI spec will surface capability checkboxes prominently.

## Implementation phases

The conductor decomposes this spec into the following contracts. Each phase is implementable in ~1 PR; phases are intended to land in order (B depends on A; D depends on B; E depends on B).

### Phase A — Capability schema + storage

**Branch / contract:** `agent-graph-capability-schema`

**Scope:**
- Add `Capability` enum.
- Add `AgentSlot.capabilities: BTreeSet<Capability>` with `serde(default = …)`.
- Add SQLite migration (number reserved by conductor at implementation time, do not reserve a number here): `ALTER TABLE agent_slots ADD COLUMN capabilities TEXT NOT NULL DEFAULT '["trader"]'`.
- Add `AgentRef.activates: Capability` + `AgentRef.granularity: FilterGranularity` (no validation enforced yet — that's Phase B).
- Add the `AgentOutput`, `FilterSignal`, `Critique`, `InternObservation`, `RouteSelection` types.
- Round-trip tests for the JSON shape (legacy strategies parse; new strategies parse; the `capabilities` column round-trips through the store).

**Out of scope:**
- Any dispatcher behavior change. The dispatcher still branches on `canonical_role`. Phase A is type + storage + parsing only.

**Verification:**
- `cargo test -p xvision-engine` passes.
- A test fixture loading a pre-spec strategy JSON deserializes without errors and the resulting `AgentRef` has `activates = Trader`, `granularity = Bar`.
- A new test fixture with `activates: filter` + `granularity: minute` round-trips.

### Phase B — Capability dispatch + per-kind I/O contracts

**Branch / contract:** `agent-graph-capability-dispatch`

**Scope:**
- Introduce `dispatch_capability(&Capability, &Briefing, &mut Context, &dyn Recorder) -> AgentOutput`.
- Rewrite `crates/xvision-engine/src/agent/pipeline.rs::run_agent_pipeline` to route by `agent_ref.activates`, not by `canonical_role`.
- Lift the parallel eval-executor dispatcher onto the same seam (`crates/xvision-engine/src/eval/executor/**`).
- Update `validate_strategy` to enforce: at least one AgentRef has `activates: Trader`; every AgentRef.activates is in the referenced AgentSlot.capabilities; no cycles in edges.
- Extended `PipelineEdge` shape with `condition: Option<EdgePredicate>`.
- Graph traversal logic per "Graph runtime semantics" above.

**Out of scope:**
- Edge editor UI.
- `Filter` capability LLM dispatch (the runtime substrate from `xvision-filters` is the v1 Filter implementation; LLM Filters land in Phase C).

**Verification:**
- `cargo test --workspace` passes.
- The `validate_draft_succeeds_for_fresh_template` test continues to be expected-fail (Phase E flips it).
- An eval run on a legacy strategy produces byte-identical trace output to pre-Phase-B baseline. Pin via a fixture diff.
- A new test runs a strategy with `kind: Graph`, two edges, and asserts the dispatcher honors edge gating.
- The `Recorder` trait is defined and both `HarnessRecorder` (no-op in Phase B beyond what the harness does today) and `EvalRecorder` (writes trace blob + mirrors into the recorder tables — the F-11(f) seam) are wired through.

### Phase C — Filter capability + granularity runtime

**Branch / contract:** `agent-graph-filter-capability`

**Scope:**
- LLM-driven `Capability::Filter` dispatcher. The Filter agent's `system_prompt` is wrapped with an output-schema constraint that returns `FilterSignal`-shaped JSON.
- Runtime honoring of `FilterGranularity` (Bar / Minute / Decision).
- Caching layer for Minute + Decision granularity signals (re-fire cached signal into downstream briefings until the next eval point).
- Integration with `xvision-filters` DSL filter so the same edge-predicate substrate works whether the producing Filter is LLM or DSL.

**Out of scope:**
- Multi-Filter cardinality decisions (see Open question 3 — operator must decide whether two Filters firing on the same bar means the Trader runs once or twice). Phase C ships the single-Filter-per-bar semantics; multi-Filter semantics is the question Open Question 3 closes.

**Verification:**
- A test strategy with one LLM Filter + one Trader, where the Filter's signal payload conditions an edge, asserts the Trader is invoked / skipped per the predicate.
- A test strategy with a `Minute`-granularity Filter on a 5m timeframe scenario asserts the runtime emits a `granularity_fallback` event and degrades to Bar.

### Phase D — Unified capability-gated recorder

**Branch / contract:** `agent-graph-unified-recorder`

**Scope:**
- Complete the `Recorder` trait surface (Phase B stubbed it; Phase D ships all seven methods on both implementors).
- Wire `HarnessRecorder` to the existing OTel + recorder-table emission code in the harness.
- Wire `EvalRecorder` to mirror into the recorder tables in addition to the trace blob.
- Regression test: synthetic eval run invokes Trader + Filter + Critic; assert non-empty rows in `tool_calls`, `events`, `supervisor_notes`, `approvals`, `sandbox_results`, `checkpoints`, `artifacts` — every table the harness path produces non-empty rows in.

**Verification:**
- F-11(f) reproduces today (eval-driven run produces empty rows in those seven tables on `main`).
- After Phase D, the same fixture produces non-empty rows in all seven.
- `cargo test --workspace` passes.

### Phase E — Starter templates + validator unblock

**Branch / contract:** `agent-graph-template-capabilities`

**Scope:**
- Add explicit `capabilities` declarations to every slot in `crates/xvision-engine/src/agents/templates.rs::builtin_templates()`.
- Add explicit `activates` declarations to every `AgentRef` produced by the matching strategy templates.
- Flip `validate_draft_succeeds_for_fresh_template` from expected-fail to expected-pass.
- Update the wizard / templates UI strings to mention capability where helpful (no UI rebuild — just label tweaks).

**Verification:**
- `validate_draft_succeeds_for_fresh_template` passes on `main` after merge.
- `cargo test -p xvision-engine` passes.
- Every builtin template's first-run validate-and-launch path completes without error in a smoke test.

### Phase F (optional, deferred) — Edge + capability editor UI

**Branch / contract:** `agent-graph-capability-editor-ui`

**Scope:** Dashboard UI for the capability editor (on the agent edit window) and the edge editor (on the strategy pipeline graph view). Deferred to a separate spec (`2026-05-XX-capability-editor-ui.md`) because the scope is large and the engine-side work in Phases A–E is independently shippable. v1 is JSON-only edge authoring; Phase F is the operator-friendly editor that lands when the engine substrate is proven.

## Out of scope (explicitly)

The spec deliberately excludes the following so the implementation doesn't bloat and so this spec stays achievable:

- **Cross-agent async coordination / message bus.** Agents do not communicate across cycles via a queue. All communication is per-cycle through the dispatcher's signal + observation maps. A future async-agent spec is a separate document.
- **Per-cycle re-routing on TraderDecision output.** "If the Trader says hold, route to the Critic and re-decide" is a tempting v2 feature. Defer until a use case exists — today's Critic position is "ran-and-recorded," not "ran-and-modified-trader-output." The Critic's verdict is an observation, not a re-routing signal in v1.
- **Inter-strategy composition (strategy-of-strategies).** Defer to V2C marketplace. The mint-time bundle schema may need to encode capability sets, but that's a V2C concern, not this spec's.
- **Cycles / loops in the agent graph.** v1 is DAG only. An iterative refinement loop (e.g. "trader proposes, critic rejects, trader refines, until critic approves or 3 iterations elapse") is a separate spec.
- **Per-Filter token budgets distinct from the AgentSlot `max_tokens`.** Filters reuse the slot's existing budget knobs. No new field.
- **An open-string capability namespace.** `Capability` is closed. Adding a sixth capability is a deliberate engine PR. Plugin-contributed capabilities (e.g. an MCP-supplied "TradeJournalReader" capability) are out of scope; that's a plugin-architecture conversation downstream of F28.
- **A migration that rewrites every workspace strategy.** No bulk rewrite. Pre-spec strategies survive on the legacy dispatch path; new strategies use the capability dispatch path. The dispatcher detects which by `agents.len() > 0`.
- **CLI breaking changes.** No CLI verb is renamed. New subcommands and flags are additive.
- **Frontend route changes.** No new routes ship in this spec. The agent-editor and pipeline-editor enhancements live in the deferred Phase F UI spec.
- **Algorithm-trait pipeline-stage code.** The `xvision-eval` `Algorithm` trait is the non-LLM eval baseline substrate. It is independent of `Capability`. Phase B does not unify them; an eval baseline running an `Algorithm` does not consume `Capability` dispatch.

## Open questions

The operator must resolve these before Phase A opens. Each is a decision the spec deliberately did not lock because the answer changes downstream phase shape.

1. **Does `role` become enforced (must match a registered capability name) or remain display-only?** The spec recommends display-only (decision 3) — but this is reversible. If the operator wants `role` to enforce capability (e.g. a slot named `"trader"` must declare `{Trader}`; a slot named `"filter"` must declare `{Filter}`), the validator gains a check and the migration backfills `capabilities` from the role name where possible. **Spec recommends:** display-only. **Operator decision pending.**

2. **Should `Capability::Router` ship in v1 or defer to v2?** Router is the most ambitious capability — it's an LLM that makes a routing decision, and the v1 use case is mostly the `multi-asset-router-with-traders` template. Deferring Router from v1 leaves that template unbuildable on the new dispatch path and forces it onto the legacy dispatcher. **Spec recommends:** ship in v1 (the template substrate is meaningful). **Operator decision pending — could also defer Router to a Phase G and ship Phases A–E with Trader / Filter / Critic / Intern only.**

3. **Multi-Filter cardinality per cycle: if two Filter-capability agents fire on the same bar, does the downstream Trader run once or twice?** (Raised in `HANDOFF-2026-05-21-next-thread.md` item #4.) Today the dispatcher runs every AgentRef in `Strategy.agents` order; if two AgentRefs both have `activates: Trader`, the last one's TraderDecision wins (per "Graph runtime semantics" step 5). For Filters, the question is different: if two Filters produce a signal, does the dispatcher build one downstream Trader briefing that sees both signals, or two separate briefings? **Spec recommends:** one briefing, both signals available. The Trader sees `filter_signals: { regime_filter: …, vol_filter: … }` and decides what to do with them. The alternative (one cycle, two Trader invocations, one per Filter) is power-of-loops semantics and belongs in v2. **Operator decision pending.**

4. **Should `Capability::Critic` be allowed to abort the cycle (veto the Trader's decision before risk sees it)?** Today a Critic produces an observation; the Trader's decision goes to risk regardless. A "veto-capable Critic" is a meaningful future feature (closer to a second risk layer), but it overlaps with the risk gate's responsibilities. **Spec recommends:** Critic is observation-only in v1. Cycle-aborting Critics are a v2 capability. **Operator decision pending.**

5. **Does the cached signal layer for `FilterGranularity::Minute` and `::Decision` persist across cycles or only within a single eval run?** Persisting across runs lets a Decision-granularity Filter avoid re-evaluating in a live trading scenario between cycles; not persisting forces re-eval on every restart. **Spec recommends:** in-memory within a run only (no SQLite caching of filter signals). Recompute on restart. The complexity of a persistent signal cache is not justified by v1 needs. **Operator decision pending.**

6. **Should `RouteSelection.to` allow forward references only (DAG-strict) or also back-references (introducing cycles)?** The spec locks DAG-only (decision 8), but RouteSelection is the natural place to introduce cycles ("route back to the trader after the critic rejects"). **Spec recommends:** DAG-strict for v1, RouteSelection validated to only reference downstream AgentRefs in `Strategy.agents` order. v2 lifts this. **Operator decision pending — could also relax to allow Router routing to any AgentRef regardless of order, accepting that v1 has no cycle-detection on Router output.**

7. **Where does the `capability` field live in the dashboard UI — on the agent edit window only, or also on the AgentRef in the strategy pipeline editor?** Display-only consequences: if `capabilities` is shown on the agent and `activates` is shown on the AgentRef, the operator sees both. If only `activates` is shown on the AgentRef, the operator has to navigate to the agent's edit window to see the full capability set. **Spec recommends:** both. Render capabilities as chips on the agent card; render `activates` as a badge on the AgentRef in the strategy diagram. **UI decision deferred to Phase F's UI spec; not blocking Phases A–E.**

8. **Are pre-spec strategies allowed to mix legacy `trader_slot` with new `agents`?** Today the deserialize path accepts both (per `mixed_strategy_json_keeps_both` test in `strategies/mod.rs`). The spec preserves this for back-compat. But the dispatcher must pick one — currently the engine prefers `agents` when both are populated. **Spec recommends:** preserve current behavior (prefer `agents` when both are present; the legacy fields are read but ignored by the dispatcher if `agents.len() > 0`). **Operator decision pending — could also harden into "reject strategies with both populated" once a migration window passes.**

## Verification

This spec is verified by:

- **Acceptance review** by the operator on the decisions list and the open-questions resolutions, before any phase contract opens.
- **Test coverage** in each phase contract pinning the invariants (round-trip parsing, dispatch routing, edge gating, recorder symmetry, template validation).
- **F-11(f) reproduction + close-out** — Phase D ships the regression test that fails on `main` (eval-driven runs produce empty recorder tables) and passes after Phase D lands.
- **`validate_draft_succeeds_for_fresh_template` flip** — Phase E flips this test from expected-fail to expected-pass. This is the single most visible closure of the QA carryover folded into this spec.

## Acceptance

The spec is accepted when:

1. The operator marks the **Decisions** list reviewed (accepted or with explicit changes).
2. The **Open questions** list is resolved or explicitly punted to specific phase contracts.
3. The conductor decomposes Phase A from this spec into a contract under `team/contracts/agent-graph-capability-schema.md` and opens implementation work.
4. The deferred `team/contracts/agent-graph-composition.md` placeholder is archived or updated to point at this spec as its replacement.

## See also

- `team/contracts/agent-graph-composition.md` — the deferred contract this spec replaces.
- `team/board-v2.md` "Capability-first agent model" research note — the second half of the convergence.
- `team/intake/2026-05-21-eval-honesty-and-agent-graph.md` — original intake row.
- `team/intake/2026-05-19-eval-traces-end-to-end-audit.md` F-11(f) — the empty-recorder finding closed by Phase D.
- `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md` — `Strategy { agents: Vec<AgentRef> }` refactor that this spec extends.
- `crates/xvision-engine/src/strategies/agent_ref.rs` — current `AgentRef` + `PipelineKind` + `PipelineEdge` shape.
- `crates/xvision-engine/src/agents/model.rs` — current `AgentSlot` shape; capabilities field lands here in Phase A.
- `crates/xvision-engine/src/agent/pipeline.rs` — current role-string-gated dispatcher; rewritten in Phase B.
- `crates/xvision-filters/src/runtime.rs` — `RuntimeFilter` + `ActivationDecision` substrate that Phase C bridges into `FilterSignal`.
- `crates/xvision-engine/src/agents/templates.rs` — starter templates; capability declarations land in Phase E.
