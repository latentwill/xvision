# QA PF-01 Run Inspect Accounting Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for Tasks 1 and 3, then `superpowers:verification-before-completion` before publication. Keep implementation isolated in `.worktrees/qa-pf01-run-inspect-accounting-20260612` or a successor worktree. Do not edit the parity-crosswalk worktree or PF-11 worktree.

**Goal:** Fix `PF-01` so `xvn run inspect <run_id>` no longer emits stale `xvn_run.json` accounting for completed eval/backtest runs. The export/report must reconcile `agent_runs` with the linked `eval_runs` row, surface terminal eval status when the sidecar row is stale, and show token totals from the authoritative eval accounting path when model-call detail rows are missing.

**Architecture:** Keep `run inspect` wired to the observability export path, but teach that export path to enrich linked eval runs from the same SQLite database. `crates/xvision-observability` cannot depend on `xvision-engine`, so the enrichment uses small read-only SQL helpers against `eval_runs`, `agent_runs`, `spans`, and `model_calls`, mirroring the existing engine report join without importing engine APIs. Because `xvn.agent_run.v1` is explicitly pinned against shape mutation, add a v2 export shape by extending `AgentRunExport` with an `accounting` provenance object and bumping `SCHEMA_VERSION` to `xvn.agent_run.v2`. Preserve every current v1 field and field order as much as possible.

**Tech Stack:** Rust, SQLite/sqlx, `xvision-observability`, `xvision-cli` integration tests, existing `scripts/cargo` wrapper.

---

## Plan Review Gate Status

**Status:** IMPLEMENTED AND LOCALLY VERIFIED. The plan review gate exhausted 3/3 iterations. On 2026-06-13 the release manager approved proceeding after incorporating the two remaining P1 blockers into this plan and tracker: frontend/UI v2 operator-surface coverage and explicit `finished_at` reconciliation assertions.

| Iteration | Feasibility | Completeness | Scope & Alignment |
|---|---|---|---|
| 1 | FAIL | FAIL | FAIL |
| 2 | PASS | FAIL | PASS |
| 3 | PASS | FAIL | PASS |

Resolved P1 blockers from iteration 3:

- UI/operator surface wiring is now in scope. The implementation must update `frontend/web/src/api/agent-runs.ts` to accept `xvn.agent_run.v2`, add/extend `frontend/web/src/api/agent-runs.test.ts`, and surface reconciled v2 accounting through the normalized run detail summary so the UI path no longer rejects v2 exports.
- `finished_at` reconciliation is now explicitly verified. The stale-sidecar, stdout JSON, direct-eval, failed/cancelled override, and non-downgrade tests must assert the expected top-level `finished_at`.

Implementation work was replayed onto current `main` in `.worktrees/pr-964-rebuild-pf01-impl` on branch `codex/pf01-run-inspect-accounting-main-20260614`, superseding the stale draft stack at PRs #959/#964.

## Implementation Closeout

PF-01 was implemented under Beads task `xvision-xth7` and later replayed onto current `main` as branch `codex/pf01-run-inspect-accounting-main-20260614`.

Evidence:

- Backend export: `AgentRunExport` now emits `xvn.agent_run.v2` with `accounting` provenance; `build_export` reconciles linked/direct `eval_runs` accounting, terminal eval status, `finished_at`, and token/cost totals.
- CLI wiring: `xvn run inspect` file JSON, stdout JSON, and markdown all route through the same enriched `build_export`/`build_report` path.
- Dashboard wiring: `GET /api/agent-runs/:id`, `/export.json`, `/export.md`, and SSE snapshots still serve `AgentRunExport` from `build_export`.
- Frontend wiring: `validateAgentRunDetail` accepts exact v1/v2 export schema strings and normalizes v2 accounting onto the detail summary while preserving v1 compatibility.
- Active design docs: `docs/design/ce-plan.md` and `docs/design/notes-live-status.md` now reference the v2 export.
- Verification: `scripts/cargo test -p xvision-cli --test run_inspect -- --nocapture` passed 13/13; `scripts/cargo test -p xvision-observability --test export_schema -- --nocapture` passed 2/2; `cd frontend/web && npm test -- agent-runs.test.ts` passed 34/34; `rustfmt --check` passed on touched Rust files; `git diff --check` passed.
- Adversarial review: read-only `codex review --uncommitted` found no actionable correctness issues after independently rerunning the focused Rust/frontend checks.

The unchecked task list below is retained as the original execution plan; optional legacy paper-mode coverage was not added because current live parity is covered by an explicit `mode = 'live'` fixture.

---

## File Structure

- Modify `QA_TRACKER.md`: PF-01 checkpoints, branch/worktree ownership, parity evidence, final implementation evidence, wiring proof, verification, and review result.
- Modify `crates/xvision-observability/src/export.rs`: eval accounting enrichment, v2 schema tag, report provenance line, focused unit coverage if useful.
- Modify `crates/xvision-observability/tests/export_schema.rs`: update the pinned schema drift test for the intentional v2 bump and assert v1 field preservation.
- Modify `crates/xvision-observability/tests/fixtures/xvn_run_v1.golden.json` by replacing it with a v2 fixture or adding sibling `xvn_run_v2.golden.json`; keep explicit schema drift coverage.
- Modify `crates/xvision-cli/tests/run_inspect.rs`: integration regression tests that seed `eval_runs` plus stale or incomplete sidecar rows and invoke `xvn run inspect`.
- Modify `crates/xvision-cli/src/commands/run/inspect.rs` only if CLI-level behavior or help text needs to mention eval-linked accounting; otherwise leave command dispatch unchanged.
- Modify `crates/xvision-dashboard/src/routes/agent_runs.rs`: update v2 route/SSE comments to match the shared export schema.
- Modify `frontend/web/src/api/agent-runs.ts`: accept v2 exports and normalize the new `accounting` object into operator-visible run detail fields.
- Modify `frontend/web/src/api/agent-runs.test.ts`: assert v2 exports normalize successfully and expose reconciled status, `finished_at`, token totals, and accounting provenance.
- Modify `frontend/web/src/api/types-agent-runs.ts` only if the normalized UI type needs an accounting field.
- Modify `docs/design/ce-plan.md` and `docs/design/notes-live-status.md`: update active schema references from v1 to v2 after implementation review surfaced the stale references.
- Do not modify `crates/xvision-engine/src/api/eval.rs`, `crates/xvision-engine/src/eval/executor/backtest.rs`, or other actively owned engine execution/API files for PF-01.

## Live/Eval Parity Plan

- **Backtest path:** Backtest eval runs persist status and token counters in `eval_runs` (`mode = 'backtest'`, `status`, `completed_at`, `actual_input_tokens`, `actual_output_tokens`) and may link sidecar rows through `agent_runs.eval_run_id`.
- **Live path:** Live eval runs use the same `eval_runs` table shape (`mode = 'live'` for current writes) and the same `agent_runs.eval_run_id` linkage. `mode = 'paper'` is only a legacy read alias and is not sufficient parity evidence. PF-01 must not special-case backtest mode; enrichment must work for both `backtest` and `live` eval modes.
- **Evidence path:** `xvn_run.json.accounting`, top-level `status`, `finished_at`, and `totals` provide the common post-run comparison surface. The detail `model_calls` array remains the inspected agent-run detail list; `accounting.source` states whether totals came from agent model-call detail rows, eval-linked model-call aggregation, eval actual counters, or no signal.
- **Operator surface:** `xvn run inspect` file JSON, stdout JSON (`--out - --format json`), `xvn_report.md`, and the dashboard/frontend agent-run API normalizer must all accept/show the reconciled status/accounting source. Missing/legacy/no-signal states must render as `accounting.source = "none"` or nullable accounting fields, never as unexplained zero.
- **Parity tests:** Add one stale-sidecar test for `mode = 'backtest'` and one equivalent stale-sidecar test for `mode = 'live'`. Both must assert the same accounting/status reconciliation behavior. Add optional legacy coverage for `mode = 'paper'` only as a backward-compatibility case, not as the current live parity proof. If true live-loop persistence later diverges from the shared `eval_runs`/`agent_runs.eval_run_id` path, that divergence becomes a follow-up item owned by the live executor track; PF-01 must still persist and surface the `mode`/source evidence so the omission is explicit.

## Work Units

### Task 1: RED Tests For Stale Sidecar Accounting And Parity

**Files:**
- Modify `crates/xvision-cli/tests/run_inspect.rs`

- [ ] Add a helper that applies migrations `002_eval.sql`, `013_cli_jobs.sql`, and `018_agent_run_observability.sql`, then inserts an `eval_runs` row with configurable `mode`, terminal `status`, and non-zero `actual_input_tokens` / `actual_output_tokens`.
- [ ] Add a stale sidecar fixture: `agent_runs.id = run_id`, `agent_runs.eval_run_id = eval_run_id`, `agent_runs.status = 'running'`, no `model_calls`, and `eval_runs.status = 'completed'`.
- [ ] Add `inspect_reconciles_completed_eval_accounting_when_sidecar_is_stale`.
- [ ] Assert `xvn run inspect <run_id> --db <db> --out <dir>` succeeds and writes `xvn_run.json` with:
  - `schema_version = "xvn.agent_run.v2"`;
  - top-level `status = "completed"`;
  - top-level `finished_at = eval_runs.completed_at`;
  - `eval_run_id = eval_run_id`;
  - `totals.input_tokens` and `totals.output_tokens` from `eval_runs.actual_*_tokens`;
  - `totals.model_calls = 0`;
  - `accounting.source = "eval_actuals"`;
  - `accounting.eval_status = "completed"`;
  - `accounting.eval_mode = "backtest"`.
- [ ] Assert `xvn_report.md` contains `Status: completed`, `Finished at: <eval completed_at>`, `Eval run: <eval_run_id>`, and an accounting provenance line naming `eval_actuals`.
- [ ] Add `inspect_reconciles_live_eval_accounting_when_sidecar_is_stale` using the same fixture with `eval_runs.mode = 'live'`; assert it matches the backtest behavior and exposes `accounting.eval_mode = "live"` plus the eval `completed_at` as top-level `finished_at`.
- [ ] Optionally add `inspect_reconciles_legacy_paper_eval_accounting_when_sidecar_is_stale` for old DB rows, but do not use this as the live parity proof.
- [ ] Add `inspect_stdout_json_reconciles_eval_accounting` using `--out - --format json` against the stale sidecar fixture; assert stdout JSON has the same v2 `accounting`, `status`, `finished_at`, and token totals as file JSON.
- [ ] Run RED:
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-cli --test run_inspect inspect_reconciles_completed_eval_accounting_when_sidecar_is_stale -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-cli --test run_inspect inspect_reconciles_live_eval_accounting_when_sidecar_is_stale -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-cli --test run_inspect inspect_stdout_json_reconciles_eval_accounting -- --nocapture`
  Expected: fail because the current export is v1, keeps `status=running`, and computes zero token totals from empty `model_calls`.

### Task 2: Implement Eval Accounting Enrichment And Schema Bump

**Files:**
- Modify `crates/xvision-observability/src/export.rs`
- Modify `crates/xvision-observability/tests/export_schema.rs`
- Modify `crates/xvision-observability/tests/fixtures/xvn_run_v1.golden.json` or create `crates/xvision-observability/tests/fixtures/xvn_run_v2.golden.json`

- [ ] Add an `ExportAccounting` struct with stable fields:
  - `source: String` where values are `agent_model_calls`, `eval_model_calls`, `eval_actuals`, or `none`;
  - `eval_run_id: Option<String>`;
  - `eval_mode: Option<String>`;
  - `eval_status: Option<String>`;
  - `eval_actual_input_tokens: Option<u64>`;
  - `eval_actual_output_tokens: Option<u64>`;
  - `eval_model_calls: u64`;
  - `eval_model_call_input_tokens: Option<u64>`;
  - `eval_model_call_output_tokens: Option<u64>`;
  - `eval_model_call_cost_usd: Option<f64>`.
- [ ] Add `accounting: ExportAccounting` to `AgentRunExport` and bump `SCHEMA_VERSION` from `xvn.agent_run.v1` to `xvn.agent_run.v2`.
- [ ] Keep all existing v1 fields present. Existing `model_calls` remains the detail list for the inspected agent run; do not synthesize fake model-call rows.
- [ ] Add `load_eval_accounting(pool, agent_run_id, run.eval_run_id.as_deref())`:
  - identify the eval run via `agent_runs.eval_run_id` first;
  - if absent, allow `eval_runs.id = agent_run_id` as a direct-id fallback;
  - query `eval_runs.mode`, `status`, `completed_at`, `actual_input_tokens`, and `actual_output_tokens`;
  - aggregate linked model calls using `eval_runs.id -> agent_runs.eval_run_id -> spans.run_id -> model_calls.span_id`.
- [ ] Reconcile top-level status:
  - if the linked eval row is terminal (`completed`, `failed`, or `cancelled`) and the sidecar row is non-terminal (`queued` or `running`), use the eval status and `completed_at` as top-level status/finished timestamp;
  - never let a non-terminal eval row downgrade a terminal sidecar row.
- [ ] Reconcile totals:
  - compute existing agent detail totals first;
  - if linked eval model-call aggregation has rows, use those aggregate token/cost totals and `source = "eval_model_calls"`;
  - otherwise, if `eval_runs.actual_*_tokens` are present and non-zero, use those token totals and `source = "eval_actuals"` while leaving `totals.model_calls = 0`;
  - otherwise preserve the existing agent detail totals with `source = "agent_model_calls"` when detail model calls exist or `source = "none"` when all sources are empty.
- [ ] Render the accounting source, eval mode, and eval status in `xvn_report.md` near the existing Eval run line.
- [ ] Keep all enrichment queries read-only and best-effort: SQL errors from missing pre-eval tables should not break pure `agent_runs` exports that predate eval tables.
- [ ] Update the export schema golden test/fixture for v2; assert that all former v1 top-level keys remain present and that `accounting` is the intentional new top-level field.
- [ ] Run Task 1 GREEN commands.
- [ ] Run the schema drift test:
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-observability --test export_schema -- --nocapture`

### Task 3: RED Tests For Direct Eval, Legacy DBs, Status Precedence, And Detail Preservation

**Files:**
- Modify `crates/xvision-cli/tests/run_inspect.rs`

- [ ] Add `inspect_preserves_agent_model_call_details_when_eval_is_linked`:
  - seed a linked eval row and an agent run with one span plus one `model_calls` row;
  - assert `model_calls` array still contains the provider/model row;
  - assert `accounting.source = "eval_model_calls"` when aggregate rows exist;
  - assert totals match the aggregate linked model calls.
- [ ] Add `inspect_direct_eval_run_id_without_sidecar_uses_eval_projection`:
  - seed only `eval_runs`;
  - assert `xvn run inspect <eval_run_id>` emits a minimal v2 export with eval status, eval mode, eval `completed_at` as top-level `finished_at`, and actual token totals.
- [ ] Add `inspect_legacy_agent_db_without_eval_table_still_exports`:
  - create a DB with `013_cli_jobs.sql` and `018_agent_run_observability.sql` but without `002_eval.sql`;
  - seed a pure `agent_runs` row with no `eval_run_id`;
  - assert `xvn run inspect` succeeds, emits v2 with legacy agent fields preserved, and sets `accounting.source = "none"` instead of failing with `no such table: eval_runs`.
- [ ] Add status-precedence tests:
  - `inspect_uses_failed_eval_status_when_sidecar_is_running`, including failed eval `completed_at` as top-level `finished_at`;
  - `inspect_uses_cancelled_eval_status_when_sidecar_is_running`, including cancelled eval `completed_at` as top-level `finished_at`;
  - `inspect_nonterminal_eval_does_not_downgrade_completed_sidecar`, including preservation of the completed sidecar `finished_at`.
- [ ] Add a legacy pure-agent assertion to the existing smoke test:
  - current non-eval `agent_runs` fixtures should now emit `schema_version = "xvn.agent_run.v2"`;
  - `accounting.source = "none"` or `agent_model_calls` according to the fixture.
- [ ] Run RED before Task 4:
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-cli --test run_inspect inspect_preserves_agent_model_call_details_when_eval_is_linked -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-cli --test run_inspect inspect_direct_eval_run_id_without_sidecar_uses_eval_projection -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-cli --test run_inspect inspect_legacy_agent_db_without_eval_table_still_exports -- --nocapture`
  - focused commands for each status-precedence test above.

### Task 4: Complete Direct Eval Fallback And CLI Wiring Audit

**Files:**
- Modify `crates/xvision-observability/src/export.rs`
- Modify `crates/xvision-cli/src/commands/run/inspect.rs` only if needed

- [ ] Add a helper that builds a minimal `AgentRunRow` projection when `agent_runs.id = run_id` is absent but `eval_runs.id = run_id` exists:
  - `run_id = eval_runs.id`;
  - `objective = "Eval run <id>"`;
  - `eval_run_id = Some(id)`;
  - `status = eval_runs.status`;
  - `started_at = eval_runs.started_at`;
  - `finished_at = eval_runs.completed_at`;
  - `retention_mode = "hash_only"`;
  - optional sidecar/protocol fields remain `None`.
- [ ] Ensure span/model/tool loaders tolerate the direct eval projection by returning empty arrays for missing agent detail rows.
- [ ] Audit `run inspect --out - --format json`, file output, and markdown output all call the same enriched `build_export` result.
- [ ] Keep the trajectory-mode stderr probe best-effort; direct eval IDs may skip that line.
- [ ] Run Task 3 GREEN commands.

### Task 4b: Frontend V2 Export Compatibility

**Files:**
- Modify `frontend/web/src/api/agent-runs.ts`
- Modify `frontend/web/src/api/agent-runs.test.ts`
- Modify `frontend/web/src/api/types-agent-runs.ts` only if needed

- [ ] Add RED test coverage that `validateAgentRunDetail` accepts a backend export with `schema_version = "xvn.agent_run.v2"` and an `accounting` object.
- [ ] Assert the normalized detail summary preserves reconciled `status`, `finished_at`, token totals, and exposes accounting provenance to operator UI code.
- [ ] Update the export-shape guard to accept v1 and v2. Do not loosen it to arbitrary strings.
- [ ] Add a nullable/optional accounting field to the normalized type if needed, keeping v1 exports valid with no accounting object.
- [ ] Run RED then GREEN:
  - `cd frontend/web && npm test -- agent-runs.test.ts --runInBand` or the repo's equivalent focused test command.
  - If the package uses Vitest directly, run the existing focused Vitest command for `frontend/web/src/api/agent-runs.test.ts`.

### Task 5: Verification, Adversarial Review, And Tracker Closeout

**Files:**
- Modify `QA_TRACKER.md`

- [ ] Run focused regression commands:
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-cli --test run_inspect -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-observability --test export_schema -- --nocapture`
  - `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"; scripts/cargo test -p xvision-observability export::tests -- --nocapture` if observability unit tests are added.
  - focused frontend API test command for `frontend/web/src/api/agent-runs.test.ts`.
- [ ] Run hygiene:
  - `git diff --check`
  - `rustfmt --check crates/xvision-observability/src/export.rs crates/xvision-observability/tests/export_schema.rs crates/xvision-cli/tests/run_inspect.rs crates/xvision-cli/src/commands/run/inspect.rs`
- [ ] Perform a wiring audit and record it in `QA_TRACKER.md`: `xvn run inspect` command -> `build_export` -> eval enrichment -> file JSON, stdout JSON, markdown report, dashboard `/api/agent-runs/:id`, dashboard `/api/agent-runs/:id/export.json`, SSE snapshot, and frontend `validateAgentRunDetail`/normalization.
- [ ] Record live/eval parity evidence in `QA_TRACKER.md`: backtest fixture, live-mode fixture, common `accounting` evidence path, operator surfaces, and any explicit live executor dependency if a real live-loop path remains outside this batch.
- [ ] Run read-only adversarial implementation review. Required prompt checks:
  - schema-version compatibility and v1 field preservation;
  - status precedence between stale sidecar and terminal eval rows;
  - token source precedence across agent details, eval model-call aggregation, and eval actual counters;
  - direct eval fallback behavior;
  - legacy DB behavior without `eval_runs`;
  - stdout/file/markdown wiring;
  - frontend v2 export acceptance and normalized accounting display;
  - live/eval parity evidence;
  - scope compliance with `team/OWNERSHIP.md`.
- [ ] Fix all P0/P1 findings and repeat review until PASS.
- [ ] Update PF-01 in `QA_TRACKER.md` with implementation evidence, wiring proof, verification command output, adversarial review result, parity result, and branch/PR status.
- [ ] Commit only scoped files on `qa/pf01-run-inspect-accounting-20260613`.
- [ ] Push and open a stacked PR against the latest QA stack head unless prior QA PRs have merged; if they have merged, rebase/retarget to `main`.

## Safety Notes

- Do not modify `/Users/edkennedy/Code/xvision/.worktrees/qa-parity-crosswalk-20260612`.
- Do not modify `/Users/edkennedy/Code/xvision/.worktrees/qa-pf11-bars-filters-20260612`.
- Do not touch currently owned engine execution/API files for PF-01 unless a separate ownership contract is approved.
- Use `scripts/cargo`, not bare `cargo`, and set `CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"` for cargo commands.
- Keep rustfmt/generated churn out of commits unless caused by scoped files.
