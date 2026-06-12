# QA Tracker

Source of truth for the 2026-06-12 QA release pass. This tracker was created
because `QA_TRACKER.md` was absent on `origin/main` at audit time.

## Checkpoint Log

| Time | Checkpoint | Evidence |
|---|---|---|
| 2026-06-12 | A. Repo state audited | Main checkout was dirty and `main` was ahead/behind; isolated worktree `.worktrees/qa-release-manager-20260612` was created from `origin/main` on branch `qa/release-manager-20260612`. |
| 2026-06-12 | B. Tracker reconciled | Source docs present; existing `QA_TRACKER.md` absent; normalized list created in `docs/research/2026-06-12-qa-findings-action-list.md`. |
| 2026-06-12 | C. Batch plan reviewed | Plan Review Gate passed with three read-only Codex reviewers: feasibility PASS, completeness PASS, scope/alignment PASS. Gemini review was unavailable because Gemini CLI had no auth configured. |
| 2026-06-12 | D. Batch B implemented and locally verified | PF-17/PF-18 red tests failed first, implementation wired into executor/schema/Cline recovery path, focused green checks passed. Awaiting read-only adversarial implementation review. |
| 2026-06-12 | E. Batch B adversarial review passed | First read-only review found P1s in PF-18 schema optionality and Cline `?` keys, plus PF-17 missing no-extra-dispatch assertion. Fixes landed; second read-only review returned PASS for PF-17 and PF-18 with no P0/P1 findings. |
| 2026-06-12 | F. Batch B branch published | Commit `cdb19c99` pushed on `qa/release-manager-20260612`; PR #949 opened for review/merge. |

## Reconciled Counts

| Category | Count | IDs |
|---|---:|---|
| Platform findings | 20 | `PF-01`..`PF-20` |
| Filed capability items | 5 | `CAP-941`..`CAP-945` |
| Unfiled capability tranche | 8 | `UF-01`..`UF-08` |
| Total QA items | 33 | No omissions from the two source docs. |

## Branch And Worktree Ownership

| Branch/worktree | Purpose | File scope | Status |
|---|---|---|---|
| `qa/release-manager-20260612` / `.worktrees/qa-release-manager-20260612` | QA reconciliation and first implementation batch | `QA_TRACKER.md`, `docs/research/2026-06-12-qa-findings-action-list.md`, batch-specific files listed below | Open PR #949 |

## Batch Plan

| Batch | Items | File scope | Status | Plan review |
|---|---|---|---|---|
| A | Tracker reconciliation and QA findings synthesis | `QA_TRACKER.md`, `docs/research/2026-06-12-qa-findings-action-list.md`, `.beads/*` | Verified | PASS: feasibility, completeness, scope/alignment |
| B | `PF-17`, `PF-18` realized-PnL blocker path | `crates/xvision-engine/src/eval/executor/backtest.rs`, `crates/xvision-engine/src/agent/llm.rs`, `crates/xvision-engine/src/agent/execute_cline.rs`, `crates/xvision-engine/tests/eval_exit_enforcement.rs`, `crates/xvision-engine/tests/support/eval_harness.rs` | Verified | PASS: feasibility, completeness, scope/alignment |
| C | `PF-19`, `PF-20` bracket persistence/config | Store/migration/export/risk config files to be scoped after Batch B | Not started | Not started |
| D | `PF-01`..`PF-16`, `CAP-*`, `UF-*` remaining items | To be grouped by non-overlapping CLI, frontend, engine, filter, and docs surfaces | Not started | Not started |

## Item Status

Status values: `verified`, `merged`, `blocked`, or `not-started`. During active
work an item may temporarily be `in-progress`, but every item must end in one
of the final states before release closeout.

| ID | Source | GitHub | Severity/Priority | Status | Implementation evidence | Wiring proof | Verification | Adversarial review | Notes |
|---|---|---|---:|---|---|---|---|---|---|
| PF-01 | Platform | #936 | High | not-started |  |  |  |  | Token/model-call accounting split. |
| PF-02 | Platform | #935 | High | not-started |  |  |  |  | `total_return_pct` semantics inconsistent with equity curve. |
| PF-03 | Platform | #938 | High | not-started |  |  |  |  | Scenario card `decision_bars: 0`. |
| PF-04 | Platform | #937 | Medium | not-started |  |  |  |  | Provider key run-path fallback. |
| PF-05 | Platform | #938 | Medium | not-started |  |  |  |  | Token counts hidden in human-readable eval output. |
| PF-06 | Platform | #938 | Medium | not-started |  |  |  |  | Filter-fire/selectivity summary missing. |
| PF-07 | Platform | #938 | Medium | not-started |  |  |  |  | Cost estimates incomplete/null. |
| PF-08 | Platform | #939 | Medium | not-started |  |  |  |  | Scenario regime classification/warning. |
| PF-09 | Platform | #938 | Medium | not-started |  |  |  |  | `n_trades` liquidation semantics. |
| PF-10 | Platform | #940 | Low | not-started |  |  |  |  | Strategy response envelope inconsistency. |
| PF-11 | Platform | #940 | Low | not-started |  |  |  |  | `bars ls` filters. |
| PF-12 | Platform | #940 | Low | not-started |  |  |  |  | Filter fire reason not propagated. |
| PF-13 | Platform | #940 | Low | not-started |  |  |  |  | Filter status stays draft after set-filter. |
| PF-14 | Platform | #940 | Low | not-started |  |  |  |  | Stale smoke default model. |
| PF-15 | Platform | #940 | Low | not-started |  |  |  |  | Provider models hard-errors before refresh. |
| PF-16 | Platform | #940 | Low | not-started |  |  |  |  | No CLI setter for risk percent. |
| PF-17 | Platform | #932 | Blocker | verified | `backtest.rs` now lets filter-gated in-position bars with SL/TP state reach the deterministic SL/TP block before skipping the agent pipeline; regression test `configured_atr_stop_runs_before_filter_gate_when_position_is_open` added and strengthened after review. | Real executor test uses `ActivationMode::FilterGated`, an active filter with `WakeInPosition::Never`, seeded bars, `Executor::run`, and `RunStore::read_decisions`; the persisted rows prove one `stop_loss` and no trader decision rows between `long_open` and `stop_loss`. | RED: `scripts/cargo test -p xvision-engine --test eval_exit_enforcement configured_atr_stop_runs_before_filter_gate_when_position_is_open -- --nocapture` failed with zero `stop_loss` rows. GREEN: same focused test passed; full `scripts/cargo test -p xvision-engine --test eval_exit_enforcement -- --nocapture` passed 5/5. | First read-only review found P1 missing no-extra-dispatch proof; fixed by asserting the open-to-stop action interval is empty. Second read-only Codex review PASS, no P0/P1. | SL/TP skipped on filter-gated bars. |
| PF-18 | Platform | #933 | High | verified | `ResponseSchema::trader_output()` now uses required-plus-nullable bracket fields for strict structured outputs; `cline_raw_json_repair_prompt()` documents parser-valid optional bracket keys; unit tests cover both. | `ResponseSchema::trader_output()` is the trader structured-output schema used by agent slots; `try_nodecision_recovery` now calls `cline_raw_json_repair_prompt()` for the Cline raw JSON recovery step; parser compatibility was reviewed against `TraderOutput` optional fields. | RED: `scripts/cargo test -p xvision-engine trader_response_schema_allows_optional_bracket_fields -- --nocapture` failed on missing `stop_loss_pct`. GREEN: `scripts/cargo test -p xvision-engine --lib trader_response_schema_allows_optional_bracket_fields -- --nocapture` passed; `scripts/cargo test -p xvision-engine --lib raw_json_repair_prompt_mentions_optional_bracket_fields -- --nocapture` passed. | First read-only review found P1 strict-schema optionality and parser-invalid `?` prompt keys; both fixed. Second read-only Codex review PASS, no P0/P1. | Trader schema forbids bracket fields. |
| PF-19 | Platform | #934 | Medium | not-started |  |  |  |  | Bracket values not persisted. |
| PF-20 | Platform | #934 | Medium | not-started |  |  |  |  | No strategy-level take-profit config. |
| CAP-941 | Capability | #941 | 1 | not-started |  |  |  |  | Position-aware tokens and manage block. |
| CAP-942 | Capability | #942 | 2 | not-started |  |  |  |  | Offline filter replay and sweeps. |
| CAP-943 | Capability | #943 | 3 | not-started |  |  |  |  | Conviction-scaled and risk-at-stop sizing. |
| CAP-944 | Capability | #944 | 4 | not-started |  |  |  |  | New filter tokens. |
| CAP-945 | Capability | #945 | 1 | not-started |  |  |  |  | Trigger context and any-branch attribution. |
| UF-01 | Roadmap | unfiled | 1 | not-started |  |  |  |  | Multi-setup filters. |
| UF-02 | Roadmap | unfiled | 2 | not-started |  |  |  |  | Short-side strategy family. |
| UF-03 | Roadmap | unfiled | 3 | not-started |  |  |  |  | No-LLM always-enter-on-fire baseline. |
| UF-04 | Roadmap | unfiled | 4 | not-started |  |  |  |  | Partial-close action. |
| UF-05 | Roadmap | unfiled | 5 | not-started |  |  |  |  | Filter-aware episodic memory. |
| UF-06 | Roadmap | unfiled | 6 | not-started |  |  |  |  | Per-scenario fire-rate guardrail. |
| UF-07 | Roadmap | unfiled | 7 | not-started |  |  |  |  | Daily/weekly loss-pause tokens. |
| UF-08 | Roadmap | unfiled | 8 | not-started |  |  |  |  | Cooldown-after-loss/win asymmetry. |

## Closeout Requirements

- Every completed item must include implementation evidence, wiring proof,
  verification command or manual path, adversarial review result, and tracker
  update.
- Any item not completed in this pass must remain explicitly `not-started` or
  become `blocked` with a concrete blocker.
- Final closeout must account for branches/worktrees and explain any dirty
  status.
