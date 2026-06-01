# Overnight Agents — 2026-06-01

Research-wiki findings from the 2026-06-01 100x overnight research-agent suite.

## 100x Runs

- `.100x/runs/20260601_002720/` — broad suite run. Completed analysis, generated useful artifacts, then hit a 100x budget-accounting bug before a clean implement/test/review close.
- `.100x/runs/20260601_003231/` — narrower Terminology/Layout slice. Completed analysis, began implementation, then wedged in the same 100x implementation stage. Artifacts were preserved and verified manually.

## Agent Findings

- [TerminologyLockSentinel](TerminologyLockSentinel.md) — operator-surface terminology leaks and reusable scanner.
- [LayoutRailSentinel](LayoutRailSentinel.md) — chat-rail layout and popup-rule violations.
- [TraceCoverageCartographer](TraceCoverageCartographer.md) — trace coverage SQL and ledger map.
- [ReplayDeterminismProver](ReplayDeterminismProver.md) — deterministic replay gap report.
- [EvalDriftArchaeologist](EvalDriftArchaeologist.md) — comparable-cohort drift SQL and refusal protocol.
- [DeadStrategyExhumer](DeadStrategyExhumer.md) — orphan/dormant strategy inventory tool.
- [RiskVetoTaxonomist](RiskVetoTaxonomist.md) — structured veto frequency queries.
- [CacheShapeMiner](CacheShapeMiner.md) — model-call cost/cache-shape queries.
- [WeirdSideQuestAnalogueMiner](WeirdSideQuestAnalogueMiner.md) — research-only cross-domain feature memo.

## Generated Reusable Artifacts

- `scripts/terminology-lock-sentinel.sh`
- `scripts/layout-rail-sentinel.sh`
- `scripts/trace-coverage-cartographer.sql`
- `scripts/eval-drift-archaeologist.sql`
- `scripts/dead-strategy-exhumer.sh`
- `scripts/risk-veto-taxonomy.sql`
- `scripts/cache-shape-miner.sql`

## Verification Summary

- `bash scripts/terminology-lock-sentinel.sh` exits 1 with current violations.
- `bash scripts/layout-rail-sentinel.sh` exits 1 with current violations.
- SQL artifacts are read-only and intended to run against `$XVN_HOME/engine.db` or `$XVN_HOME/core.db` as documented in each finding.

## 100x Runner Note

The suite was executed through `100x`, but this local 100x version (`v1.1.0`) emitted a malformed multi-line cost value after analysis. That caused shell arithmetic failures in the implementation stage for the broad run. The artifacts and analysis were still written to disk, then verified and organized here.
