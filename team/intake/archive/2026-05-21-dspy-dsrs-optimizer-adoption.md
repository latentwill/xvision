# Intake — 2026-05-21 — DSPy / DSRs optimizer adoption

Source: operator strategy conversation 2026-05-21 exploring whether DSPy-style
prompt optimization should land as workspace infrastructure before V3
autooptimizer. After initial scoping the operator redirected: DSPy/DSRs
adoption should **not** be its own wave. Stage 1 folds into the filter
work — specifically into **filter v1.5**, which adds LLM-backed filters
on top of the deterministic-indicator v1 that ships first per
`docs/superpowers/specs/2026-05-21-filter-v1-shape.md` and
`docs/superpowers/plans/2026-05-21-filter-v1-implementation.md`. Stage 2
is a separately-tracked future agent refactor (intern / trader / risk)
that lands after v1.5 proves the loop and before autooptimizer's mutator
design is locked.

This intake captures both stages and the rationale; Stage 1 contracts are
authored by the v1.5 wave conductor as a follow-on to filter v1, not as
a standalone wave.

## Revision history

- 2026-05-21 — initial intake; pre-decomposition. Two-stage framing.
- 2026-05-21 — revised after the filter v1 spec/plan/contract landed.
  Stage 1 now explicitly slots into filter **v1.5** (not the filter
  intake's `agent-graph-composition` track directly) because filter v1
  ships deterministic-indicator-only — LLM-backed filters are v1.5,
  and that is where DSPy applies.

## Position (one sentence)

DSPy/DSRs is not its own wave — Stage 1 is the implementation substrate
for **filter v1.5** (LLM-backed filters on top of the deterministic-
indicator v1 shipping per `docs/superpowers/specs/2026-05-21-filter-v1-shape.md`),
and Stage 2 is a follow-on agent refactor (intern / trader / risk) that
lands after v1.5 proves the loop.

## Why this came up

The convergence is concrete. From `MANUAL.md` §scaling:

> AutoOptimizer cost: at N=100 with each agent generating 100 mutator
> variants/night × 50K-token briefings × Sonnet-class evaluation, the LLM
> bill is ~$15K/month.

That projection is for *random* mutation. A reflective evolutionary
optimizer (GEPA, in the DSPy family) typically lands the same quality
improvement with substantially fewer rollouts because each mutation is
informed by an LM reading the failed trace and proposing a targeted
change. The same substrate — outcomes per cycle, metric, mutate-and-
evaluate loop — already exists in `crates/xvision-engine/src/eval/`.
Adopting the optimizer as a primitive before autooptimizer locks its
mutator design means autooptimizer consumes it instead of rebuilding
it later.

The same primitive also applies to filters (regime detectors, news
classifiers, signal-emitters), where the metric is clean classification
accuracy, not noisy PnL — and that is the cheaper, lower-stakes proving
ground.

DSRs (krypticmouse/DSRs, published as `dspy-rs` on crates.io) is a native
Rust rewrite of DSPy, beta with stabilizing API, and already includes a
GEPA implementation (`crates/dspy-rs/examples/10-gepa-llm-judge.rs` in
that repo). No Python sidecar required.

## Current state (what already ships)

- **No optimization layer.** Agent prompts are hand-tuned. Starter
  templates at `crates/xvision-engine/src/agents/templates.rs`;
  user-authored agents carry a `system_prompt: String` per
  `crates/xvision-engine/src/agents/model.rs`. Nothing in the workspace
  mutates these against an objective.
- **Eval substrate exists.** `crates/xvision-engine/src/eval/` writes
  `decisions.jsonl` + `events.jsonl` per run with per-cycle outcomes;
  A/B compare at `crates/xvision-engine/src/eval/compare.rs` operates
  on these. This is the input shape an optimizer needs.
- **No Rust DSPy dependency.** Workspace `Cargo.toml` does not depend
  on `dspy-rs`.
- **AutoOptimizer unbuilt.** V3 per `FOLLOWUPS.md` row 15. Right time
  to make the foundational optimizer choice before the mutator loop is
  written.
- **V2D + V2E shipped 2026-05-21** per `team/board-v2.md`. The two
  named V3 prerequisites are done. Filters come next, and Stage 1 of
  this intake rides on that track.

## Stage 1 — DSPy/DSRs inside filter v1.5 (near-term, post-v1)

Filter v1 ships deterministic-indicator-only — the per-bar evaluator is
math, no LLM call, nothing to optimize (see
`docs/superpowers/specs/2026-05-21-filter-v1-shape.md` for the v1
shape). Filter v1.5 adds **LLM-backed filters** — a filter whose
condition tree is replaced (or augmented) by an LLM classification
("is this regime trending or chopping?"). That LLM call IS the
optimization target.

Why fold Stage 1 into v1.5 specifically:

- **v1 ships the substrate.** `FilterEventV1`, `FilterSummary`, the
  per-bar hook at `crates/xvision-engine/src/eval/executor/backtest.rs:496`,
  and the read-only frontend panels are exactly what an optimizer needs
  to surface diffs into. Building Stage 1 against half-built
  infrastructure would be a mistake.
- **v1 validates the operator UX.** The Filter entity, ActivationMode,
  and the no-popups inline panels prove out before any optimizer
  jargon enters the surface.
- **Clean metric.** An LLM filter's metric is classification accuracy
  on a labeled holdout. Ideal first signal for an optimizer; you avoid
  debugging noisy PnL while debugging DSRs.
- **Operator artifact.** Stage 1 produces a measurable accuracy delta on
  a holdout slice. That validates the entire loop — DSRs maturity,
  optimizer choice, UX of opt-in tuning — before anything risks the
  trader pipeline.

Sketched v1.5 sub-items (decomposed by the v1.5 wave conductor after
v1 lands):

| Sub-item | Notes |
|---|---|
| Introduce `SlotRuntime::Llm` and `kind: filter` per the earlier wider filter draft. | The structural change v1 deliberately deferred. Stage 1 inherits this from v1.5's foundation work. |
| Add `dspy-rs` workspace dep, pinned, behind a feature gate. | Cheap. Lets the rest of the workspace keep building if DSRs has a transient breaking change. |
| One-day spike on DSRs's `10-gepa-llm-judge.rs` example to confirm beta maturity before committing. | Risk reduction; lives inside v1.5 foundation work. |
| One LLM filter (regime detector or similar — TBD with conductor) compiles through DSRs against a labeled holdout using BootstrapFewShot or MIPROv2. | First real optimizer proof. |
| Optimizer diff surfaces in eval-review as a small "Filter tuned" panel — original prompt vs optimized, holdout accuracy delta, accept/revert. | Honors no-popups rule per CLAUDE.md; renders inline on the filter detail surface alongside the v1 `FilterSummaryPanel`. |
| Operator UX: opt-in "Tune this filter" action on the filter detail page. | Vibetrader principle: the user owns the original; the optimized variant is a named, reviewable artifact. |
| Eval-review event kinds for `filter_optimizer_run` and `filter_optimizer_accepted`. | Same `events.jsonl` channel V2D memory + v1 `FilterEventV1` use. |

**Conductor decision at v1.5 decomposition:** sub-items as part of the
larger v1.5 contract set, or as a dedicated `filter-v1-5-optimizer-dsrs`
companion contract? Recommend companion contract — the DSRs dependency
surface is its own change with its own beta-risk posture, and decoupling
it lets the rest of v1.5 (the SlotRuntime split + graph executor) land
even if DSRs hits a snag.

## Stage 2 — Agent refactor with DSPy (secondary, post-filter)

Once filters prove the loop in production, a broader agent refactor
brings DSPy primitives — typed signatures, modules, optimizer adapters
— to the intern / trader / risk pipeline. The autooptimizer's mutator
loop is built as a GEPA call from day one in V3 instead of as random
mutation that gets replaced later.

Sketched scope (deliberately undercommitted; revisit once Stage 1
lands):

- **Per-agent-kind optimizer choice.** Trader / risk → GEPA (PnL is
  noisy; reflective mutation justifies its cost). Intern → BootstrapFewShot
  or MIPROv2 (cleaner classification-ish work). Filters → already
  proven in Stage 1. Codified in a small adapter trait inside
  `xvision-engine` so the choice is per-agent-kind config, not a
  global flag.
- **AutoOptimizer refoundation.** The V3 mutator loop is a GEPA call,
  not random mutation. This is the lever that justifies the work — the
  $15K/month cost projection assumes random mutation.
- **Strategy-detail-page UI.** Same "tune this strategy" opt-in
  action that filter detail will already have, scaled up to the full
  pipeline. The user always sees a diff; the original prompt is
  preserved as a named variant.

**Slotting:** V2G (last hardening before V3) or V3 foundation. Hold
open until Stage 1 lands; the filter-pilot outcome informs whether
Stage 2 is "low risk, do it next" or "needs more soak time."

## Out of this intake

- **Standalone DSPy adoption wave.** Replaced by the two-stage framing
  above. Standalone "adopt DSPy" doesn't justify its own slot.
- **Python sidecar.** DSRs is native Rust.
- **Replacing hand-tuned prompts wholesale.** Optimization is opt-in
  per agent. Hand-tuned starter templates remain the default surface a
  new operator sees.
- **Optimizer in the live trading hot path.** Compilation is offline;
  the live runtime consumes the *result* (a frozen prompt + few-shots).
  No optimizer call inside a decision cycle.
- **Surfacing DSPy / MIPRO / GEPA jargon to vibetraders.** The
  optimizer is hidden behind "tune this filter" / "improve this
  strategy" actions. Vibetraders never need to learn the names.
- **Memory + optimization interaction.** v1 of optimization treats
  V2D memory as frozen runtime context. The interaction — does the
  optimizer also tune the memory recorder? — is a Stage 2 v1.1
  question.
- **Eval-honesty preconditions.** The eval-honesty intake's
  `eval-honesty-smell-tests` and `eval-provider-attestation` MUST
  land before any optimizer is allowed near a real run. An optimizer
  compiled against stub-fixture outcomes is worse than no optimizer.

## Open questions for the conductor

1. **Where does Stage 1 live contractually?** Sub-section of
   `agent-graph-composition`, or companion contract
   `agent-graph-filter-optimizer-dsrs`? **Recommend companion
   contract** — DSRs dependency is its own change.
2. **DSRs beta risk.** API stabilizing, may break. Pin a version,
   budget one tracking upgrade per wave; do not block adoption on 1.0.
3. **GEPA feature parity in DSRs vs Python DSPy.** Whether DSRs's GEPA
   matches the Python implementation's sample efficiency needs a
   one-day spike on `10-gepa-llm-judge.rs` before Stage 1's optimizer
   sub-item starts.
4. **Stage 1 filter pick.** Regime detector? News scanner? Setup
   detector? Conductor picks at decomposition based on which filter
   has the cleanest available labeled dataset.
5. **Metric authority for Stage 1.** Accuracy on holdout is the
   obvious metric; F1 or AUROC might be better for unbalanced
   classes. Defer to the filter pick.
6. **Cycle-count / sample-count minimums.** Stage 1 optimizer is
   gated by minimum labeled samples per filter (~200 to avoid
   overfitting). Numbers to be tuned empirically; defaults exist as
   footgun guards.
7. **Stage 2 slotting.** V2G or V3? Hold open until Stage 1 lands.

## Related artifacts

- This intake's source — cowork session 2026-05-21 (DSPy strategy
  thread + redirect to fold-into-filters framing).
- `MANUAL.md` §scaling — autooptimizer $15K/month cost projection at
  N=100; the lever number that justifies the intake.
- `FOLLOWUPS.md` row 15 — V3 autooptimizer scope.
- `team/board-v2.md` — V2D + V2E shipped 2026-05-21; V2C marketplace
  next-active; filters come after.
- `docs/superpowers/specs/2026-05-21-filter-v1-shape.md` — filter v1
  spec; v1 ships deterministic-only, v1.5 adds LLM filters + DSPy.
- `docs/superpowers/plans/2026-05-21-filter-v1-implementation.md` —
  5-stage v1 plan; v1.5 follow-up section sketches the optimizer wave.
- `team/contracts/filter-v1-shape.md` — Stage 1 contract for v1; this
  intake's Stage 1 is the v1.5 follow-on after v1 lands.
- `team/intake/2026-05-21-eval-honesty-and-agent-graph.md` —
  `agent-graph-composition` track is the umbrella the filter work
  sits under.
- `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` — wave
  roadmap source.
- `crates/xvision-engine/src/agents/templates.rs` — current hand-tuned
  starter templates Stage 2 eventually targets.
- `crates/xvision-engine/src/eval/compare.rs` — A/B compare harness
  the optimizer's metric loop reuses.
- External: DSRs project (https://github.com/krypticmouse/DSRs),
  `dspy-rs` on crates.io, DSRs docs at
  https://dsrs.herumbshandilya.com, GEPA paper.
