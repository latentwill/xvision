# Watcher v1 — Implementation Plan

> Companion to `docs/superpowers/specs/2026-05-21-watcher-v1-shape.md`.
> The spec is normative for shape and acceptance; this plan is normative
> for ordering, slicing, and PR-sized stages. If the two disagree, the
> spec wins and this plan is updated.

## Stage ordering

Stages are sliced so each one merges independently and leaves `main`
green. Stages 1–4 are the v1 ship surface. Stage 5 is the regression
guard work, kept last because it has to land before we cut over the
default surface in the editor.

```
Stage 1: domain + validation        (no behavior change yet)
Stage 2: backtest evaluator         (engine-only; gated by activation_mode)
Stage 3: export + summary           (visible in CLI export, no UI yet)
Stage 4: frontend types + panels    (read-only in v1)
Stage 5: regression + fixtures      (locks the v1 contract)
```

Each stage is one PR. No stacking. Rebase on `origin/main` before each.

---

## Stage 1 — Domain types + validation

**Branch:** `task/watcher-v1-shape` (this branch).

**Goal:** Land the new types and the validator. No runtime wiring yet.
After this stage, a strategy can be saved with `activation_mode =
WatcherGated` and a `Watcher`, and the save API rejects invalid
watchers — but the backtest still runs every-bar regardless.

**Files added:**
- `crates/xvision-engine/src/strategies/watcher.rs`
  - `Watcher`, `WatcherConditions`, `Condition`, `Operand`, `Op`,
    `IndicatorRef`, `InPositionPolicy`, `ScanCadence`, `ActivationMode`.
  - All `#[derive(Serialize, Deserialize)]` with `#[serde(rename_all =
    "snake_case")]`. `ts-export` feature gate matches existing
    `agent_ref.rs` pattern.
- `crates/xvision-engine/src/strategies/watcher_validate.rs`
  - `pub fn validate(s: &Strategy) -> Result<(), Vec<WatcherValidationError>>`.
  - One error variant per spec rule (1–10), each with a stable code
    string (`watcher.schema_version`, `watcher.op_range_required`, ...).
- `crates/xvision-engine/migrations/032_strategy_watcher.sql`
  - `ALTER TABLE strategies ADD COLUMN activation_mode TEXT NOT NULL
    DEFAULT 'every_bar';`
  - `ALTER TABLE strategies ADD COLUMN watcher_json TEXT;`
- `crates/xvision-engine/migrations/032_strategy_watcher.down.sql`
  - Drop both columns.

**Files edited:**
- `crates/xvision-engine/src/strategies/mod.rs`
  - Add `pub mod watcher;` and re-export the new types.
  - Add `activation_mode: ActivationMode` (serde default `EveryBar`) and
    `watcher: Option<Watcher>` (serde default `None`) to `Strategy`.
  - Update `Strategy::default()` and any constructor helpers; existing
    fixtures must serialize without the new fields and parse with
    defaults.
- `crates/xvision-engine/src/strategies/validate.rs`
  - Call `watcher_validate::validate` from the existing top-level
    validator. Existing error type gets a new variant
    `WatcherInvalid(Vec<WatcherValidationError>)`.
- `crates/xvision-engine/src/eval/store.rs` (or wherever strategy rows
  are serialized to/from SQL) — write `activation_mode` and
  `watcher_json` on save, read on load. Keep backfill default to
  `every_bar`.

**Tests added:**
- `crates/xvision-engine/tests/watcher_validate.rs` — one case per
  validation rule (rules 1–10), plus a happy-path fixture.
- `crates/xvision-engine/tests/strategy_serde_compat.rs` — load
  existing strategy fixtures from `tests/fixtures/strategies/*.json`
  and assert they parse with defaults.

**Verification:**
- `cargo test -p xvision-engine --tests watcher_validate
  strategy_serde_compat`
- `cargo test --workspace`
- `cargo fmt --check && cargo clippy --workspace -- -D warnings`

**Done when:** all three commands green; a manual round-trip via
`xvn strategy save <fixture-with-watcher.json>` then
`xvn strategy show` returns the same JSON.

---

## Stage 2 — Backtest evaluator + per-bar wiring

**Branch:** `task/watcher-v1-runtime` (cut from `main` after Stage 1
merges).

**Goal:** Hook the validated watcher into the per-bar backtest loop.
After this stage, `activation_mode = WatcherGated` skips trader LLM
calls on non-fire bars. No export changes yet — observable only via
the call count.

**Files added:**
- `crates/xvision-engine/src/eval/watcher_eval.rs`
  - Pure evaluator. No I/O, no async, no state owned outside the call.
  - `pub fn evaluate(watcher: &Watcher, panel_t: &IndicatorPanel,
    panel_t_minus_1: Option<&IndicatorPanel>, state: &WatcherRuntimeState,
    position: PositionSnapshot) -> WatcherOutcome`
  - `pub struct WatcherRuntimeState { cooldown_remaining: u32,
    wakes_today: u32, current_utc_date: NaiveDate }`
  - Cross operators require `panel_t_minus_1.is_some()`; on `None`
    (i.e., bar 0 after warmup), cross operators evaluate as `false`.

**Files edited:**
- `crates/xvision-engine/src/eval/executor/backtest.rs`
  - Initialize `WatcherRuntimeState` at run start.
  - In the bar loop (`for (i, bar) in bars.iter().enumerate()` at line
    496), branch on `strategy.activation_mode`:
    - `EveryBar` → unchanged.
    - `WatcherGated` → compute `panel_t` and `panel_t_minus_1` (reuse
      the same `compute_panel_from_fixture` path, but on the live
      bar slice), call `watcher_eval::evaluate`, only call the trader
      on `WatcherOutcome::Fired`. Update cooldown/day-count state.
  - Day boundary: derive `current_utc_date` from `bar.timestamp`; reset
    `wakes_today` when it changes.
  - In-position policy resolution: read existing position-state snapshot
    pre-bar. For `OnInvalidationOrTargetOnly`, if the engine cannot
    resolve invalidation/target on the bar (no stop/target armed), the
    evaluator returns the collapse-to-suppress outcome described in the
    spec.

**Files touched (light):**
- `crates/xvision-engine/src/eval/executor/mod.rs` — `pub mod
  watcher_eval;` if needed for re-export.

**Tests added:**
- `crates/xvision-engine/tests/watcher_eval_unit.rs` — pure-function
  tests across all `Op` variants, cooldown, daily cap, in-position
  policies, and the cross-on-bar-0 edge case.
- `crates/xvision-engine/tests/watcher_gated_backtest.rs` — runs an
  existing scenario in `EveryBar` and `WatcherGated` modes, asserts:
  - `WatcherGated` produced strictly fewer `eval_decisions` rows.
  - Same scenario in `EveryBar` after Stage 2 produces identical
    `eval_decisions` to a stored golden file from before Stage 2
    (regression guard — golden file is committed in Stage 1).

**Verification:**
- `cargo test -p xvision-engine --tests watcher_eval_unit
  watcher_gated_backtest`
- `cargo test --workspace`

**Done when:** acceptance criteria #4 (every-bar regression) and #6
(decision count = fired) hold in CI.

---

## Stage 3 — Export + run summary

**Branch:** `task/watcher-v1-export`.

**Goal:** Emit `WatcherEventV1` rows and the `watcher_summary` block.
After this stage, `xvn eval export <run>` includes watcher data; the
API surfaces `watcher_summary` on run detail. No frontend yet.

**Files edited:**
- `crates/xvision-engine/src/eval/executor/backtest.rs`
  - Accumulate `WatcherEventV1` rows in the bar loop alongside the
    existing decision/execution accumulators.
- `crates/xvision-engine/src/eval/export.rs`
  - Add `watcher_events: Vec<WatcherEventV1>` field on the export
    payload. Add `watcher_summary` derivation.
  - Both default to empty/zero for `EveryBar` runs so the schema is
    stable.
- `crates/xvision-engine/src/api/eval.rs`
  - Surface `watcher_summary` on the run-detail response.
- `crates/xvision-engine/src/eval/store.rs`
  - Persist `watcher_events` to a new `eval_watcher_events` table.
- `crates/xvision-engine/migrations/033_eval_watcher_events.sql` (+ down)
  - `CREATE TABLE eval_watcher_events ( run_id TEXT, decision_index
    INTEGER, ts TEXT, bar_ref TEXT, outcome_json TEXT, conditions_json
    TEXT, PRIMARY KEY (run_id, decision_index));`
  - Add index on `run_id`.

**Tests added:**
- `crates/xvision-engine/tests/watcher_export.rs` — schema-snapshot
  test on a `WatcherGated` run; assert determinism by running twice
  and `assert_eq!`-ing the serialized export.
- `crates/xvision-engine/tests/watcher_summary.rs` — invariant
  `fired + suppressed.sum + no_match = scanned_bars`.

**Verification:**
- `cargo test -p xvision-engine --tests watcher_export watcher_summary`
- `cargo test --workspace`

**Done when:** acceptance criteria #5, #6, #7 hold.

---

## Stage 4 — Frontend types + read-only panels

**Branch:** `task/watcher-v1-frontend`.

**Goal:** Surface watchers in the dashboard. Read-only in v1:
- Strategy editor shows a "Watcher" pane with condition cards + a
  JSON edit textarea. Saving routes through the existing strategy save
  endpoint and surfaces validation errors inline.
- Run review shows a `WatcherEventsPanel` listing rows and the summary.

**Files added:**
- `frontend/web/src/api/types.gen/Watcher.ts` and the rest of the new
  ts-rs exports. Generated by `cargo run -p xvision-engine --bin
  generate-ts-types` (existing path).
- `frontend/web/src/components/strategy/WatcherCard.tsx`
  - Reads `Strategy.watcher`. Renders condition cards. JSON pane for
    edits. No NL → DSL in v1.
- `frontend/web/src/features/eval-runs/review/WatcherEventsPanel.tsx`
  - Lists `WatcherEventV1` rows. Top summary block from
    `watcher_summary`. Filter chips for outcome variants.
- `frontend/web/src/components/strategy/WatcherCard.test.tsx` and
  `WatcherEventsPanel.test.tsx`.

**Files edited:**
- `frontend/web/src/api/types.gen/types.gen.ts` — re-export new types.
- `frontend/web/src/features/strategies/StrategyEditor.tsx` (or
  current equivalent) — mount `WatcherCard` under a new tab/section
  when `activation_mode == 'watcher_gated'`.
- `frontend/web/src/features/eval-runs/review/RunDetail.tsx` (or
  current equivalent) — mount `WatcherEventsPanel` when the run has
  non-empty `watcher_summary`.

**Tests added:**
- Component tests above. No e2e in v1.

**Verification:**
- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web test --run`

**UI rule reminder:** xvision dashboards do not use popups. Watcher
authoring/edit happens inline in the strategy editor panel; validation
errors land as inline error rows, not modals.

**Done when:** acceptance criterion #9 holds; manual round-trip from
the editor saves a watcher, runs an eval, and the run-detail view
shows the events.

---

## Stage 5 — Regression locks + fixtures

**Branch:** `task/watcher-v1-fixtures`.

**Goal:** Commit the fixture watcher and the golden export so the v1
contract is locked.

**Files added:**
- `docs/superpowers/specs/fixtures/watcher-v1-pullback.toml` — the
  fixture referenced by acceptance criterion #5.
- `crates/xvision-engine/tests/fixtures/watcher_v1_pullback_export.json`
  — golden export for the fixture watcher on the BTC/USD 1h scenario.
- `crates/xvision-engine/tests/watcher_v1_acceptance.rs` — runs the
  fixture watcher against the scenario and diffs the export against
  the committed golden.

**Verification:**
- `cargo test -p xvision-engine --test watcher_v1_acceptance`
- `cargo test --workspace`

**Done when:** all ten acceptance criteria from the spec are CI-checked.

---

## Cross-stage rules

- **Forbidden in all stages:** changes to the trader briefing schema
  beyond the additive `wake` block; changes to `AgentRef`; new
  indicators; provider/model wiring.
- **DB safety:** migrations 032 and 033 are additive only. No `DROP`,
  no `ALTER ... NOT NULL` on existing columns.
- **Determinism gates:** every stage that touches the runtime adds at
  least one test that runs the same input twice and asserts byte-equal
  outputs (`assert_eq!` on the serialized payload).
- **CLAUDE.md guardrails:** all `cargo` work happens on the local build
  host, not on deploy hosts. No `target/` in temporary worktrees.

## Estimated PR sizes (rough)

| Stage | LOC engine | LOC frontend | New tests |
|---|---|---|---|
| 1 | ~600 | 0 | ~12 |
| 2 | ~400 | 0 | ~10 |
| 3 | ~250 | 0 | ~6 |
| 4 | 0 | ~700 | ~8 |
| 5 | ~50 (test) | 0 | ~2 |

Anything ballooning past 1.5× the rough estimate is a signal to slice
further rather than merge a big PR.

## Out-of-plan tracks that interact later

- `agent-graph-composition` (foundation) — will move watcher config
  from `Strategy.watcher` to a kind-watcher `AgentRef`. v1 leaves a
  serde shape compatible with that move: the on-disk JSON for a
  watcher is a self-contained object, so it can be lifted into an
  `Agent` record without schema change.
- `eval-token-efficiency` (leaf) — will fill in
  `watcher_summary.input_tokens_saved_estimate` once per-call token
  accounting is deterministic.
- `eval-honesty-smell-tests` (foundation) — may flag
  `watcher_summary.fired = 0` runs as suspicious; the field exists for
  that hook.

## References

- Spec: `docs/superpowers/specs/2026-05-21-watcher-v1-shape.md`
- Bar loop: `crates/xvision-engine/src/eval/executor/backtest.rs:496`
- Indicator panel: `crates/xvision-data/src/indicators.rs:288`
- Intake context: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
