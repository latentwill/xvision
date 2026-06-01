# 100x Overnight Research Agent Suite — 2026-06-01

This cleans the adversarial feature atlas into bounded 100x tasks. The goal is
not to build a giant new product surface in one pass. The goal is to send
overnight agents through repo-grounded audits that produce durable artifacts:
tests, scripts, SQL views, CLI output, or research-wiki findings.

## Output convention

Every completed agent must write a finding page under:

```text
docs/research/overnight-agents/2026-06-01/<agent-name>.md
```

Each page must include:

- agent name
- date
- source 100x run id/path
- repo surfaces inspected
- findings
- files changed
- verification run
- residual risks or follow-ups

If an agent has no code finding, it still writes the page with the null result
and the reusable by-product it produced.

## Agent 1 — TerminologyLockSentinel

Combines the original CLI/docs drift detector and developer terminology leak
scanner.

Scope:

- Audit operator-facing surfaces: CLI help, `MANUAL.md`,
  `crates/xvision-dashboard/wiki/**`, frontend labels, SSE display labels, and
  operator docs.
- Enforce the 2026-05-27 autoresearcher terminology lock.
- Flag forbidden operator-surface terms: `blake3`, `merkle`, `ed25519`,
  `canonical json`, `Ghost`, `Quarantined`, `CycleSeal`, `Mutation`,
  `Mutator`, `gate-epsilon`, `parent-holdout-score`, and close variants.

Useful feature:

- Cheap, deterministic, CI-friendly. The repo already has a locked two-surface
  naming contract.

Waste risk:

- False positives in developer specs and source identifiers. The scanner must
  distinguish operator-facing files from developer-facing files.

Deliverables:

- A repeatable script or test that catches operator-surface leaks.
- A research-wiki finding page.
- A row-by-row remediation list for any remaining leaks.

## Agent 2 — LayoutRailSentinel

Cleans the three-pane layout violation sweeper.

Scope:

- Inspect `frontend/web/src/**` routes mounted under `<Layout>`.
- Flag `grid-cols-12` / `lg:col-span-4` right-sidebar patterns where the chat
  rail is present.
- Flag remaining `Dialog`, `Modal`, `Sheet`, and `Popover` usage, while
  preserving documented exceptions: toasts, native browser primitives, and the
  mobile-only `MListSheet`.

Useful feature:

- Directly enforces a written design rule and prevents recurring layout
  regressions.

Waste risk:

- A naive grep will report allowed standalone routes and test names. Prefer a
  small route-aware scanner or a documented allowlist.

Deliverables:

- A lint/test/report for layout and popup violations.
- Inline-strip migration suggestions for real violations.
- A research-wiki finding page.

## Agent 3 — TraceCoverageCartographer

Cleans the trace coverage map.

Scope:

- Build a completeness matrix across eval runs, agent runs, spans, checkpoints,
  model calls, tool calls, decisions, findings, determinism receipts, and trace
  exports.
- Prefer a SQL view, CLI report, or reusable diagnostic route over a one-off
  markdown audit.

Useful feature:

- This is the best prerequisite for replay proving and observability cleanup.

Waste risk:

- If the data source is empty or split across multiple SQLite databases, the
  agent must produce a clear null-result map instead of inventing coverage.

Deliverables:

- Reusable coverage query/view/report.
- Focused tests if code is added.
- A research-wiki finding page.

## Agent 4 — ReplayDeterminismProver

Cleans the cycle-trace replay prover.

Scope:

- Reuse existing determinism receipts, checkpoints, model-call records, and
  eval decision records.
- Prove stable replay only for deterministic/stored paths. Do not re-call live
  models or external brokers.
- If replay cannot be proved yet, identify exact missing trace fields and
  produce a blocking matrix.

Useful feature:

- High leverage once trace coverage exists.

Waste risk:

- Full replay is a trap if it silently re-executes stochastic/external calls.
  Hash or stored-payload comparison is acceptable; fake determinism is not.

Deliverables:

- A replay/verifier command or test when feasible.
- Otherwise, a precise gap report.
- A research-wiki finding page.

## Agent 5 — EvalDriftArchaeologist

Cleans eval-drift archaeology.

Scope:

- Compare archived eval runs only when strategy/scenario/bars/seed/engine
  comparability can be established.
- Produce per-algorithm or per-strategy volatility/trust scores from real
  `metrics_json` data.

Useful feature:

- Finds silent regression patterns that green CI can miss.

Waste risk:

- Comparing incomparable runs creates fake signal. The agent must refuse or
  bucket separately when determinism receipts or comparable inputs are missing.

Deliverables:

- A report or CLI that explains comparable cohorts and rejected comparisons.
- A research-wiki finding page.

## Agent 6 — DeadStrategyExhumer

Cleans the dead strategy/orphaned AgentRef audit.

Scope:

- Cross-reference live strategy records, strategy files, `AgentRef` rows, eval
  runs, and agent run history.
- Identify strategies/agents that are truly unused, not merely dormant or
  deferred.

Useful feature:

- Low-risk cleanup with real operational value.

Waste risk:

- Production usage boundaries may be ambiguous. Unknown is a valid state and
  must not be labelled dead.

Deliverables:

- Orphan inventory with confidence levels.
- Safe cleanup recommendations, not destructive deletes.
- A research-wiki finding page.

## Agent 7 — RiskVetoTaxonomist

Cleans the veto reason clusterer.

Scope:

- Prefer structured `VetoReason` aggregation before embeddings or clustering.
- Inspect `Custom(String)` and operator-facing veto text only when present.

Useful feature:

- Can reveal unintended risk-gate firing patterns.

Waste risk:

- Embedding an enum is token waste. Use counts first; reserve LLM analysis for
  free-text or unexplained custom reasons.

Deliverables:

- Structured veto-frequency report.
- Candidate taxonomy only for custom/free-text reasons.
- A research-wiki finding page.

## Agent 8 — CacheShapeMiner

Cleans the per-strategy verdict cache miner.

Scope:

- Audit existing model/tool call cost, prompt hashes, response hashes, strategy
  agent slots, and cache/window settings.
- Do not depend on a nonexistent or planned-only `PerStrategyVerdict` runtime
  surface unless it is actually implemented.

Useful feature:

- May expose expensive stable calls worth caching or partitioning.

Waste risk:

- Premature if the verdict/cache key contract is not real yet.

Deliverables:

- Cache-shape/cost report from existing ledgers.
- Explicit list of missing schema needed for true `PerStrategyVerdict` mining.
- A research-wiki finding page.

## Agent 9 — WeirdSideQuestAnalogueMiner

Keeps the weird structural/cross-domain feature memo, deliberately separated
from implementation work.

Scope:

- Apply the `perception -> proposition -> gate -> act` abstraction to xvision.
- Compare against trading-agent, robotics, autonomous-driving, and reliability
  architectures.
- Produce a memo of missing-stage candidates and dangerous overfitting analogies.
- This is a research memo only: no code changes, no product commitments.

Useful feature:

- Good idea generator. It can name future product bets that repo-local audits
  will never see.

Waste risk:

- High hallucination and generic-architecture risk. Every recommendation must
  be marked as hypothesis, analogy, or repo-grounded.

Deliverables:

- `docs/research/overnight-agents/2026-06-01/WeirdSideQuestAnalogueMiner.md`
- A short “do not build yet” section for weak analogies.
- A follow-up shortlist that could become future intake items.

## Recommended 100x execution order

1. `TerminologyLockSentinel`
2. `LayoutRailSentinel`
3. `TraceCoverageCartographer`
4. `ReplayDeterminismProver`
5. `EvalDriftArchaeologist`
6. `DeadStrategyExhumer`
7. `RiskVetoTaxonomist`
8. `CacheShapeMiner`
9. `WeirdSideQuestAnalogueMiner`

The first two should be easiest to complete with deterministic tests. The trace
and replay pair may produce a gap report instead of full implementation if the
ledger is incomplete. The weird side quest is intentionally memo-only.
