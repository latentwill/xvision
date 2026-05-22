# Intake — 2026-05-22 — agent firing-filter operator surface

Source: 2026-05-22 operator review of the agent-create form. The screenshot
showed no firing-conditions input on the agent form and prompted a "where
does this go?" investigation. Audit found the agent-graph capability cascade
(`agent-graph-2026-05-22`) already provides the runtime substrate
(`Capability::Filter`, `FilterGranularity`, `PipelineEdge.condition`), but
nothing on the operator-facing side prompts users to discover or use it.

Spec authored 2026-05-22 at
`docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`.
This intake registers the spec for conductor decomposition.

## What the spec ships

Operator-surface only — no engine schema beyond one nullable column on
`agents` for private (per-strategy) Filter agents. Three phases:

1. **Phase 1 — AgentForm awareness + bottom-margin fix.** Frontend only.
   Adds a "Firing conditions" explainer card next to each non-Filter slot
   in the agent editor (under Behavior section), plus a CSS fix for the
   bottom margin where the sticky save bar visually merges with the last
   card. Ships the operator docs page (`docs/operator/firing-conditions.md`)
   in the same PR.
2. **Phase 2 — CLI verbs.** `xvn agent create` (new), `xvn strategy
   add-filter` / `remove-filter` (new), `xvn strategy validate`
   soft-warning when a Trader/Critic has no upstream Filter.
3. **Phase 3 — StrategyForm "When does this fire?" + inline Filter
   composer.** Each non-Filter `AgentRef` card in the strategy editor
   renders a firing-conditions sub-section. "Add filter →" opens an
   inline composer that lets the operator pick from existing Filter
   agents OR author a Filter agent inline. Inline Filter agents save
   reusable by default (toggle off → scoped to this strategy via the new
   `agents.scope_strategy_id` column).

## Dependencies (hard)

All three phases depend on the agent-graph cascade landing:

- **Phase A** (`agent-graph-capability-schema`, PR #527) — provides
  `Capability` enum, `AgentSlot.capabilities`, `AgentRef.activates`,
  `PipelineEdge.condition`. **All three phases need this.**
- **Phase B** (`agent-graph-capability-dispatch`) — provides
  `dispatch_capability`, `EdgePredicate` evaluator. **Phases 2 and 3 need
  this** (CLI validate-warning and the SPA composer both reference the
  predicate shape).
- **Phase C** (`agent-graph-filter-capability`) — provides the LLM Filter
  dispatcher and `FilterGranularity` runtime. **Phase 3 needs this**
  (the inline composer authors agents that this phase makes
  dispatchable).
- **Phase E** (`agent-graph-template-capabilities`) — provides starter
  templates with explicit capabilities. **Phase 1's awareness card text
  references "Filter-capable" agents** — works regardless, but Phase E
  populating templates with `Capability::Filter` is what makes the
  empty-workspace experience non-degenerate.

The cascade is in flight under wave `agent-graph-2026-05-22` (board.md
Active section). This new wave queues behind it.

## Dependencies (soft)

- **Operator docs surface.** `docs/operator/` doesn't exist yet — Phase 1
  creates the directory along with `firing-conditions.md`. No
  cross-cutting docs index to wire up; a follow-up could add a top-level
  `docs/operator/README.md` if more operator pages accumulate.

## Schema cost

One additive migration in Phase 3 only:

- `agents.scope_strategy_id TEXT NULL` — when set, hides the agent from
  the workspace agent list (operator authored it inline in a strategy
  editor with "Save as reusable agent" toggled off). Migration number to
  be registered in `team/MANIFEST.md` before Phase 3 opens.

Phases 1 and 2 ship no migrations.

## Out of scope (deferred)

- **Per-Agent default firing filter at the template level.** Revisited
  when the multi-agent strategies refactor lands and reusable agents
  carry richer defaults. Spec already notes this as a follow-up.
- **Graph canvas UI for non-linear strategies.** Already deferred in the
  capability-first spec.
- **DSL ↔ LLM Filter unification in the operator UI.** Phase C's LLM
  Filter and `xvision-filters`' deterministic DSL filter both produce
  `FilterSignal`. v1 ships LLM Filter authoring only via the composer;
  DSL-filter authoring stays CLI-only (operators who want it can write
  the JSON literal).
- **`--when-file` CLI flag.** JSON literal only for v1; file-form deferred.

## Proposed wave

`agent-firing-filter-operator-surface-2026-05-XX` (conductor sets the
date). Three contracts, one per phase. All conditional on the cascade
wave completing.

Contract sketches (conductor expands):

- `agent-firing-filter-form-and-docs` — Phase 1. Allowed:
  `frontend/web/src/components/agent/AgentForm.tsx`,
  `frontend/web/src/components/agent/SlotForm.tsx`,
  `docs/operator/firing-conditions.md`. Forbidden: any Rust crate,
  migrations, strategy form. Verification: e2e snapshot of /agents/new
  with the new awareness card; manual visual check of the bottom margin
  fix. Acceptance: card renders next to Trader/Critic/Intern/Router
  slots; not rendered next to Filter slots; docs page lives at the
  expected path.

- `agent-firing-filter-cli-verbs` — Phase 2. Allowed:
  `crates/xvision-cli/src/commands/agent.rs`,
  `crates/xvision-cli/src/commands/strategy.rs`,
  `crates/xvision-engine/src/strategies/validate.rs` (warning emission
  only — schema unchanged), `crates/xvision-cli/tests/**` (new test
  files). Forbidden: SPA, migrations, engine schema. Verification:
  `cargo test -p xvision-cli`, `cargo test -p xvision-engine
  --test strategy_validate_warnings`. Acceptance: `xvn agent create`
  round-trips; `xvn strategy add-filter`/`remove-filter` round-trip;
  `xvn strategy validate` emits the soft-warning on a Trader-without-
  Filter strategy and still exits 0.

- `agent-firing-filter-strategy-composer` — Phase 3. Allowed:
  `frontend/web/src/components/strategy/**` (new
  `InlineFilterComposer.tsx`), one new migration in
  `crates/xvision-engine/migrations/` (`agents.scope_strategy_id`),
  agents API surface for the `scope_strategy_id` field. Forbidden: agent
  form. Verification: e2e for strategy create with inline Filter authoring;
  migration up/down test. Acceptance: operator can wire a Filter from
  the strategy editor end-to-end without leaving the page; toggle
  on/off correctly scopes/unscopes the agent.

## Priority

P2 — not blocking the cascade, lands after. The agent-graph cascade
itself is P1 and remains the conductor's current focus.

## Acceptance for intake

Conductor accepts when:
- The three phases are recognizable as three contracts queued behind
  the cascade.
- The migration registry has the next number reserved for Phase 3 (or a
  note that the reservation happens at Phase 3 open).
- The wave is added to `team/board.md` Deferred section behind the
  cascade.
