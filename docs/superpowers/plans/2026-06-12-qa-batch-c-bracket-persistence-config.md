# QA Batch C Bracket Persistence And Config Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Verify and implement `PF-19` bracket persistence/export and `PF-20` strategy-level take-profit configuration without touching the parity-crosswalk worktree or docs.

**Architecture:** Add bracket columns to `eval_decisions` with a SQLite migration, but keep the public `DecisionRow` struct unchanged because `crates/xvision-mcp/src/tools.rs` constructs it directly and is owned by another track. Store bracket data through a companion `DecisionBrackets`/`DecisionBracketRow` API keyed by `(run_id, decision_index)`, then join that companion data into eval export, run-detail DTOs, generated frontend DTOs, live chart events, and the backtest executor. Add a conservative `risk.take_profit_atr_multiple` config field that defaults to `0.0` for legacy compatibility and is only used as an ATR take-profit fallback when the model emits no take-profit bracket. The dashboard risk editor must preserve and edit the field; MCP documentation for the same shape is blocked from this batch because `crates/xvision-mcp/src/tools.rs` is actively owned by another track.

**Tech Stack:** Rust, SQLite/sqlx migrations, serde, existing `scripts/cargo` wrapper, focused integration/unit tests.

---

## File Structure

- Modify `QA_TRACKER.md`: Batch C checkpoint, ownership, PF-19/PF-20 evidence.
- Create `crates/xvision-engine/migrations/065_eval_decision_brackets.sql`: additive bracket columns on `eval_decisions`.
- Modify `crates/xvision-engine/src/api/mod.rs`: include/apply migration and update fresh DB fallback schema.
- Modify `crates/xvision-engine/src/eval/store.rs`: companion bracket type/API plus insert/select/map; leave `DecisionRow` fields unchanged.
- Modify `crates/xvision-engine/src/eval/export.rs`: decision export fields and tests.
- Modify `crates/xvision-engine/src/api/eval.rs`: run-detail DTO fields.
- Modify `crates/xvision-engine/src/api/chart.rs`: live decision event fields.
- Modify `crates/xvision-engine/src/eval/executor/backtest.rs`: populate persisted bracket companion rows, emit chart bracket payloads, and use risk-level TP fallback.
- Modify `crates/xvision-engine/src/eval/executor/sltp.rs`: allow ATR take-profit evaluation when `tp_atr_mult` is present even if percent TP is zero.
- Modify `crates/xvision-engine/src/strategies/risk.rs`: `take_profit_atr_multiple` with serde default and presets.
- Modify `crates/xvision-engine/src/strategies/validate.rs`: reject negative TP ATR multiples.
- Modify `frontend/web/src/api/strategies.ts`: frontend `RiskConfig` includes `take_profit_atr_multiple`.
- Modify `frontend/web/src/routes/authoring.tsx` and `frontend/web/src/routes/authoring-risk.test.tsx`: dashboard risk editor preserves and can edit the TP ATR multiple.
- Regenerate or update generated frontend DTO files under `frontend/web/src/api/types.gen/**` for `DecisionRowDto`; generated files are allowed by `team/OWNERSHIP.md` for tracks editing ts-export Rust types.
- Modify focused tests: `crates/xvision-engine/tests/eval_store.rs`, `crates/xvision-engine/tests/eval_exit_enforcement.rs`, `crates/xvision-engine/tests/strategy_roundtrip.rs`, frontend authoring risk tests, and migration helpers that construct eval DBs for these tests.
- Do not modify `crates/xvision-mcp/src/tools.rs` in this batch: `team/OWNERSHIP.md` lists it as held by `indicator-tool-wiring`. The bracket design must leave the existing external `DecisionRow { ... }` constructor valid; record the stale MCP risk-shape comment as a blocked follow-up in `QA_TRACKER.md` while verifying the functional MCP path still deserializes explicit `RiskConfig` through `serde_json::from_value::<RiskConfig>`.

## Work Units

### Task 1: PF-19 RED Tests

**Files:**
- Modify `crates/xvision-engine/tests/eval_store.rs`
- Modify `crates/xvision-engine/src/eval/export.rs`
- Modify `crates/xvision-engine/tests/eval_exit_enforcement.rs`

- [ ] Add an `eval_store` test that records a `DecisionRow` with companion `DecisionBrackets` and expects `read_decision_brackets()` to return the same values keyed by `decision_index`.
- [ ] Add an eval export test that expects decision JSON to include bracket fields when present.
- [ ] Add an executor integration assertion that a trader-emitted `long_open` bracket persists on the `long_open` decision row.
- [ ] Run RED commands:
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-engine --test eval_store record_decision_persists_bracket_fields -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-engine --lib export::tests::decision_export_includes_bracket_fields_when_present -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-engine --test eval_exit_enforcement emitted_bracket_fields_are_persisted_on_decision_rows -- --nocapture`
  Expected: fail because store/export/executor do not yet carry companion bracket fields.

### Task 2: PF-19 Implementation

**Files:**
- Create `crates/xvision-engine/migrations/065_eval_decision_brackets.sql`
- Modify `crates/xvision-engine/src/api/mod.rs`
- Modify `crates/xvision-engine/src/eval/store.rs`
- Modify `crates/xvision-engine/src/eval/export.rs`
- Modify `crates/xvision-engine/src/api/eval.rs`
- Modify `crates/xvision-engine/src/api/chart.rs`
- Modify `crates/xvision-engine/src/eval/executor/backtest.rs`
- Modify generated `frontend/web/src/api/types.gen/DecisionRowDto.ts` and any generated index/output files touched by the repo's ts-export flow
- Modify test migration helpers that build eval schemas

- [ ] Add nullable columns: `stop_loss_pct`, `take_profit_pct`, `trailing_stop_pct`, `breakeven_trigger_pct`, `breakeven_offset_pct`, `fade_sl_bars`, `fade_sl_start_pct`, `fade_sl_end_pct`, `max_bars_held`, `sl_atr_mult`, `tp_atr_mult`, `tp1_pct`, `tp1_close_fraction`, `tp2_pct`.
- [ ] Add a `DecisionBrackets` companion type carrying matching `Option<f64>` fields plus `Option<u32>` for bar counts; keep `DecisionRow` unchanged so external constructors remain valid.
- [ ] Add a write path such as `record_decision_with_brackets(&DecisionRow, &DecisionBrackets)` that writes the normal row plus companion column values, while existing `record_decision(&DecisionRow)` writes null bracket columns for non-bracket callers.
- [ ] Add a read path such as `read_decision_brackets(run_id)` returning bracket values keyed by `decision_index`; do not change `read_decisions()` callers that only need the existing row shape.
- [ ] Update export/API/chart DTO assembly to join companion bracket values by `decision_index` and expose them with serde option behavior matching existing optional decision fields.
- [ ] Regenerate or update generated frontend DTOs so frontend imports of `DecisionRowDto` see the bracket fields.
- [ ] Populate companion bracket fields from parsed trader output when persisting model decision rows; deterministic forced close rows can leave them `None`.
- [ ] Add chart emission wiring that includes the companion bracket payload for model decision rows instead of relying on `LiveDecisionRow::from(&DecisionRow)` alone.
- [ ] Run Task 1 GREEN commands.

### Task 3: PF-20 RED Tests

**Files:**
- Modify `crates/xvision-engine/tests/strategy_roundtrip.rs`
- Modify `crates/xvision-engine/tests/eval_exit_enforcement.rs`

- [ ] Add a legacy-strategy roundtrip test where `risk.take_profit_atr_multiple` is absent and deserializes to `0.0` without resetting existing risk fields.
- [ ] Add a validation test that negative `risk.take_profit_atr_multiple` is rejected.
- [ ] Add an executor integration test proving a strategy-level `take_profit_atr_multiple` creates a deterministic `take_profit` close when the model emits no take-profit bracket.
- [ ] Run RED commands:
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-engine --test strategy_roundtrip risk_take_profit_atr_multiple_defaults_for_legacy_json -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-engine --test strategy_roundtrip negative_take_profit_atr_multiple_is_invalid -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-engine --test eval_exit_enforcement configured_atr_take_profit_runs_when_model_emits_no_tp -- --nocapture`
  Expected: fail because the config field and fallback do not exist.

### Task 4: PF-20 Implementation

**Files:**
- Modify `crates/xvision-engine/src/strategies/risk.rs`
- Modify `crates/xvision-engine/src/strategies/validate.rs`
- Modify `crates/xvision-engine/src/eval/executor/backtest.rs`
- Modify `crates/xvision-engine/src/eval/executor/sltp.rs`
- Modify `frontend/web/src/api/strategies.ts`
- Modify `frontend/web/src/routes/authoring.tsx`
- Modify `frontend/web/src/routes/authoring-risk.test.tsx`

- [ ] Add `RiskConfig.take_profit_atr_multiple` with serde default `0.0`.
- [ ] Set presets to `0.0` to avoid silently changing existing strategy behavior.
- [ ] Reject negative take-profit ATR multiples in `validate_strategy()`.
- [ ] In backtest entry risk-state wiring, use `parsed.tp_atr_mult` first; otherwise use `strategy.risk.take_profit_atr_multiple` only when `take_profit_pct <= 0.0` and the config value is positive.
- [ ] Compute entry ATR when either effective SL or effective TP ATR fallback is active.
- [ ] Update SL/TP trigger evaluation so ATR TP can fire from `tp_atr_mult` even when percent `take_profit_pct` is `0.0`.
- [ ] Update dashboard `RiskConfig`, form state, dirty check, validation, save payload, and tests so saving risk does not reset `take_profit_atr_multiple` to the default.
- [ ] Verify MCP remains functionally compatible by noting that `xvn_set_risk_config` deserializes `explicit` JSON into engine `RiskConfig`; do not edit its stale comment/schema text in this batch because the file is actively owned by another track.
- [ ] Run Task 3 GREEN commands.

### Task 5: Review, Verification, And Tracker Closeout

**Files:**
- Modify `QA_TRACKER.md`

- [ ] Run focused regression commands for PF-19/PF-20 plus `rustfmt --check` and `git diff --check`.
- [ ] Run focused frontend authoring risk tests if frontend files change.
- [ ] Run read-only adversarial implementation review against PF-19/PF-20; fix P0/P1 findings and repeat review if needed.
- [ ] Update `QA_TRACKER.md` with implementation evidence, wiring proof, verification commands, review result, MCP follow-up/blocker note for `crates/xvision-mcp/src/tools.rs`, and branch/PR status.
- [ ] Commit only scoped files on `qa/batch-c-brackets-20260612`.
- [ ] Push branch and open a stacked PR against `qa/release-manager-20260612` unless PR #949 has merged first; if #949 merges, rebase/retarget to `main`.

## Safety Notes

- Do not modify `/Users/edkennedy/Code/xvision/.worktrees/qa-parity-crosswalk-20260612`.
- Do not modify `docs/research/2026-06-12-live-eval-parity-crosswalk.md` or `docs/research/2026-06-12-qa-tracker-parity-import.md`.
- Use `scripts/cargo`, not bare `cargo`.
- Keep generated/rustfmt churn out of commits unless caused by touched files.
