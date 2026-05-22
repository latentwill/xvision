# Agent firing-filter operator surface

**Date:** 2026-05-22
**Surface:** `frontend/web/src/components/{agent,strategy}/**`, `crates/xvision-cli/src/commands/{agent,strategy}.rs`, `crates/xvision-engine/src/strategies/validate.rs` (warning text only).
**Status:** Draft for operator review (spec only — no implementation in this PR).
**Related:**
- `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md` — the load-bearing spec this one rides on top of. Introduces `Capability::Filter`, `FilterGranularity`, and `PipelineEdge.condition: Option<EdgePredicate>`.
- `team/contracts/agent-graph-capability-schema.md` (Phase A, PR #527) — `AgentSlot.capabilities`, `AgentRef.activates`, `PipelineEdge.condition` shapes.
- `team/contracts/agent-graph-capability-dispatch.md` (Phase B) — `dispatch_capability` seam, `AgentOutput`, `EdgePredicate` evaluator.
- `team/contracts/agent-graph-filter-capability.md` (Phase C) — LLM Filter dispatcher + `FilterGranularity` runtime.
- `team/contracts/agent-graph-template-capabilities.md` (Phase E) — starter templates declaring explicit capabilities.
- `crates/xvision-filters/src/lib.rs` — `Filter` DSL substrate the LLM Filter capability bridges into.
- `frontend/web/src/components/agent/AgentForm.tsx`, `SlotForm.tsx` — current agent-create form (no Filter section today).
- `crates/xvision-cli/src/commands/agent.rs` — current `xvn agent` (read-only `get` only).

## Goal

When the capability cascade (`agent-graph-2026-05-22`) lands, the **runtime** can gate any agent in a strategy on a Filter-capability agent's signal via an `EdgePredicate`. The engineering substrate is sufficient. But:

1. The current agent-create form (`AgentForm.tsx`) has no concept of "firing conditions" — an operator builds a Trader-capability agent and the form never asks "when should this fire?".
2. There is no CLI surface that prompts an operator to attach a firing condition when none has been set.
3. There is no guidance text explaining what a Filter is, when to add one, or what happens by default (`EveryBar`).
4. `xvn strategy validate` will accept a strategy with a single Trader and no upstream Filter — silently `EveryBar`. That's correct engine behavior, but a new operator never learns the lever exists.

This spec defines the **operator-facing surface** that turns the cascade's runtime capability into a feature an operator can discover, configure, and be guided into using — without adding any new engine schema beyond what Phase A already ships.

## Decisions

These are the load-bearing calls. Operator should accept or reject at review.

1. **Firing-filter authoring happens at the strategy level, not the agent level (v1).** The `AgentForm` (agent template editor) does not author firing filters. It only declares the agent's *capabilities* (via Phase A's `capabilities: BTreeSet<Capability>`). Firing conditions are a property of an agent's *appearance in a strategy*, not the template itself. The `StrategyForm` is where the operator wires a Filter agent upstream of a Trader and authors the edge predicate.

2. **The `AgentForm` gains a "Firing conditions" awareness card — not an input.** When the operator builds a Trader-capable agent, the form shows a small explainer card *next to the agent* (under the Behavior section, before the save bar) reading roughly:

   > **Firing conditions** — This agent runs on every bar by default. To gate it on a market regime, indicator threshold, or other signal, add a Filter-capable agent upstream of this one inside a strategy. ([Learn more](#))

   Same card on Critic-capable, Intern-capable, and Router-capable agents. *Not* shown on Filter-capable agents (they're the gate, they don't have one).

   Rationale: the user's "filter section next to the agent" requirement is satisfied as guidance, not as an authoring input. The actual configuration lives one layer up where it belongs (per-strategy), and the agent template stays reusable across strategies with different firing conditions.

3. **The `StrategyForm` gains a "Firing conditions" section per non-Filter AgentRef.** Each `AgentRef` card in the strategy editor renders a "When does this fire?" sub-section that lists:
   - **Default** (no upstream Filter) — "Every bar." with a one-click "Add filter →" affordance.
   - **Active** (one or more upstream Filter agents wired in) — a read-only summary like *"Fires when `regime_filter.regime == 'high_vol'`"* derived from `PipelineEdge.condition` on the incoming edge(s).
   - **Inline editor** — when "Add filter →" is clicked, opens an inline composer that (a) lets the operator pick from existing Filter-capable agents in the workspace OR create a new one inline, then (b) authors the `EdgePredicate` against the selected Filter agent's expected `FilterSignal` schema.

   The inline composer routes; it is not a popup (per the no-popup rule in `/CLAUDE.md`).

4. **`xvn strategy validate` emits a soft-warning when a Trader/Critic AgentRef has no upstream Filter.** Soft, not error — a single-trader-every-bar strategy is a legitimate config. The warning reads:

   > `warning: strategy '<name>' has a Trader agent with no upstream Filter — it will dispatch on every bar. Consider adding a Filter to reduce LLM cost. (See: xvn agent create --capability filter)`

   The CLI exits 0 with a warning, not 2. `xvn strategy create` and `xvn strategy edit` pass through the same warning. Operators who want every-bar behavior can suppress the warning per-strategy via `--no-filter-warning` (persisted in the strategy JSON as `acknowledge_no_filter: true`).

5. **`xvn agent` gains a create verb in v1 of this spec.** The current `xvn agent` is read-only. This spec adds `xvn agent create --name <n> --capability <trader|filter|critic|intern|router> --provider <p> --model <m> --system-prompt <path-or-string> [--skills <ids>...]`. No firing-filter argument — see Decision 1.

6. **`xvn strategy edit` gains a filter-wiring verb.** New subcommands:
   - `xvn strategy add-filter <strategy_id> --filter-agent <agent_id> --gates <role> --when <predicate-json>` — wires a Filter agent into the strategy and attaches an `EdgePredicate` on the new edge to the named downstream agent role.
   - `xvn strategy remove-filter <strategy_id> --role <filter_role>` — removes the Filter agent and any edges originating from it.

   The `--when` predicate value is a JSON-serialized `EdgePredicate` matching the typed form from Phase A. No DSL parser at the CLI; the SPA editor produces the JSON.

7. **The `AgentForm` bottom save bar inherits the same spacing as other long forms.** The user's screenshot flagged a margin issue at the bottom of the form. The save bar currently uses `sticky bottom-4` (1rem). Audit: the bottom-most card under the save bar should have `pb-4` so the sticky bar floats over visible content, not flush against the last card. Fix included in Phase 1 of this spec.

8. **No new engine schema.** This spec is operator-surface only. All gating runs through Phase A's `PipelineEdge.condition` and Phase C's Filter-capability dispatcher. If the spec proposes a schema field that doesn't already exist in the cascade, it's wrong.

## Scope

This spec covers:

- The AgentForm "Firing conditions" awareness card (text-only, no input).
- The StrategyForm "When does this fire?" section per AgentRef.
- The inline Filter composer for picking/creating a Filter agent and authoring an `EdgePredicate`.
- `xvn agent create` (new CLI verb).
- `xvn strategy add-filter` / `remove-filter` (new CLI verbs).
- `xvn strategy validate` soft-warning for Trader-without-Filter.
- The AgentForm bottom-margin / save-bar fix.

This spec does NOT cover:

- The Filter-capability runtime — owned by Phase C (`agent-graph-filter-capability`).
- `EdgePredicate` evaluation — owned by Phase B (`agent-graph-capability-dispatch`).
- `FilterSignal` schema — owned by Phase A (`agent-graph-capability-schema`).
- A graph editor / canvas UI for arbitrary DAG authoring — Phase A's spec defers graph-editor UI to a follow-up (`2026-05-XX-capability-editor-ui.md`). This spec ships the linear "Add filter →" affordance only.
- Per-Agent default firing filter at the template level — **deferred follow-up** (see "Follow-ups" below).

## Phases

The spec decomposes into three sequential phases. All three depend on the agent-graph cascade Phases A, B, C, E having landed.

### Phase 1 — AgentForm awareness + margin fix
- Surface: `frontend/web/src/components/agent/AgentForm.tsx`, `SlotForm.tsx`.
- Add the "Firing conditions" explainer card under the Behavior section.
- Fix the bottom-margin so the sticky save bar doesn't visually merge with the last card.
- Acceptance: visual check on `/agents/new` matches the spec; e2e snapshot updated.

### Phase 2 — `xvn agent create` + `xvn strategy add-filter` / `remove-filter`
- Surface: `crates/xvision-cli/src/commands/agent.rs`, `strategy.rs`.
- Acceptance: integration tests cover the happy path; `xvn strategy validate` emits the soft-warning on a Trader-without-Filter strategy.

### Phase 3 — StrategyForm "When does this fire?" + inline Filter composer
- Surface: `frontend/web/src/components/strategy/**` (StrategyForm + new InlineFilterComposer).
- Wires through the existing strategy edit API; reads Filter-capable agents from the workspace.
- Acceptance: an operator can create a strategy, see "Every bar" under each non-Filter ref, click "Add filter →", pick a Filter agent, author a predicate, and save.

## Follow-ups

These are deliberately out of scope for this spec; tracked here so they're not lost.

- **Per-Agent default firing filter at the template level.** When the multi-agent strategies refactor (`2026-05-21-v2f-strategies-folder-and-template-refactor.md` or its successor) makes "agents-are-reusable-across-strategies" more central, revisit whether an agent template should carry a *default* `Filter` that strategies inherit but can override per-AgentRef. Out of scope here because v1 strategies have a small enough cardinality that authoring the filter per-strategy is cheaper than the inheritance complexity.

- **DSL ↔ LLM Filter unification in the operator UI.** Phase C's LLM Filter capability and `xvision-filters`' deterministic DSL filter both produce `FilterSignal` shapes. The operator UI should at some point offer "deterministic indicator filter" vs "LLM-judged filter" as two ways to populate the same edge predicate target. v1 of this spec defers — the LLM Filter agent is the only authorable source.

- **Graph canvas UI** for non-linear strategies. Already noted in the capability-first spec as a deferred follow-up.

## Open questions

All open questions resolved 2026-05-22; locked decisions below.

## Locked decisions (2026-05-22 operator review)

These resolve the four open questions above. They have the same force as the numbered decisions in the main Decisions block.

- **L1. Docs page.** The "Learn more" link in the AgentForm awareness card (Decision 2) and the `Consider adding a Filter` warning in `xvn strategy validate` (Decision 4) both point at `docs/operator/firing-conditions.md`. Owned by Phase 1 of this spec — the page ships in the same PR as the awareness card. Page covers: what a Filter agent is, why you'd add one (LLM cost reduction, regime-gating, indicator-threshold-gating), how to wire one from the SPA, how to wire one from the CLI, the default `EveryBar` behavior, and the `acknowledge_no_filter` opt-out.

- **L2. Soft-warning surface.** The Trader-without-Filter warning fires in **both** the CLI and the SPA. The engine emission point is the existing `warnings: Vec<String>` return on `validate_strategy()`. `xvn strategy validate` prints each warning to stderr, exits 0. The SPA's existing validation feedback panel (`AgentForm.tsx:321-328`'s `DiagnosticList` and the equivalent in `StrategyForm`) already renders the `warnings` array — no new SPA plumbing, just confirm the strategy editor surface threads it through. Save button stays enabled when only warnings (not errors) are present.

- **L3. `--when` shape.** JSON literal only for v1. `xvn strategy add-filter --when '{"signal":"regime_filter","field":"regime","op":"eq","value":"high_vol"}'`. No `--when-file` flag, no `@path` convention, no DSL. The CLI is the headless surface; operators authoring non-trivial predicates use the SPA composer. If operator demand for shell-authored multi-line predicates surfaces post-launch, add `--when-file` as a follow-up — it's an additive flag.

- **L4. Inline Filter agent authoring.** The Phase 3 inline Filter composer lets the operator author a Filter-capable agent inline (provider, model, system_prompt, skills) without leaving the strategy editor. A "Save as reusable agent" toggle defaults **on** — the operator's expectation is that work is reusable unless they opt out. Toggle off → the Filter agent persists scoped to this one strategy (a "private" agent, not surfaced in the workspace agent list). Toggle on → the agent saves to the workspace alongside everything else and the strategy references it by `agent_id` like any other ref.

  Private (toggle-off) Filter agents are stored as ordinary `Agent` rows with a `scope: Option<StrategyId>` field that hides them from the workspace agent list when set. No new table. If the operator later flips the toggle on for a private Filter from the strategy editor, the scope clears and the agent appears in the workspace.

  Note: `scope: Option<StrategyId>` is the one schema field this spec adds beyond the cascade. It's additive (nullable, default `None`) and lives on `Agent`, not `AgentRef` — see Decision 8 amendment below.

## Amendment to Decision 8 (no new engine schema)

L4 introduces one nullable column on the `agents` table: `scope_strategy_id TEXT NULL`. This is the minimum schema cost of inline Filter authoring with a working "Save as reusable agent" toggle. Migration number reserved via the team manifest before Phase 3 opens. All other gating still rides on the cascade's `PipelineEdge.condition` — no change to that decision.
