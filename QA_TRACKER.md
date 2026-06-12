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
| 2026-06-12 | G. Batch C isolated and planned | Worktree `.worktrees/qa-batch-c-brackets-20260612` on branch `qa/batch-c-brackets-20260612` created from Batch B branch. Batch C plan saved in `docs/superpowers/plans/2026-06-12-qa-batch-c-bracket-persistence-config.md`; parity-crosswalk files are reserved for another agent and out of scope. |
| 2026-06-12 | H. Batch C plan review iteration 1 failed and revised | Read-only plan review returned feasibility FAIL (missing `sltp.rs` and full `DecisionRow` constructor impact), completeness FAIL (missing dashboard risk editor, generated DTO, MCP follow-up), and scope/alignment PASS. Plan revised before implementation. |
| 2026-06-12 | I. Batch C plan review iteration 2 failed and revised | Read-only plan review returned feasibility FAIL and completeness FAIL because adding fields to public `DecisionRow` would break a direct constructor in owned MCP code. Plan revised to keep `DecisionRow` stable and persist brackets through companion store APIs joined into export/API/chart payloads. |
| 2026-06-12 | J. Batch C plan review exhausted and blocked | Iteration 3 returned feasibility PASS, completeness PASS, scope/alignment FAIL. Remaining blocker: PF-19/PF-20 require `crates/xvision-engine/src/eval/executor/backtest.rs`, `crates/xvision-engine/src/api/eval.rs`, and `crates/xvision-engine/src/strategies/validate.rs`, but `team/OWNERSHIP.md` lists active owners for those files and the QA batch has no approved contract update. No implementation files were edited. |
| 2026-06-12 | K. Batch D isolated and scoped | Worktree `.worktrees/qa-pf15-provider-models-20260612` on branch `qa/pf15-provider-models-20260612` created from Batch C checkpoint. Scope is PF-15 only: `crates/xvision-cli/src/commands/provider.rs` plus `QA_TRACKER.md`. Ownership search found no active owner for `provider.rs`; `qa-pf11-bars-filters-20260612` exists separately, so PF-11 remains untouched. |
| 2026-06-12 | L. Batch D implemented and locally verified; review blocked | PF-15 RED test failed first on the hard-error path, implementation changed the missing configured catalog path to print the refresh hint and return success, focused GREEN checks passed. External adversarial review is blocked for now: Codex reviewer hit usage limit, Gemini CLI has no auth, Claude CLI hung without a verdict and was terminated. |
| 2026-06-12 | M. Batch D adversarial review passed | Fresh read-only Codex implementation review returned PASS with no P0/P1 blockers. Reviewer confirmed only scoped files changed, configured/no-cache catalog returns success with a refresh hint, unknown providers still error through `providers_catalog::get`, and enable/disable paths remain before the catalog lookup. |
| 2026-06-12 | N. Batch C and D branches published | Batch C blocker record pushed and opened as PR #952 against `main`. PF-15 pushed and opened as stacked PR #953 against `qa/batch-c-brackets-20260612`, limiting the PF-15 diff to the provider CLI change plus tracker updates. |
| 2026-06-12 | O. Batch E isolated and PF-04 locally verified | Worktree `.worktrees/qa-pf04-provider-secret-fallback-20260612` on branch `qa/pf04-provider-secret-fallback-20260612` created from the PF-15 tracker head. PF-04 code path was already present; a run-path regression test was added to `api_provider_parity.rs` proving `resolve_provider` accepts a configured provider whose key exists only in `$XVN_HOME/secrets/providers.toml`. Focused and full provider parity checks passed. |
| 2026-06-12 | P. Batch E adversarial review passed | Read-only Codex implementation review returned PASS with no P0/P1 blockers. Reviewer confirmed scope, the new regression test's use of the run-path launch gate, existing eval/optimize wiring through `resolve_provider` and `resolve_provider_key_value`, and honest tracker status. |
| 2026-06-12 | Q. Batch E branch published | PF-04 pushed and opened as stacked PR #954 against `qa/pf15-provider-models-20260612`, limiting the PF-04 diff to the provider parity regression test plus tracker updates. |

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
| `qa/batch-c-brackets-20260612` / `.worktrees/qa-batch-c-brackets-20260612` | Batch C `PF-19`/`PF-20` bracket persistence and take-profit config | `QA_TRACKER.md`, `docs/superpowers/plans/2026-06-12-qa-batch-c-bracket-persistence-config.md` only; implementation files were not edited because required files are actively owned by other tracks | Open PR #952; blocked pending ownership/contract coordination |
| `qa/pf15-provider-models-20260612` / `.worktrees/qa-pf15-provider-models-20260612` | Batch D `PF-15` provider models soft miss | `QA_TRACKER.md`, `crates/xvision-cli/src/commands/provider.rs` | Open stacked PR #953 |
| `qa/pf04-provider-secret-fallback-20260612` / `.worktrees/qa-pf04-provider-secret-fallback-20260612` | Batch E `PF-04` provider key run-path fallback | `QA_TRACKER.md`, `crates/xvision-engine/tests/api_provider_parity.rs` | Open stacked PR #954 |

## Batch Plan

| Batch | Items | File scope | Status | Plan review |
|---|---|---|---|---|
| A | Tracker reconciliation and QA findings synthesis | `QA_TRACKER.md`, `docs/research/2026-06-12-qa-findings-action-list.md`, `.beads/*` | Verified | PASS: feasibility, completeness, scope/alignment |
| B | `PF-17`, `PF-18` realized-PnL blocker path | `crates/xvision-engine/src/eval/executor/backtest.rs`, `crates/xvision-engine/src/agent/llm.rs`, `crates/xvision-engine/src/agent/execute_cline.rs`, `crates/xvision-engine/tests/eval_exit_enforcement.rs`, `crates/xvision-engine/tests/support/eval_harness.rs` | Verified | PASS: feasibility, completeness, scope/alignment |
| C | `PF-19`, `PF-20` bracket persistence/config | Store/migration/export/API/chart/generated DTOs, executor risk-state and SL/TP trigger wiring, risk config, dashboard authoring risk editor, migration, focused tests/helpers. MCP tools are blocked from this batch by existing ownership of `crates/xvision-mcp/src/tools.rs`; public `DecisionRow` remains unchanged to avoid that collision. | Blocked | Iteration 1: feasibility FAIL, completeness FAIL, scope/alignment PASS. Iteration 2: feasibility FAIL, completeness FAIL. Iteration 3: feasibility PASS, completeness PASS, scope/alignment FAIL due active ownership of required files |
| D | `PF-15` provider models soft miss | `crates/xvision-cli/src/commands/provider.rs`, `QA_TRACKER.md` | Verified | Small single-file CLI batch; read-only implementation review PASS |
| E | `PF-04` provider key run-path fallback | `crates/xvision-engine/tests/api_provider_parity.rs`, `QA_TRACKER.md` | Verified | Small regression-coverage batch; no production code changed because run-path fallback already exists; read-only implementation review PASS |
| F | `PF-01`..`PF-16` except active/claimed items, `CAP-*`, `UF-*` remaining items | To be grouped by non-overlapping CLI, frontend, engine, filter, and docs surfaces | Not started | Not started |

## Item Status

Status values: `verified`, `merged`, `blocked`, or `not-started`. During active
work an item may temporarily be `in-progress`, but every item must end in one
of the final states before release closeout.

| ID | Source | GitHub | Severity/Priority | Status | Implementation evidence | Wiring proof | Verification | Adversarial review | Notes |
|---|---|---|---:|---|---|---|---|---|---|
| PF-01 | Platform | #936 | High | not-started |  |  |  |  | Token/model-call accounting split. |
| PF-02 | Platform | #935 | High | not-started |  |  |  |  | `total_return_pct` semantics inconsistent with equity curve. |
| PF-03 | Platform | #938 | High | not-started |  |  |  |  | Scenario card `decision_bars: 0`. |
| PF-04 | Platform | #937 | Medium | verified | Existing run-path implementation resolves keys through the shared env-first/secrets-file helper; this batch adds a regression test in `api_provider_parity.rs` for a configured OpenRouter provider with env unset and key only in `$XVN_HOME/secrets/providers.toml`. | `providers::resolve_provider` is the launch gate used by eval run-path provider checks; the new test exercises that public API surface and confirms the provider is accepted when only the persisted secret is present. Related production wiring already calls `resolve_provider_key_value` from eval dispatch and optimize dispatch. | `scripts/cargo test -p xvision-engine --test api_provider_parity resolve_provider_accepts_key_from_secrets_file_when_env_unset -- --nocapture` passed. `scripts/cargo test -p xvision-engine --test api_provider_parity -- --nocapture` passed 8/8. `git diff --check` and `rustfmt --check crates/xvision-engine/tests/api_provider_parity.rs` passed. | Read-only Codex implementation review PASS, no P0/P1 blockers. Reviewer confirmed scope, launch-gate coverage, eval/optimize production wiring, and tracker accuracy. | Provider key run-path fallback. |
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
| PF-15 | Platform | #940 | Low | verified | `provider.rs` now treats a configured provider with no cached catalog as a soft miss: it prints `xvn provider refresh-models --name <provider>` and returns `Ok(())`; `--enable`/`--disable` paths remain before the read-only catalog path. | `ProviderAction::Models` still dispatches to `models(...)`; `providers_catalog::get` still validates provider existence before returning `Ok(None)`, so unknown providers remain errors while configured/no-cache providers get the soft hint. | RED: `scripts/cargo test -p xvision-cli provider::tests::models_without_cached_catalog_returns_soft_hint -- --nocapture` failed on the hard-error message. GREEN: same focused test passed; `scripts/cargo test -p xvision-cli provider::tests::models_ -- --nocapture` passed 5/5. `git diff --check` and `rustfmt --check crates/xvision-cli/src/commands/provider.rs` passed. Full `scripts/cargo fmt --check --all` was attempted and failed on unrelated pre-existing formatting drift outside this batch. | Read-only Codex implementation review PASS, no P0/P1 blockers. Reviewer confirmed scope, soft-miss success path, unknown-provider error preservation, enable/disable path ordering, and TDD evidence. | Provider models hard-errors before refresh. |
| PF-16 | Platform | #940 | Low | not-started |  |  |  |  | No CLI setter for risk percent. |
| PF-17 | Platform | #932 | Blocker | verified | `backtest.rs` now lets filter-gated in-position bars with SL/TP state reach the deterministic SL/TP block before skipping the agent pipeline; regression test `configured_atr_stop_runs_before_filter_gate_when_position_is_open` added and strengthened after review. | Real executor test uses `ActivationMode::FilterGated`, an active filter with `WakeInPosition::Never`, seeded bars, `Executor::run`, and `RunStore::read_decisions`; the persisted rows prove one `stop_loss` and no trader decision rows between `long_open` and `stop_loss`. | RED: `scripts/cargo test -p xvision-engine --test eval_exit_enforcement configured_atr_stop_runs_before_filter_gate_when_position_is_open -- --nocapture` failed with zero `stop_loss` rows. GREEN: same focused test passed; full `scripts/cargo test -p xvision-engine --test eval_exit_enforcement -- --nocapture` passed 5/5. | First read-only review found P1 missing no-extra-dispatch proof; fixed by asserting the open-to-stop action interval is empty. Second read-only Codex review PASS, no P0/P1. | SL/TP skipped on filter-gated bars. |
| PF-18 | Platform | #933 | High | verified | `ResponseSchema::trader_output()` now uses required-plus-nullable bracket fields for strict structured outputs; `cline_raw_json_repair_prompt()` documents parser-valid optional bracket keys; unit tests cover both. | `ResponseSchema::trader_output()` is the trader structured-output schema used by agent slots; `try_nodecision_recovery` now calls `cline_raw_json_repair_prompt()` for the Cline raw JSON recovery step; parser compatibility was reviewed against `TraderOutput` optional fields. | RED: `scripts/cargo test -p xvision-engine trader_response_schema_allows_optional_bracket_fields -- --nocapture` failed on missing `stop_loss_pct`. GREEN: `scripts/cargo test -p xvision-engine --lib trader_response_schema_allows_optional_bracket_fields -- --nocapture` passed; `scripts/cargo test -p xvision-engine --lib raw_json_repair_prompt_mentions_optional_bracket_fields -- --nocapture` passed. | First read-only review found P1 strict-schema optionality and parser-invalid `?` prompt keys; both fixed. Second read-only Codex review PASS, no P0/P1. | Trader schema forbids bracket fields. |
| PF-19 | Platform | #934 | Medium | blocked | No implementation attempted. Revised plan would persist bracket columns through a companion store API while keeping public `DecisionRow` stable, but required wiring touches actively owned files. | Planned wiring path covered store migration, export, run-detail DTO, generated DTO, chart SSE payload, and executor persistence; not executed because `backtest.rs` and `api/eval.rs` are owned by other active tracks. | Plan Review Gate iteration 3: feasibility PASS, completeness PASS, scope/alignment FAIL. | Read-only scope reviewer blocked the plan because `team/OWNERSHIP.md` marks `crates/xvision-engine/src/eval/executor/backtest.rs` and `crates/xvision-engine/src/api/eval.rs` as owned by other tracks; proceeding would violate collision rules. | Bracket values not persisted. Blocked pending ownership/contract update or owner handoff. |
| PF-20 | Platform | #934 | Medium | blocked | No implementation attempted. Revised plan covered `risk.take_profit_atr_multiple`, serde default, presets, validation, executor fallback, SL/TP ATR trigger behavior, dashboard preservation/editing, and MCP functional compatibility. | Planned wiring path covered engine risk config, `validate_strategy`, executor risk-state construction, `sltp.rs`, dashboard risk editor, and frontend API type; not executed because `backtest.rs` and `strategies/validate.rs` are owned by other active tracks. | Plan Review Gate iteration 3: feasibility PASS, completeness PASS, scope/alignment FAIL. | Read-only scope reviewer blocked the plan because `team/OWNERSHIP.md` marks `crates/xvision-engine/src/eval/executor/backtest.rs` and `crates/xvision-engine/src/strategies/validate.rs` as owned by other tracks; proceeding would violate collision rules. | No strategy-level take-profit config. Blocked pending ownership/contract update or owner handoff. |
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
