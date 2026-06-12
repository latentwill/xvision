# QA Findings Action List — 2026-06-12 Eval Round

Sources:
- `docs/research/2026-06-12-eval-round-platform-findings.md`
- `docs/research/filter-capability-roadmap.md`

This document normalizes the two source documents into actionable QA items.
`QA_TRACKER.md` is the live source of truth for execution state, evidence, and
batch ownership.

## Counts

- Platform findings: 20 (`PF-01` through `PF-20`)
- Filed capability items: 5 (`CAP-941` through `CAP-945`)
- Unfiled capability tranche: 8 (`UF-01` through `UF-08`)
- Total normalized QA items: 33

## Platform Findings

| ID | Severity | GitHub | Actionable finding | Primary area |
|---|---:|---|---|---|
| PF-01 | High | #936 | Reconcile token/model-call accounting between eval and agent-run inspect surfaces. | run inspect, eval accounting |
| PF-02 | High | #935 | Make `total_return_pct` semantics consistent with equity curve and baselines. | eval metrics |
| PF-03 | High | #938 | Fix scenario card `decision_bars: 0` for scenarios with cached/run bars. | scenario inspect |
| PF-04 | Medium | #937 | Make run paths fall back to `$XVN_HOME/secrets/providers.toml` like provider check/list. | provider resolution |
| PF-05 | Medium | #938 | Surface actual token counts in human-readable eval verbose output. | eval CLI |
| PF-06 | Medium | #938 | Show filter-fire count/selectivity and synthesized-row split in eval results/show. | eval observability |
| PF-07 | Medium | #938 | Compute or clearly mark LLM cost estimates instead of always null/incomplete. | eval cost accounting |
| PF-08 | Medium | #939 | Classify scenario regime and warn on strategy/regime mismatch. | scenario/eval validation |
| PF-09 | Medium | #938 | Clarify or split `n_trades` so end-of-scenario liquidation is not confused with model actions. | eval metrics |
| PF-10 | Low | #940 | Normalize JSON response envelopes for `strategy new --prompt` and `--from-file`. | strategy CLI |
| PF-11 | Low | #940 | Add `--asset`/`--timeframe` filters to `xvn bars ls`. | bars CLI |
| PF-12 | Low | #940 | Propagate filter `fire.reason` into `filter_events`. | filter eval events |
| PF-13 | Low | #940 | Clarify or promote filter `status` after `set-filter`. | filter lifecycle |
| PF-14 | Low | #940 | Replace stale smoke-profile default model. | eval profile |
| PF-15 | Low | #940 | Soften `provider models --name` miss with auto-refresh or actionable hint. | provider CLI |
| PF-16 | Low | #940 | Add CLI setter for `risk.risk_pct_per_trade`. | strategy authoring |
| PF-17 | Blocker | #932 | Evaluate SL/TP before filter gating so sleeping in-position strategies still enforce exits. | backtest executor |
| PF-18 | High | #933 | Add optional bracket fields to trader structured-output schema and repair prompt. | LLM schema |
| PF-19 | Medium | #934 | Persist emitted SL/TP bracket values for audit/export. | eval decisions/store |
| PF-20 | Medium | #934 | Add strategy-level take-profit config or file explicit follow-up if out of scope. | strategy risk config |

## Filed Capability Items

| ID | GitHub | Actionable capability | Sequencing |
|---|---|---|---|
| CAP-941 | #941 | Add position-aware tokens plus a `manage` block for deterministic profit/loss/time wakes. | After PF-17 exits are real |
| CAP-942 | #942 | Add offline filter replay, condition attribution, and sweeps. | Parallel after tracker reconciliation |
| CAP-943 | #943 | Add conviction-scaled and risk-at-stop sizing. | After PF-17/PF-18 and before profitability matrix rerun |
| CAP-944 | #944 | Add filter tokens for choppiness, ATR distance, range position, wick fractions, consecutive bars, and stretch tokens. | After PF-17 baseline |
| CAP-945 | #945 | Add default-on trigger context and any-branch attribution. | Prerequisite for UF-01 |

## Unfiled Capability Tranche

| ID | Priority | Actionable capability | Prerequisites |
|---|---:|---|---|
| UF-01 | 1 | Multi-setup filters as first-class labeled `any` branches with per-branch context/cooldowns. | CAP-945 |
| UF-02 | 2 | Short-side strategy family for bear scenarios. | Strategy-side experiment; no platform blocker identified |
| UF-03 | 3 | Always-enter-on-fire no-LLM baseline arm. | Eval baseline structure; fits near PF-02/PF-09 work |
| UF-04 | 4 | Partial-close action for manage wakes. | CAP-941 and SL/TP exit machinery |
| UF-05 | 5 | Filter-aware episodic memory keyed by trigger context and resolved outcomes. | Prior-episodes seed plus outcome-bias guard |
| UF-06 | 6 | Per-scenario fire-rate guardrail at `eval validate`. | CAP-942 replay |
| UF-07 | 7 | Daily/weekly loss-pause tokens. | Realized exits and PnL accounting |
| UF-08 | 8 | Cooldown-after-loss vs cooldown-after-win asymmetry. | PF-17/CAP-941 realized exits |

## Initial Batch Plan

Batch A is tracker and synthesis only. It touches `QA_TRACKER.md`,
`docs/research/2026-06-12-qa-findings-action-list.md`, and Beads metadata.

Batch B targets the realized-PnL blocker path with non-overlapping engine files:
PF-17 and PF-18 first, then PF-19/PF-20 only after storage schema scope is
confirmed. The scope includes the backtest executor, trader response schema,
Cline raw-JSON repair prompt, and focused engine tests/harness migrations.
Verification must include a failing-first regression test proving SL/TP runs
before filter gating, schema tests for bracket fields, wiring proof from the
executor/schema/recovery call sites, and read-only adversarial review.

Remaining batches stay explicitly not-started until their file scopes are
reconciled against active worktrees and branches.
