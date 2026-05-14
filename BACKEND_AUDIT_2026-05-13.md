# Backend Audit - xvision

_Date: 2026-05-13 (UTC)_

## Scope & Method
- Scope: Rust backend code under `crates/`, `xtask/`, `probes/` (excluding frontend TS/Vite code).
- Worktree: `backend-audit-20260513`.
- Constraints: per `CLAUDE.md`, no local Rust build/test commands were run on this host.
- Failing-test evidence source: GitHub Actions logs (`gh run view`) for recent runs on/near `main`.

## Priority Findings

### P0
- **Silent fallback to `flat` on trader-output parse failure** in both backtest and paper executors. This converts malformed trader JSON into a valid `flat` action, masking model/protocol failures and biasing eval outcomes instead of failing fast. Evidence: `crates/xvision-engine/src/eval/executor/backtest.rs:351-355`, `crates/xvision-engine/src/eval/executor/paper.rs:226-230`. **Effort: M**
- **Incorrect `FillRecorded.side` for flat-closing trades in backtest**. Side is inferred _after_ mutating `position` and `entry_price`; in the close-to-flat branch `entry_price` has already been reset to `0.0`, so branch logic cannot distinguish closing long vs closing short and can emit wrong side semantics. Evidence: `crates/xvision-engine/src/eval/executor/backtest.rs:384-391` with prior mutation at `:367-368`. **Effort: S**

### P1
- **Paper executor position sizing uses hardcoded BTC reference price (`70_000`)** instead of live/reference market price. This distorts sizing accuracy as price drifts and is wrong for non-BTC scenarios. Evidence: `crates/xvision-engine/src/eval/executor/paper.rs:40`, `:238-239`. **Effort: M**
- **CI compile blocker observed in recent run** (`25770964170`): dashboard compile fails when embedded static folder is absent during preflight (`#[derive(RustEmbed)] folder .../crates/xvision-dashboard/static/ does not exist`), followed by missing `Assets::get`. Code touchpoints: `crates/xvision-dashboard/src/embed.rs:13-14`, `crates/xvision-dashboard/src/routes/static_files.rs:53`. This is a release-pipeline blocker even when backend code compiles. **Effort: S**
- **CI setup blocker observed in latest run** (`25771386134`): `actions/setup-node@v4` step errors with `Unable to locate executable file: pnpm`. This blocks preflight and prevents test/compile stages from running. **Effort: S**

### P2
- **Large concentration of oversized modules and long functions** increases defect surface and review cost (details below). Most critical concentration is in `api/settings/providers.rs`, `orderly.rs`, `api/chart.rs`, `api/eval.rs`, and eval executors. **Effort: L**
- **Repeated configuration/provider literals (3+ occurrences) are not centralized**, increasing drift risk between CLI/API/dashboard paths (details below). **Effort: M**

## Failing Tests / Checks Status
- Local test execution: **not run** (explicit host constraint: no Rust builds/tests on this box).
- GitHub Actions evidence:
  - Run `25771386134` (2026-05-13): failed in preflight Node setup (`pnpm` missing).
  - Run `25770964170` (2026-05-13): failed in Rust deploy compile check due missing dashboard static embed folder.
  - Result: current failures are **pipeline/build-blocker failures**, not confirmed unit-test assertion failures.

## Files Over 500 Lines (Backend Source)

```text
1362	crates/xvision-engine/src/api/settings/providers.rs
1347	crates/xvision-execution/src/orderly.rs
1219	crates/xvision-engine/src/api/chart.rs
1184	crates/xvision-engine/src/api/eval.rs
1108	crates/xvision-mcp/src/tools.rs
989	crates/xvision-execution/src/alpaca.rs
977	crates/xvision-eval/src/backtest.rs
959	crates/xvision-dashboard/src/wizard_loop.rs
953	crates/xvision-core/src/config.rs
819	crates/xvision-engine/src/api/strategy.rs
756	crates/xvision-engine/src/eval/executor/backtest.rs
628	crates/xvision-engine/src/api/settings/brokers.rs
615	crates/xvision-eval/src/harness.rs
613	crates/xvision-identity/src/client.rs
596	crates/xvision-engine/src/eval/store.rs
576	crates/xvision-core/src/trading.rs
566	crates/xvision-engine/src/authoring.rs
554	crates/xvision-eval/src/metrics.rs
540	crates/xvision-eval/src/ab_compare.rs
523	crates/xvision-cli/src/commands/scenario.rs
```

## Functions Over 80 Lines (Backend Source)

```text
crates/xvision-engine/src/eval/executor/backtest.rs:225:run_inner:318
crates/xvision-eval/src/harness.rs:131:run:214
crates/xvision-engine/src/eval/executor/paper.rs:147:run_inner:213
crates/xvision-eval/src/report.rs:49:render:186
crates/xvision-dashboard/src/cli_jobs/runner.rs:173:run_inner:172
crates/xvision-dashboard/src/wizard_loop.rs:421:wizard_tool_defs:153
crates/xvision-engine/src/agent/llm.rs:302:complete:150
crates/xvision-execution/src/orderly.rs:633:submit:143
crates/xvision-engine/src/api/settings/providers.rs:559:add_inner:139
crates/xvision-dashboard/src/server.rs:17:build_router:138
crates/xvision-cli/src/commands/ab_compare.rs:39:run:134
crates/xvision-engine/src/api/chart.rs:289:build_run_payload:133
crates/xvision-engine/src/api/eval.rs:649:run_inner:127
crates/xvision-eval/src/backtest.rs:282:tick:126
crates/xvision-dashboard/src/wizard_loop.rs:300:run_tool:116
crates/xvision-engine/src/eval/scenario.rs:363:canonical_scenarios:113
crates/xvision-engine/src/api/chart.rs:1094:build_scenario_preview:112
crates/xvision-dashboard/src/wizard_loop.rs:153:run_one_turn:107
crates/xvision-eval/src/bootstrap.rs:71:paired_bootstrap_sharpe_delta:107
crates/xvision-eval/src/backtest.rs:429:submit:106
crates/xvision-engine/src/agents/templates.rs:35:builtin_templates:105
crates/xvision-cli/src/lib.rs:218:run:95
crates/xvision-cli/src/commands/migrate.rs:54:run_dry:92
crates/xvision-dashboard/src/routes/eval_runs.rs:238:stream:92
crates/xvision-cli/src/commands/strategy.rs:134:run_inline:91
crates/xvision-dashboard/src/cli_jobs/store.rs:194:append_chunk:91
crates/xvision-dashboard/src/llm_dispatch.rs:31:resolve:89
crates/xvision-engine/src/api/chart.rs:800:build_strategy_payload:88
crates/xvision-engine/src/api/chart.rs:979:build_compare_payload:88
crates/xvision-engine/src/eval/executor/backtest.rs:574:simulate_fill:88
crates/xvision-eval/src/ab_compare.rs:65:parse_arm_spec:88
crates/xvision-engine/src/api/settings/providers.rs:752:set_enabled_models_inner:87
crates/xvision-execution/src/alpaca.rs:459:submit:86
crates/xvision-engine/src/agent/execute.rs:32:execute_slot:85
probes/m0-byreal/src/main.rs:70:main:85
probes/m0-orderly/src/main.rs:33:probe:85
crates/xvision-eval/src/metrics.rs:265:compute_regime_stratified:84
crates/xvision-engine/src/api/chart.rs:538:split_markers:82
crates/xvision-engine/src/agents/validate.rs:39:validate_agent:81
```

## Duplicated String Literals (3+ occurrences, selected actionable)

```text
21	openai-compat
17	https://api.anthropic.com
12	https://api.openai.com/v1
12	ANTHROPIC_API_KEY
9	sqlite://x.db
9	data/vectors
9	alpaca-historical-v1
8	config/default.toml
7	OPENAI_API_KEY
```

## Suggested Refactor Targets (order)
1. Split `crates/xvision-engine/src/api/settings/providers.rs` into parse/validation/persistence/service modules. **Effort: L**
2. Split `crates/xvision-engine/src/eval/executor/backtest.rs` into pipeline, fill-simulation, and event-emission submodules. **Effort: L**
3. Extract shared trader-output parse/validation logic used by paper + backtest executors to one strict parser returning typed errors. **Effort: M**
4. Introduce a shared constants module for provider URLs/env keys/config paths/data-source tags used across CLI/API/dashboard. **Effort: M**
