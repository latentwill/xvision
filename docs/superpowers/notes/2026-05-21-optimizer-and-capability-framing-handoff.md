# Handoff — Optimizer adoption + capability framing

**Date:** 2026-05-21
**Status:** Discussion substrate + audit results, ready for an implementing agent.
**Audience:** Whichever agent picks up the optimizer wave next.

This note captures a multi-turn strategy conversation that started with "should
we use DSPy?" and arrived at a substantively different design than where it
began. The decisions that survived, the ones that didn't, and the audit results
that ground the next step. Read top-to-bottom; the action items are at the end.

## TL;DR

1. **Optimization framing:** xvision adopts DSPy-style prompt optimization. The
   primary optimizer is GEPA (reflective evolutionary), chosen because its
   `MetricOutcome::with_feedback(score, feedback_string)` shape matches what
   xvision has — outcomes plus rich trace-derived narrative, no requirement for
   pre-labeled supervised data.
2. **Mechanism:** DSRs (`dspy-rs` on crates.io, beta) is the workspace dependency.
   Live runtime is ClineSDK; DSRs runs on rig-core. The bridge is a rig-core
   `CompletionModel` impl backed by ClineSDK so optimization and runtime hit the
   same SDK behavior. The optimizer output is a String (the new `instruction`)
   written back into `AgentSlot.system_prompt`, versioned via `prompt_version`.
   DSRs never touches the live decision path.
3. **No "Watcher" wave.** Watcher v1 as previously written has been fully
   replaced by Filter — the existing `xvision-filters` crate is the canonical
   surface and already implements everything that spec described. The watcher
   v1 spec / plan / contract written 2026-05-21 are duplication and should be
   retired.
4. **No rigid agent types.** "Critic", "Trader", "Intern" are capabilities, not
   types. The agent model carries `capabilities: Vec<Capability>`; the
   optimizer adapter is selected by capability, not by an `AgentKind` enum.
5. **First target:** the `grades_decisions` capability — an agent that
   post-hoc scores completed cycles. Dataset is free (every existing eval run
   produces `(decision, outcome)` pairs). Metric is graded forward-return
   agreement, train/holdout split ≥30%. No live decision impact possible
   because the critic capability is post-hoc only.
6. **Two metric families** — strategy agents (PnL family, expanded) and chat
   rail agents (tool-use family, expanded). Both first-class. Chat rail
   agents *produce* filters / strategies / other agents, so their metrics
   compose with downstream artifact performance — captured below as
   "Capability composition for chat rail."

## Background: how we got here

The conversation arc (compressed):

- **Started:** "Would DSPy help xvision? Wouldn't it complicate things for
  vibetraders?" The original ask was about whether to adopt DSPy-style
  optimization at all.
- **Identified killer use case:** xvision's planned V3 autoresearcher is
  itself a hand-rolled prompt optimizer (per `MANUAL.md` §scaling — "~100
  mutator variants/night × 50K-token briefings × Sonnet-class evaluation, the
  LLM bill is ~$15K/month"). GEPA-style reflective mutation should bend that
  unit economics meaningfully.
- **Slotting:** Adoption belongs *before* autoresearcher, not as part of it.
  Autoresearcher should consume the optimizer as a primitive.
- **Original first target was wrong.** Initial recommendation pointed at the
  "watcher" abstraction and proposed a critic agent. This was wrong on two
  axes — see §"What changed mid-conversation."
- **Final shape:** is what's captured in this handoff.

## What changed mid-conversation

Four corrections shifted the design substantially. Each is locked.

### Correction 1 — Filter, not Watcher

The "Watcher v1" framing was dropped. `xvision-filters` already exists,
implements deterministic JSON/TOML rules that determine firing conditions for
an agent, and is more comprehensive than the watcher spec I had drafted. The
substance is identical; the name is different. "Watcher" was never the right
word.

**Consequence:** the three watcher v1 artifacts written 2026-05-21 should be
retired (see Action items). The DSPy intake's "v1.5" framing was carrying the
wrong concept and should be reworked accordingly.

### Correction 2 — Capabilities, not types

xvision is moving away from rigid agent types. There is no `AgentKind` enum
on the long-term roadmap. Capabilities compose; types lock things into a
closed enum. The optimizer's design follows:

- Agent model gains `capabilities: Vec<Capability>` (not `kind: AgentKind`).
- A capability brings (a) a metric scoring function, (b) a feedback-string
  format, (c) a dataset shape.
- The optimizer adapter is selected by capability.
- Same agent can declare multiple capabilities; same machinery optimizes any
  capability for any agent that declares it.

**Consequence:** the v1.5 LLM-watcher work as previously sketched (adding
`AgentKind` enum) is replaced by capability composition. The first target is a
*capability* (`grades_decisions`), not a role (the critic).

### Correction 3 — ClineSDK, not direct Anthropic

xvision's live runtime is ClineSDK (the `AcpxIntern` path observed in the
V2D intake). DSRs uses rig-core with its own Anthropic adapter. Two
different SDKs.

**Earlier mistake:** I claimed Path A (DSRs as-is) was fine for non-tool
capabilities and Path C (roll-your-own GEPA on ClineSDK) was probably right
for everything else. That was an overcorrection on the alignment concern.

**Locked path:** an adapter — implement rig-core's `CompletionModel` trait for
ClineSDK, then pass the adapter into DSRs's `LM` builder. The adapter is at
the rig-core level, not the DSRs level. This inherits borrowed correctness on
the GEPA algorithm (which has real subtleties — reflection prompt design,
mutation selection over the archive, hyperparams for the reflective LM) while
keeping the optimizer's dispatch behavior identical to xvision's runtime.

### Correction 4 — Chat rail agents are first-class

Two metric families exist, both first-class:

**Strategy agents** (act on markets, embedded in trading pipeline):

- PnL / Sharpe / drawdown — noisy outcome, expensive
- Per-decision forward-return — graded score on lookahead window, clean per-cycle, splittable, the main signal
- Calibration — Brier on conviction vs realized return
- Net-of-inference-cost return — outcome minus tokens
- Selectivity — wake-rate × hit-rate trade-off
- Action discipline — % cycles with valid action, no-op rates

**Chat rail agents** (interact with the user, author artifacts):

- Tool-use accuracy
- Schema/format adherence (matches slot `output_schema`)
- Instruction following
- Token budget adherence
- Latency
- Helpfulness — judge-graded against calibrated judge
- Conversation coherence
- Capability appropriateness — invokes the right capability per intent
- **Artifact-quality composition** — see next section

## Capability composition for chat rail

A late insight that reshapes the optimizer's reach: **chat rail agents produce
filters, strategies, and agents as artifacts.** Their quality is not just
"helpful response to user" — it's "the filters / strategies / agents they
generated perform well downstream."

This creates a hierarchical optimization loop:

1. Strategy agents optimize against forward-return-style metrics (direct
   signal on their own decisions).
2. Chat rail agents optimize against a composite metric: short-loop chat
   metrics (tool use, schema adherence, helpfulness) AND long-loop artifact
   quality (the downstream performance of what they generated).
3. GEPA's feedback string is the natural carrier for long-loop signal —
   "the strategy you produced got Sharpe X over 3 holdout scenarios; the
   filter you specified fires too rarely; the agent you grading-configured
   has 51% accuracy on holdout."

This is meta-optimization. The chat rail authoring agent IS the autoresearcher
in the limit — the artifacts it produces are exactly the artifacts
autoresearcher mutates. Worth being explicit that they're the same loop at
different scopes.

## Audit results

### `xvision-filters` audit

The crate is at `crates/xvision-filters/`. Read 2026-05-21.

**What exists (everything from the retired watcher v1 spec, plus more):**

- `src/types.rs` — `Filter`, `FilterId`, `FilterStatus`, `ActivationMode`
  (`EveryBar` / `FilterGated` / `CompiledRules`), `ScanCadence`,
  `WakeInPosition`, `ConditionTree` (`All` / `Any`), `Condition`, `Operand`
  (`Indicator` / `Numeric` / `Range`), `IndicatorName` (6 entries:
  `Ema`, `Sma`, `Rsi`, `Atr`, `AtrPct`, `Close` with documented period
  bounds), `IndicatorRef`, `Operator` (8 entries: `>`, `<`, `>=`, `<=`,
  `==`, `crosses_above`, `crosses_below`, `between`). All `ts-rs` exported
  behind feature gate.
- `src/parse.rs` — `parse_toml` + `parse_json` with field-path errors.
- `src/validate.rs` — validation with stable `E_FILTER_*` error codes
  (codes mirror what the retired watcher spec called `E_WATCHER_*` — just
  the prefix changed).
- `src/runtime.rs` — `RuntimeFilter::evaluate` per-bar, returning
  `ActivationDecision` (`Warming` / `Inactive` / `Active { transition:
  Trip | Hold }` / `Cooldown` / `CappedForDay` / `SuppressedInPosition`).
  Includes `dsl_to_filter_signal` adapter producing `BridgedFilterSignal`
  for engine-side agent dispatch consumption.
- `src/state.rs` — `FilterState` carrying warmup, cooldown, daily wakeup
  counter, previous-bar leaf cache.
- `src/events.rs` — `FilterEventV1` (with `schema_version` field for
  forward compat), `FilterSummary`, `SuppressedReason`.
- `src/indicators.rs` — incremental math for the six indicators.
- `src/errors.rs` — `ParseError` + `ValidationError` with codes + paths.
- `tests/parse_roundtrip.rs`, `tests/validate_codes.rs`,
  `tests/golden_determinism.rs` — golden fixtures already in place.
- `AVG_BRIEFING_TOKEN_COST: u64 = 50_000` constant for tokens-saved
  calculation in `FilterSummary`.

**What might still be missing for the optimizer integration:**

- Forward-return label extraction utility (`compute_forward_return(decision,
  bars, n) -> f64`). Looks bespoke to optimization; probably belongs in a new
  `xvision-optimizer` crate, not in `xvision-filters`.
- Eval-review frontend panels for `FilterEventV1` / `FilterSummary`.
  Status unknown — would need a `frontend/web/src/features/eval/` audit.
  Out of scope for the optimizer wave; flag as a follow-up if missing.

**Conclusion:** the filter substrate is complete. No watcher wave needed.

### DSRs API audit

Sources: docs.rs/dspy-rs/0.7.3, https://github.com/krypticmouse/DSRs.

- `dspy_rs::core::lm` exposes `LM` (struct), `LMBuilder`, `LMResponse`,
  `DummyLM`. **Important:** `LM` is a concrete struct, not a trait. There
  is no public `LM` trait you implement directly. The `client_registry`
  submodule is where rig-core completion models are registered.
- `dspy_rs::optimizer` has modules `gepa`, `mipro`, `copro`, `pareto`, plus
  an `Optimizer` trait. GEPA is the primary target; the others are
  available if needed.
- The seam for ClineSDK is **one level deeper than initially claimed**:
  you implement rig-core's `CompletionModel` trait for ClineSDK, then
  configure DSRs's `LM` to use it via the builder + client registry. The
  adapter is rig-core ↔ ClineSDK; DSRs consumes the result transparently.
- DSRs depends on `rig-core ^0.22.0`, `async-trait`, `tokio`, `reqwest`,
  `schemars`. Reasonable surface; nothing exotic.
- Beta status: API stabilizing, may break. Pin to 0.7.3 and budget one
  tracking upgrade per wave.

**Conclusion:** the adapter path is feasible but requires understanding
rig-core's completion-model interface. ~Couple hundred lines for the adapter,
plus a small wrapper that wires DSRs's LM to use it. Substantially less work
than reimplementing GEPA; pays back forever in borrowed correctness.

## First target proposal — `grades_decisions` capability

**The capability.** An agent that declares `grades_decisions` accepts a
completed cycle (decision text + briefing + outcome + next N bars) and emits
a quality score with rationale. Post-hoc only; no live decision impact
possible.

**Dataset.** Every existing eval run produces `(decision, outcome)` pairs.
The metric infrastructure extracts forward-return labels:
`forward_return(decision_bar, N=20) = (close[t+N] - close[t]) / close[t]`,
graded (not binary), normalized by per-scenario volatility (z-score over the
lookahead). Train/holdout split ≥ 30%. Minimum dataset size before "tune"
is enabled: ~200 cycles per scenario family.

**Metric.** Agreement between the critic's grade and the forward-return
label. Could be Spearman correlation (graded) or AUROC (if binarized at
threshold). The `MetricOutcome::with_feedback(score, feedback)` carries the
correlation/AUROC as score and the cycle's trace narrative (decision
rationale + actual outcome + observed return) as feedback.

**Why first.** Lowest blast radius (post-hoc, no live impact), cleanest
metric (label is deterministic from price data), reusable downstream (a
calibrated critic IS the judge for any subsequent capability optimization,
including autoresearcher's mutator variants).

**Composition.** Once `grades_decisions` is calibrated:

- Strategy agents optimize against a composite metric: forward-return
  agreement (direct) PLUS the critic's per-decision grade (judged).
- Chat rail agents optimize partly against artifact downstream performance
  judged by the same critic — "how good are the strategies you produced?".
- Autoresearcher uses the critic to grade mutator variants without full
  backtests, which is where the $15K/month projection actually bends.

## Validity discipline (non-negotiable)

The user's stated goal is "track valid improvements for autoresearcher and
internal optimization" — not just any improvement. Concrete:

- Every optimizer run reports `train_score`, `holdout_score`,
  `improvement_with_ci`. No improvement claim without holdout.
- Holdout fraction ≥ 30%.
- Minimum dataset size per-capability before "tune" is enabled:
  - `grades_decisions` and other classification-shaped capabilities: ~200 cycles
  - PnL-outcome capabilities: ~500+ cycles
- Improvements smaller than the noise floor (computed from train→train
  split-half variance) are reported as "no improvement" regardless of point
  estimate.
- Every optimized variant is a named artifact; the user (or autoresearcher)
  compares pairs on the holdout. No automatic promotion.
- The optimizer cost runs on internal credits, not user tokens. Live
  runtime never re-pays optimization cost (consumes a frozen String).

The optimizer's value is honest "no improvement" reports as much as it is
the rare "+5% on holdout with CI" wins. Pattern-match for the autoresearcher:
1× real signal and 9× honest "no signal" is better than 10× overconfident
"improvements" half of which are wrong.

## Open questions for the implementing agent

1. **rig-core `CompletionModel` shape.** Read rig-core's docs (probably
   start at `https://docs.rs/rig-core`) and confirm the trait is
   externally implementable. If the trait is sealed or only consumed via
   concrete types, the adapter path collapses and the team should
   re-evaluate Path C (roll-your-own).
2. **ClineSDK Rust surface.** Confirm xvision's `AcpxIntern` exposes the
   primitives a `CompletionModel` impl needs (model handle, completion
   request, streaming or sync). Read `crates/xvision-intern/`.
3. **Forward-return label specifics.** N=20 bars is a starting guess.
   Calibrate against a small set of hand-graded decisions before locking.
   Consider per-scenario volatility normalization vs raw return.
4. **Capability registration surface.** Where do `capabilities: Vec<Capability>`
   declarations live in the agent model? Likely an extension of
   `crates/xvision-engine/src/agents/model.rs:AgentSlot`. Needs a
   migration; coordinate with the agents-page surface.
5. **Critic v1 agent shape.** Prompt template, expected output schema,
   how it consumes the cycle bundle. This is the "what does the first
   target actually look like" question that wasn't fully resolved in the
   conversation.
6. **`xvision-filters` event surfacing.** Quick check whether
   `frontend/web/src/features/eval/` surfaces `FilterEventV1` and
   `FilterSummary` already. If not, that's a small follow-up but does
   not block the optimizer wave.

## Action items (concrete next steps)

In suggested execution order:

1. **Retire the watcher v1 artifacts.** Delete or recast as historical
   notes the three files written 2026-05-21:
   - `.worktrees/glamin-cortex-explorations/docs/superpowers/specs/2026-05-21-watcher-v1-shape.md`
   - `.worktrees/glamin-cortex-explorations/docs/superpowers/plans/2026-05-21-watcher-v1-implementation.md`
   - `.worktrees/glamin-cortex-explorations/team/contracts/watcher-v1-shape.md`
   The substance is duplication of `xvision-filters`. Recommend deletion
   with a one-line tombstone redirecting to this handoff.

2. **Update the DSPy intake.** Rewrite
   `team/intake/2026-05-21-dspy-dsrs-optimizer-adoption.md` so it:
   - Drops watcher terminology entirely.
   - Reframes around the capability model (`grades_decisions` as first
     target, not "the critic").
   - Captures both metric families (strategy agents, chat rail agents)
     with the chat-rail artifact-quality composition explicitly noted.
   - Names DSRs + rig-core ↔ ClineSDK adapter as the locked path (not
     "Option 2 among three") with the rig-core trait check as the
     remaining open question.
   - Surfaces validity discipline as a core constraint.
   - Cross-links this handoff doc.

3. **Verify rig-core's `CompletionModel` is externally implementable.**
   ~30 min spike on rig-core docs. Output: a one-paragraph note on
   feasibility appended to this handoff.

4. **Draft the `xvision-optimizer` crate plan.** Stages, file targets,
   verification — same shape as the filter v1 plan that shipped. Lives
   at `docs/superpowers/plans/2026-05-2X-optimizer-v1-implementation.md`.
   Should cover:
   - Workspace dep on `dspy-rs` 0.7.3 pinned.
   - rig-core → ClineSDK adapter.
   - DSRs `LM` builder wiring.
   - Capability registration surface on the agent model.
   - Forward-return label extractor.
   - `grades_decisions` capability prompt + schema.
   - GEPA wiring with train/holdout split, validity reporting, named-variant
     artifact emission.

5. **Decompose into Stage 1 contract.** First stage probably the rig-core
   adapter + DSRs wire-up — small, testable, no agent-model changes yet.

## Source links

- Original conversation: cowork session 2026-05-21 (DSPy → DSRs → capability
  framing arc).
- `team/intake/2026-05-21-dspy-dsrs-optimizer-adoption.md` — intake to be
  rewritten per Action item #2.
- `team/intake/2026-05-21-eval-honesty-and-agent-graph.md` — adjacent intake,
  some `agent-graph-composition` track items overlap (capability model
  surface).
- `MANUAL.md` §scaling — the $15K/month autoresearcher cost projection that
  motivates GEPA's sample efficiency argument.
- `crates/xvision-filters/` — the canonical filter crate; obsoletes the
  retired watcher v1 spec.
- DSRs project: https://github.com/krypticmouse/DSRs
- DSRs docs: https://dsrs.herumbshandilya.com
- `dspy-rs` crate: https://crates.io/crates/dspy-rs (0.7.3 at audit time)
- rig-core: https://docs.rs/rig-core (verify `CompletionModel` trait shape)

## Open items not in scope for this handoff

- Chat rail agent inventory — which existing agents declare which
  capabilities, who their authoring touchpoints are. Belongs in the
  capability-model PR sequence, not here.
- Autoresearcher integration design — how `grades_decisions` becomes a
  primitive the autoresearcher consumes. V3 work; this handoff sets up
  the substrate for it but does not specify it.
- v2-board slotting — where the optimizer wave lives in the wave
  sequence (V2G? V3 foundation?). Conductor's call at decomposition time.
