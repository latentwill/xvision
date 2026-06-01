# WeirdSideQuestAnalogueMiner

**Date:** 2026-06-01
**Source:** 100x run `.100x/runs/20260601_002720/`
**Artifact type:** research-only feature memo

## Surfaces Inspected

- xvision pipeline vocabulary from `CLAUDE.md`
- strategy/agent composition surfaces under `crates/xvision-engine/src/agents/**`
- eval/risk/executor traces under `crates/xvision-engine/src/eval/**`
- identity/marketplace notes and Mantle surfaces where relevant

## Abstraction

xvision's trading loop can be abstracted as:

```text
perception -> proposition -> gate -> act
```

Local mapping:

- perception: briefing, filters, market context, memory recall
- proposition: trader/agent decision
- gate: risk layer, eval gates, holdout checks, operator sign-off
- act: executor, paper/live venue, marketplace/identity publishing

## Side Quest Findings

### 1. Explicit World Model Stage

Hypothesis: insert a durable world model or state-estimate artifact between perception and proposition.

Potential benefit:

- Makes the trader's belief state inspectable before it chooses an action.
- Lets eval compare bad-belief vs bad-policy failures.

Why not build yet:

- xvision already has briefing, filters, and memory. A world-model stage could duplicate those unless it has a strict schema and evaluation contract.

Possible intake shape:

- Add a read-only `MarketStateEstimate` artifact emitted by filter/briefing stages and consumed by trader slots.

### 2. Reflection Stage After Risk Veto

Hypothesis: after a veto, route the original trader decision and structured `VetoReason` back into a non-trading reflection path.

Potential benefit:

- Converts risk vetoes into training examples without changing live execution.
- Could improve strategy authoring and future trader prompts.

Why not build yet:

- Reflection can become a prompt-token sink if it runs on every veto. Start with SQL aggregation (`RiskVetoTaxonomist`) and only reflect on repeated custom reasons.

Possible intake shape:

- Batch reflection on top N veto clusters once per day, not inline in execution.

### 3. Safety Case / Assurance Case Artifact

Hypothesis: borrow the safety-case pattern from autonomous systems: each strategy carries an operator-readable argument for why it is allowed to run.

Potential benefit:

- Unifies risk caps, eval receipts, untouched-period results, and operator sign-off into a single review surface.
- Strong marketplace trust story.

Why not build yet:

- Requires stable terminology, receipts, and trace coverage first. Premature UI would be decorative.

Possible intake shape:

- Generate a read-only `assurance_case.md` from existing eval attestations and risk config.

### 4. Scenario Adversary

Hypothesis: add a stage that actively searches for scenarios where the strategy fails, similar to adversarial testing in autonomy systems.

Potential benefit:

- More honest than average-case backtests.
- Produces better "do not deploy in regime X" labels.

Why not build yet:

- Could overfit to synthetic scenarios if not grounded in real data and pinned manifests.

Possible intake shape:

- Offline scenario-miner that proposes candidate stress windows; human accepts before eval.

### 5. Post-Act Forensics

Hypothesis: after executor or paper fill, add a forensic record that explains actual actuation vs intended action.

Potential benefit:

- Separates "agent decision was bad" from "broker/fill/venue behavior changed the result."
- Helps live/paper parity.

Why not build yet:

- Trace coverage and broker span completeness should land first.

Possible intake shape:

- Broker/fill span completion track, then a fill-quality report.

## Do Not Build Yet

- Full cross-domain robotics architecture import. Too generic.
- Always-on reflection after every decision. Token sink.
- A new world-model UI before the schema is useful.
- Autonomous overnight PR filer. Side effects are too high for this suite.

## Follow-Up Shortlist

1. `risk-veto-reflection-batch` — daily batch reflections only for repeated custom/free-text veto reasons.
2. `strategy-assurance-case` — generated markdown from eval receipts, risk caps, and operator sign-off.
3. `scenario-adversary-audit` — read-only scenario-miner that proposes stress tests.
4. `broker-fill-forensics` — executor trace completion before any actuation-quality scoring.

## Files Changed

- `docs/research/overnight-agents/2026-06-01/WeirdSideQuestAnalogueMiner.md`

## Verification

Research-only memo. No code verification required.

## Residual Risks

This memo uses analogy. Treat every suggested feature as a hypothesis until backed by a concrete xvision data contract and acceptance test.
