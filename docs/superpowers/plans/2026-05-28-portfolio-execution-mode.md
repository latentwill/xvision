# Portfolio Execution Mode — Implementation Plan

**Date:** 2026-05-28
**Status:** Draft. Design review captured; implementation sequence to be
finalized after Q1–Q5 below are answered.
**Related:**
- Behavior spec (locked contract): `docs/superpowers/specs/2026-05-25-execution-capital-modes.md`
- Parent follow-up plan: `docs/superpowers/plans/2026-05-25-multi-asset-followups.md` (Phase 3)
- Pre-existing rejection test:
  `crates/xvision-engine/tests/multi_asset_backtest.rs::portfolio_mode_returns_not_implemented`

## Motivation

The trader is called once per asset per bar. This was never the design
intent. Per-asset shape causes two stated problems: token cost (N LLM calls
per bar where 1 would suffice), and portfolio imbalance (the trader cannot
reason about allocation when it sees assets serially — "rotate AAPL→MSFT"
is unrepresentable in two independent calls).

The briefing/output shape for Portfolio mode is already locked in the
execution-capital-modes spec. This plan covers the substrate refactor
required to satisfy that contract — and, importantly, what to keep out of
engine code so strategy behavior stays in agents and prompts.

## Design review (2026-05-28)

An initial 7-PR proposal was adversarially reviewed. About 85% of the
review concerns engine substrate (strategy-neutral type and data hygiene);
about 15% pushed back against code-implied strategy that should belong to
agent prompts. The split below is the controlling lens for the
implementation. Reintroducing anything from the "engine must not decide"
column requires a written rationale per the CLAUDE.md convention.

### Engine substrate (strategy-neutral — these are non-negotiable)

**1. `SnapshotMode` enum, not `Option<Vec<AssetView>>`.**
`MarketSnapshot` mode should be `enum SnapshotMode { PerAsset(AssetView),
Portfolio(Vec<AssetView>) }`. The compiler then forces every consumer to
handle both arms. An additive `Option<Vec>` silently creates an "is this
set?" check at every call site — exactly the bug pattern Rust enums exist
to prevent.

**2. Normalize the decision data model.**
One portfolio decision = one row. Introduce a `portfolio_decisions` table
(one row per cycle for Portfolio runs) carrying `trader_summary` and any
portfolio-level metadata. `eval_decisions` (per-asset actions) carries an
FK to its parent portfolio decision. Do NOT denormalize by stuffing
`trader_summary` "on the first row only" of a group — that encodes a query
convention as tribal knowledge.

**3. Backfill `decision_group_id` and make it NOT NULL.**
Legacy per-asset rows get a generated ULID at migration time (singleton
group of size 1). Consumers then never need an "if null, treat as
singleton" branch. We are pre-launch (cf. the setup→cycle rename precedent)
— use the freedom. Nullable-with-meaning columns are a permanent tax on
every query.

**4. Bump `TRAJECTORY_SCHEMA_VERSION`.**
The briefing shape changes. That is what the schema version exists for.

**5. Keep the per-asset `note` field on `AssetAction`.**
The field exists; whether a given trader agent fills it is a function of
its prompt and response schema. Audit signal is most valuable when a mode
is new. Do not remove it as a "simplification" in v1.

**6. Compile-time honesty about mode coupling.**
If a strategy declares `ExecutionMode::Portfolio`, the bound algorithm
must support portfolio decisions. A `decide_portfolio` default impl that
wraps `decide` silently gives Portfolio runs per-asset behavior — the
engine lies about what it's doing. Resolve at compile time (no default
impl) or at manifest validation time (reject the strategy), but do not
ship the silent degradation. Pick one explicitly before PR 1.

### Strategy-by-prompt / agent strategy (engine must NOT decide)

xvision's design philosophy puts strategy in agent composition and prompts,
not in engine branches. The engine provides substrate; it does not author
policy.

**7. Portfolio risk verdict — engine routes, risk agent decides.**
The initial review recommended a "veto-on-any-veto" rule in v1. That
recommendation is withdrawn: it is code-implied strategy and belongs in the
risk agent's prompt, not in engine code. Correct shape:

- Extend `RiskDecision` (or add a portfolio-shaped sibling) so the verdict
  type can carry a portfolio-level decision.
- Route the trader's portfolio output to the risk agent whole.
- The risk agent issues the verdict (veto-on-any-veto, partial-pass,
  modify-allocation, …) via its prompt. Engine carries the verdict and
  acts on it.

The gray-area piece — manifest validation that a Portfolio strategy's risk
agent declares it accepts portfolio input — IS substrate. Same kind of
check as "all agent slots reference existing agents." Allowed.

### Process notes

**Dual-mode is debt, not safety.** Keeping `PerAsset` as a peer-default to
`Portfolio` indefinitely commits the codebase to two trait methods, two
snapshot shapes, two response schemas, two test matrices forever. The plan
should declare a deprecation intent for `PerAsset` (even if the deprecation
itself is a later wave), with any retained per-asset strategies named with
written rationale. "Additive because migrations are scary" is not a
rationale.

**"7 independently revertible PRs" is overclaimed.** Nothing is
user-visible until the manifest-opt-in PR. The plumbing PRs depend on each
other's types. Honest framing: pre-feature plumbing → flag flip → cleanup.
Real revertibility question: can the flag-flip PR be reverted in isolation
after it ships? If the manifest default stays `PerAsset`, yes — make that
an explicit acceptance criterion on the flag-flip PR.

## Open design calls (post-review)

The original 5-question set was framed inside choices that mostly had
better third options (use an enum / use a normalized table / route to the
risk agent). Revised question set, scoped to genuine tradeoffs:

**Q1. Risk-agent manifest contract for portfolio input.**
What does "this risk agent accepts portfolio decisions" look like in the
manifest — a new field, an agent capability flag, a trait obligation? Pick
the shape before the migration so strategy validation can enforce
composition correctness from day one.

**Q2. `RiskDecision` extension vs sibling type.**
Does `RiskDecision` grow a `Portfolio { … }` variant, or does
`PortfolioRiskDecision` live next to it? Both workable; choice affects how
the executor branches on verdict shape and how persistence stores it.

**Q3. `PerAsset` deprecation timeline.**
Is `PerAsset` retained because a strategy class genuinely needs per-asset
reasoning, or because the rewrite feels scary? If the former, name the
strategy class. If the latter, set a deprecation milestone (e.g.
"Portfolio default after one quarter of operator usage; PerAsset opt-in
with written per-strategy rationale thereafter").

**Q4. Strategy-author error message for invalid compositions.**
When manifest validation rejects (e.g.) a Portfolio strategy paired with a
per-asset-only risk agent, what does the operator see? Follow the
"experimental-stored / runtime-rejected" precedent from the exec-modes
spec for consistency.

**Q5. Eval / ab-compare semantics for portfolio decisions.**
With one portfolio decision per cycle replacing N per-asset rows,
`cycles_evaluated` and per-arm decision counts in `xvn ab-compare` change
shape. Define the new semantics now (likely: portfolio decision = 1 cycle;
per-asset action counts reported separately as a child metric).

## Implementation sequence

To be drafted after Q1–Q5 are answered. The 7-PR sketch in the original
proposal is a reasonable starting frame conditional on:

- PR 1 (types + migration) uses `SnapshotMode` enum and the normalized
  `portfolio_decisions` table. Not `Option<Vec>`. Not denormalized group
  rows.
- Migration backfills `decision_group_id` and lands the column NOT NULL.
- The risk-verdict extension lands in the same wave as the trader
  portfolio call. Splitting them leaves v1 with a half-baked verdict
  story.
- The §6 "compile-time honesty" choice is made before PR 1.
- The flag-flip PR's acceptance criteria include "reverting this PR alone
  restores PerAsset behavior with no schema rollback required."

A revision of this plan will reorder PRs, add per-PR verification
criteria, and replace this section with the executable task list once the
open questions have answers.
