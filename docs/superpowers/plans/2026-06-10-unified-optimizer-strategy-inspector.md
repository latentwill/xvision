# Unified Optimizer + Strategy Inspector — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify `xvn optimize` and `xvn optimizer` into a single strategy-level optimizer CLI, eliminate the fake stub in `run_optimize()`, and add a Strategy Inspector screen to the Optimizer UI.

**Architecture:** The engine (`cycle.rs`, `mutator.rs`, `lineage.rs`, `blob_store.rs`) is untouched. The CLI migration moves `RunCycleArgs` / `run_cycle_cmd` / `run_mutate_once` from `autooptimizer.rs` into `optimize.rs`, which becomes the canonical optimizer CLI. `autooptimizer.rs` becomes a thin deprecation shim. Three new dashboard endpoints serve the Strategy Inspector screen.

**Tech Stack:** Rust (axum, sqlx, tokio), TypeScript/React (TanStack Query, React Router), Vitest, cargo test

**Parallel execution map:**
- **Wave 1 (parallel):** Task 1, Task 2, Task 3, Task 5 — all independent
- **Wave 2 (parallel, after Wave 1):** Task 4 (needs Task 3), Task 6 (needs Tasks 2 + 5)
- **Wave 3:** Task 7 (needs Tasks 3–6 complete)

---

## Task 1: Engine — `strategy_diff()` helper

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs`

- [ ] **Step 1: Add `StrategyDiff` type and `strategy_diff()` after `MutationDiff` (around line 168)**

  In `mutator.rs`, after the existing `MutationDiff` struct definition (around line 160), add:

  ```rust
  /// Structural diff between two strategy snapshots. Mirrors `MutationDiff`'s
  /// field shape but is computed by field comparison rather than LLM proposal.
  /// Used by the Strategy Inspector's "diff from originating strategy" panel.
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  pub struct StrategyDiff {
      pub prose: Vec<ProseEdit>,
      pub params: Vec<ParamChange>,
      pub tools: ToolDiff,
      pub filter: Vec<FilterEdit>,
  }

  /// Compute the structural diff between strategy `a` (before) and `b` (after).
  /// Returns the changes needed to transform `a` into `b`.
  pub fn strategy_diff(a: &Strategy, b: &Strategy) -> StrategyDiff {
      // Prose: compare AgentRef.prompt_override for matching roles
      let mut prose = Vec::new();
      for agent_b in &b.agents {
          let before = a.agents.iter()
              .find(|ag| ag.canonical_role() == agent_b.canonical_role())
              .and_then(|ag| ag.prompt_override.clone())
              .unwrap_or_default();
          let after = agent_b.prompt_override.clone().unwrap_or_default();
          if before != after {
              prose.push(ProseEdit {
                  agent_role: agent_b.role.clone(),
                  before,
                  after,
              });
          }
      }

      // Params: flatten both mechanical_params as JSON objects and diff keys
      let mut params = Vec::new();
      let empty_obj = serde_json::Value::Object(serde_json::Map::new());
      let params_a = a.mechanical_params.as_object().unwrap_or(empty_obj.as_object().unwrap());
      let params_b = b.mechanical_params.as_object().unwrap_or(empty_obj.as_object().unwrap());
      for (key, val_b) in params_b {
          let val_a = params_a.get(key).cloned().unwrap_or(serde_json::Value::Null);
          if &val_a != val_b {
              params.push(ParamChange {
                  key: key.clone(),
                  before: val_a,
                  after: val_b.clone(),
              });
          }
      }

      // Tools: compare manifest.required_tools
      let tools_a: std::collections::HashSet<_> = a.manifest.required_tools.iter().collect();
      let tools_b: std::collections::HashSet<_> = b.manifest.required_tools.iter().collect();
      let added: Vec<String> = tools_b.difference(&tools_a).map(|s| (*s).clone()).collect();
      let removed: Vec<String> = tools_a.difference(&tools_b).map(|s| (*s).clone()).collect();
      let tools = ToolDiff { added, removed };

      // Filter: flatten and diff numeric leaf values
      // Simple approach: serialize both filters to JSON and diff scalar values
      let mut filter_edits = Vec::new();
      if let (Some(fa), Some(fb)) = (&a.filter, &b.filter) {
          let ja = serde_json::to_value(fa).unwrap_or_default();
          let jb = serde_json::to_value(fb).unwrap_or_default();
          diff_filter_values(&ja, &jb, "", &mut filter_edits);
      }

      StrategyDiff { prose, params, tools, filter: filter_edits }
  }

  fn diff_filter_values(
      a: &serde_json::Value,
      b: &serde_json::Value,
      path: &str,
      out: &mut Vec<FilterEdit>,
  ) {
      match (a, b) {
          (serde_json::Value::Number(na), serde_json::Value::Number(nb)) => {
              if na != nb {
                  out.push(FilterEdit {
                      path: path.to_string(),
                      before: a.clone(),
                      after: b.clone(),
                  });
              }
          }
          (serde_json::Value::Object(ma), serde_json::Value::Object(mb)) => {
              for (k, vb) in mb {
                  let va = ma.get(k).unwrap_or(&serde_json::Value::Null);
                  let child_path = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                  diff_filter_values(va, vb, &child_path, out);
              }
          }
          (serde_json::Value::Array(aa), serde_json::Value::Array(ab)) => {
              for (i, (va, vb)) in aa.iter().zip(ab.iter()).enumerate() {
                  diff_filter_values(va, vb, &format!("{path}[{i}]"), out);
              }
          }
          _ => {}
      }
  }
  ```

- [ ] **Step 2: Write failing test at the bottom of `mutator.rs`** (inside `#[cfg(test)]` block)

  ```rust
  #[test]
  fn strategy_diff_detects_prose_change() {
      use crate::strategies::agent_ref::AgentRef;
      use crate::strategies::mod::Strategy;

      let mut a = Strategy::default();
      a.agents = vec![AgentRef {
          role: "trader".to_string(),
          agent_id: "agent-1".to_string(),
          prompt_override: Some("buy low".to_string()),
          ..Default::default()
      }];
      let mut b = a.clone();
      b.agents[0].prompt_override = Some("sell high".to_string());

      let diff = strategy_diff(&a, &b);
      assert_eq!(diff.prose.len(), 1);
      assert_eq!(diff.prose[0].before, "buy low");
      assert_eq!(diff.prose[0].after, "sell high");
      assert!(diff.params.is_empty());
      assert!(diff.tools.added.is_empty());
      assert!(diff.tools.removed.is_empty());
  }

  #[test]
  fn strategy_diff_detects_param_change() {
      use crate::strategies::mod::Strategy;

      let mut a = Strategy::default();
      a.mechanical_params = serde_json::json!({ "rsi_period": 14 });
      let mut b = a.clone();
      b.mechanical_params = serde_json::json!({ "rsi_period": 21 });

      let diff = strategy_diff(&a, &b);
      assert_eq!(diff.params.len(), 1);
      assert_eq!(diff.params[0].key, "rsi_period");
      assert_eq!(diff.params[0].before, serde_json::json!(14));
      assert_eq!(diff.params[0].after, serde_json::json!(21));
  }

  #[test]
  fn strategy_diff_identical_strategies_empty() {
      use crate::strategies::mod::Strategy;
      let s = Strategy::default();
      let diff = strategy_diff(&s, &s);
      assert!(diff.prose.is_empty());
      assert!(diff.params.is_empty());
      assert!(diff.tools.added.is_empty());
      assert!(diff.tools.removed.is_empty());
      assert!(diff.filter.is_empty());
  }
  ```

- [ ] **Step 3: Run tests to verify they fail first**

  ```bash
  cd /Users/edkennedy/Code/xvision
  scripts/cargo test -p xvision-engine strategy_diff 2>&1 | tail -20
  ```
  Expected: compile error or test not found (function not yet implemented).

- [ ] **Step 4: Add `strategy_diff` and helpers at the right location in the file**

  The code from Step 1 is what needs to be inserted. Check `Strategy` import and `AgentRef` import are accessible — `Strategy` is already used in this file. Confirm `FilterEdit`, `ParamChange`, `ProseEdit`, `ToolDiff` are defined in this same file (they are, around lines 133–168).

- [ ] **Step 5: Run tests to verify they pass**

  ```bash
  scripts/cargo test -p xvision-engine strategy_diff 2>&1 | tail -20
  ```
  Expected: 3 tests PASSED.

- [ ] **Step 6: Run full engine test suite to verify no regressions**

  ```bash
  scripts/cargo test -p xvision-engine 2>&1 | tail -10
  ```
  Expected: all tests pass.

- [ ] **Step 7: Commit**

  ```bash
  git add crates/xvision-engine/src/autooptimizer/mutator.rs
  git commit -m "feat(engine): add strategy_diff() for cumulative lineage diff computation"
  ```

---

## Task 2: Dashboard — three new API endpoints

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`

- [ ] **Step 1: Write failing tests in `autooptimizer_cycle.rs`**

  Add at the bottom of the file, inside a `#[cfg(test)]` block:

  ```rust
  #[cfg(test)]
  mod inspector_tests {
      use super::*;
      use axum::http::StatusCode;
      use axum_test::TestServer;

      // Helper: build a minimal AppState with an in-memory blob store.
      // (mirror the pattern used by existing tests in this file)

      #[tokio::test]
      async fn get_strategy_by_hash_returns_404_for_unknown() {
          // Build a minimal app with the new routes wired.
          // This test asserts the endpoint returns 404 when hash is not in blob store.
          // Use the same test-state builder already in this file.
          todo!("implement after endpoint is added")
      }

      #[tokio::test]
      async fn promote_strategy_saves_to_folder_and_returns_id() {
          todo!("implement after endpoint is added")
      }
  }
  ```

  Run to confirm they fail:
  ```bash
  scripts/cargo test -p xvision-dashboard inspector_tests 2>&1 | tail -10
  ```

- [ ] **Step 2: Add `get_strategy_blob` handler to `autooptimizer_cycle.rs`**

  Find the existing blob endpoint `GET /api/autooptimizer/blob/:hash` (search for `getBlob` or `/blob/`). Add a new handler immediately after that serves `GET /api/optimizer/strategy/:hash`:

  ```rust
  /// GET /api/optimizer/strategy/:hash
  /// Returns the strategy JSON for a candidate in the blob store.
  pub async fn get_optimizer_strategy(
      State(state): State<AppState>,
      Path(hash): Path<String>,
  ) -> Result<Json<serde_json::Value>, DashboardError> {
      let content_hash = ContentHash::from_hex(&hash).map_err(|_| {
          DashboardError::bad_request(format!("invalid hash: {hash}"))
      })?;
      let blob_dir = state.xvn_home.join("lineage").join("blobs");
      let blobs = BlobStore::new(blob_dir);
      let json = blobs
          .get_json(&content_hash)
          .await
          .map_err(|_| DashboardError::not_found(format!("strategy {hash} not in blob store")))?;
      Ok(Json(json))
  }
  ```

- [ ] **Step 3: Add `get_origin_diff` handler**

  Add immediately after `get_optimizer_strategy`:

  ```rust
  #[derive(serde::Serialize)]
  pub struct OriginDiffResponse {
      pub origin_hash: String,
      pub diff: xvision_engine::autooptimizer::mutator::StrategyDiff,
  }

  /// GET /api/optimizer/strategy/:hash/diff/origin
  /// Walks lineage to root, returns StrategyDiff between root strategy and this one.
  pub async fn get_origin_diff(
      State(state): State<AppState>,
      Path(hash): Path<String>,
  ) -> Result<Json<OriginDiffResponse>, DashboardError> {
      let content_hash = ContentHash::from_hex(&hash).map_err(|_| {
          DashboardError::bad_request(format!("invalid hash: {hash}"))
      })?;
      let pool = &state.pool;
      let blob_dir = state.xvn_home.join("lineage").join("blobs");
      let blobs = BlobStore::new(blob_dir);
      let lineage = xvision_engine::autooptimizer::lineage::LineageStore::new(pool.clone());

      // Walk lineage chain back to root (parent_hash IS NULL).
      let mut current_hash = content_hash.clone();
      let mut origin_hash = content_hash.clone();
      loop {
          let node = lineage
              .get(&current_hash)
              .await
              .map_err(|e| DashboardError::internal(format!("lineage lookup: {e}")))?;
          match node {
              None => break, // not in lineage; treat current as root
              Some(n) => match n.parent_hash {
                  None => {
                      origin_hash = current_hash.clone();
                      break;
                  }
                  Some(ph) => {
                      origin_hash = current_hash.clone();
                      current_hash = ph;
                  }
              },
          }
      }

      // Load origin and current strategies from blob store.
      let origin_json = blobs.get_json(&origin_hash).await.map_err(|_| {
          DashboardError::not_found(format!("origin blob {} not found", origin_hash.to_hex()))
      })?;
      let current_json = blobs.get_json(&content_hash).await.map_err(|_| {
          DashboardError::not_found(format!("strategy blob {hash} not found"))
      })?;

      let origin: xvision_engine::strategies::Strategy =
          serde_json::from_value(origin_json).map_err(|e| {
              DashboardError::internal(format!("deserialize origin strategy: {e}"))
          })?;
      let current: xvision_engine::strategies::Strategy =
          serde_json::from_value(current_json).map_err(|e| {
              DashboardError::internal(format!("deserialize current strategy: {e}"))
          })?;

      let diff = xvision_engine::autooptimizer::mutator::strategy_diff(&origin, &current);
      Ok(Json(OriginDiffResponse {
          origin_hash: origin_hash.to_hex(),
          diff,
      }))
  }
  ```

- [ ] **Step 4: Add `promote_strategy` handler**

  ```rust
  #[derive(serde::Deserialize)]
  pub struct PromoteStrategyRequest {}

  #[derive(serde::Serialize)]
  pub struct PromoteStrategyResponse {
      pub strategy_id: String,
  }

  /// POST /api/optimizer/strategy/:hash/promote
  /// Saves the blob-store strategy to the filesystem strategies folder.
  /// Idempotent: if a strategy with the same display_name prefix already exists,
  /// returns its id instead of creating a duplicate.
  pub async fn promote_strategy(
      State(state): State<AppState>,
      Path(hash): Path<String>,
  ) -> Result<Json<PromoteStrategyResponse>, DashboardError> {
      let content_hash = ContentHash::from_hex(&hash).map_err(|_| {
          DashboardError::bad_request(format!("invalid hash: {hash}"))
      })?;
      let blob_dir = state.xvn_home.join("lineage").join("blobs");
      let blobs = BlobStore::new(blob_dir);
      let strategy_json = blobs.get_json(&content_hash).await.map_err(|_| {
          DashboardError::not_found(format!("strategy {hash} not in blob store"))
      })?;

      let mut strategy: xvision_engine::strategies::Strategy =
          serde_json::from_value(strategy_json).map_err(|e| {
              DashboardError::internal(format!("deserialize strategy: {e}"))
          })?;

      // Generate a stable id and display name from the hash prefix.
      let candidate_id = format!("opt-{}", &hash[..8]);
      let display_name = format!("optimizer-candidate-{}", &hash[..8]);

      // Idempotency: check if this id already exists in the folder.
      let store_dir = xvision_engine::strategies::store::strategy_store_dir(&state.xvn_home);
      let store = xvision_engine::strategies::store::FilesystemStore::new(store_dir);
      if store.load(&candidate_id).await.is_ok() {
          return Ok(Json(PromoteStrategyResponse { strategy_id: candidate_id }));
      }

      // Stamp the strategy with the new id and display name.
      strategy.manifest.id = candidate_id.clone();
      strategy.manifest.display_name = display_name;

      store.save(&strategy).await.map_err(|e| {
          DashboardError::internal(format!("save promoted strategy: {e}"))
      })?;

      Ok(Json(PromoteStrategyResponse { strategy_id: candidate_id }))
  }
  ```

- [ ] **Step 5: Wire routes in `mod.rs`**

  Find where the existing autooptimizer routes are registered (search for `.route("/api/autooptimizer/` in `mod.rs`). Add the three new routes in the same block:

  ```rust
  .route(
      "/api/optimizer/strategy/:hash",
      axum::routing::get(routes::autooptimizer_cycle::get_optimizer_strategy),
  )
  .route(
      "/api/optimizer/strategy/:hash/diff/origin",
      axum::routing::get(routes::autooptimizer_cycle::get_origin_diff),
  )
  .route(
      "/api/optimizer/strategy/:hash/promote",
      axum::routing::post(routes::autooptimizer_cycle::promote_strategy),
  )
  ```

- [ ] **Step 6: Fill in the placeholder tests and run them**

  Replace the `todo!()` stubs in `inspector_tests` with actual axum-test calls using the pattern already in other tests in this file. Key assertions:
  - `GET /api/optimizer/strategy/deadbeef00000000` → 400 (invalid hex hash length)
  - `GET /api/optimizer/strategy/<valid-but-absent-hash>` → 404
  - `POST /api/optimizer/strategy/<hash>/promote` with a pre-seeded blob → 200 with `strategy_id`
  - Second `POST /api/optimizer/strategy/<hash>/promote` → 200 with same `strategy_id` (idempotent)

  ```bash
  scripts/cargo test -p xvision-dashboard inspector_tests 2>&1 | tail -15
  ```
  Expected: all tests PASS.

- [ ] **Step 7: Run full dashboard test suite**

  ```bash
  scripts/cargo test -p xvision-dashboard 2>&1 | tail -10
  ```
  Expected: all pass.

- [ ] **Step 8: Commit**

  ```bash
  git add crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs \
          crates/xvision-dashboard/src/routes/mod.rs
  git commit -m "feat(dashboard): add optimizer strategy inspector API endpoints"
  ```

---

## Task 3: CLI — migrate cycle commands into `optimize.rs`

**Files:**
- Modify: `crates/xvision-cli/src/commands/optimize.rs`
- Modify: `crates/xvision-cli/src/commands/autooptimizer.rs`

This task moves the real optimizer implementations from `autooptimizer.rs` into `optimize.rs`, replaces the stub `run_optimize()` with a thin wrapper over `run_cycle_cmd`, and makes all args, helpers, and dispatch code canonical in `optimize.rs`.

- [ ] **Step 1: Write a failing integration test for the new `xvn optimize run`**

  In `crates/xvision-cli/src/commands/optimize.rs` bottom, add:

  ```rust
  #[cfg(test)]
  mod migration_tests {
      use super::*;

      /// Verify RunCycleArgs (now in optimize.rs) has `--strategy` as a required field.
      #[test]
      fn run_cycle_args_strategy_is_required() {
          use clap::CommandFactory;
          let cmd = OptimizeCmd::command();
          let run_sub = cmd.find_subcommand("run").expect("run subcommand must exist");
          let strategy_arg = run_sub.get_arguments().find(|a| a.get_id() == "strategy");
          assert!(strategy_arg.is_some(), "--strategy arg must exist on `xvn optimize run`");
          assert!(
              strategy_arg.unwrap().is_required_set(),
              "--strategy must be required on `xvn optimize run`"
          );
      }
  }
  ```

  Run:
  ```bash
  scripts/cargo test -p xvision-cli migration_tests 2>&1 | tail -10
  ```
  Expected: FAIL (no `strategy` arg on `xvn optimize run` yet).

- [ ] **Step 2: Remove the stub sub-commands from `OptimizeAction`**

  In `optimize.rs`, replace the entire `OptimizeAction` enum with:

  ```rust
  #[derive(Subcommand, Debug)]
  enum OptimizeAction {
      /// Run the full optimizer cycle (parent selection → experiment → gate → judge).
      /// Both the experiment writer (Mutator) and the judge use the strategy's bound model.
      Run(RunCycleArgs),
      /// Propose one experiment, gate it, and commit to lineage (single-shot).
      MutateOnce(MutateOnceArgs),
      /// Run the full optimizer cycle (alias for `run`; prefer `run`).
      RunCycle(RunCycleArgs),
      /// Replay a saved optimizer cycle from a fixture (no API keys required).
      Demo(DemoArgs),
      /// Show a persisted optimization run, its candidates, and snapshots.
      Inspect(InspectArgs),
  }
  ```

  Note: `Run` and `RunCycle` both use `RunCycleArgs` — `Run` enforces `--strategy` as required (see Step 4), `RunCycle` keeps it optional for backwards-compat with existing scripts.

- [ ] **Step 3: Copy `RunCycleArgs`, `MutateOnceArgs`, `DemoArgs` structs from `autooptimizer.rs` into `optimize.rs`**

  Find these structs in `autooptimizer.rs` (around lines 94–300) and copy the full struct definitions verbatim into `optimize.rs`, placing them before the `run_optimize` function. Make them `pub` so `autooptimizer.rs` can re-export them.

  Also copy all required imports from `autooptimizer.rs` that are not already in `optimize.rs` — specifically:
  ```rust
  use std::sync::Arc;
  use chrono::Utc;
  use ulid::Ulid;
  use xvision_core::config::{self, ConfigError, InternProvider, ProviderEntry, ProviderKind, RuntimeConfig};
  use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch, OpenaiCompatDispatch};
  use xvision_engine::api::autooptimizer::{self};
  use xvision_engine::api::{Actor, ApiContext};
  use xvision_engine::autooptimizer::blob_store::BlobStore;
  use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
  use xvision_engine::autooptimizer::content_hash::ContentHash;
  use xvision_engine::autooptimizer::cycle::{run_cycle, CycleConfig};
  use xvision_engine::autooptimizer::cycle_runs::{get_cycle_run, list_cycle_runs, CycleRunDetail, CycleRunSummary};
  use xvision_engine::autooptimizer::eval_adapter::{BudgetCappedPaperTester, CachedBacktestPaperTester, PaperTestRunner, StubPaperTester};
  use xvision_engine::autooptimizer::gate::GateVerdict;
  use xvision_engine::autooptimizer::judge::Judge;
  use xvision_engine::autooptimizer::lineage::{ensure_lineage_schema, LineageNode, LineageStatus, LineageStore};
  use xvision_engine::autooptimizer::local_dispatch::AutoOptimizerLocalDispatch;
  use xvision_engine::autooptimizer::metering_dispatch::{CostMeteringDispatch, CycleMeter};
  use xvision_engine::autooptimizer::mutator::{MutationDiff, Mutator};
  use xvision_engine::autooptimizer::parent_policy::ParentPolicy;
  use xvision_engine::autooptimizer::progress::CycleProgressEvent;
  use xvision_engine::autooptimizer::scenario_synthesis::{synthesize_baseline_untouched_scenario, synthesize_optimizer_day_scenario};
  use xvision_engine::eval::run::MetricsSummary;
  use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
  use xvision_engine::strategies::Strategy;
  use xvision_engine::tools::ToolRegistry;
  ```

- [ ] **Step 4: Make `RunCycleArgs.strategy` required when dispatched from `Run`**

  The `RunCycleArgs` struct keeps `strategy: Option<String>` so it can be shared between `Run` (where strategy is required) and `RunCycle` (where it is optional for compat). The `run` dispatch path validates this at runtime:

  ```rust
  async fn run_optimize(args: RunCycleArgs) -> CliResult<()> {
      if args.strategy.is_none() {
          return Err(CliError::usage(anyhow::anyhow!(
              "`xvn optimize run` requires --strategy <id>; \
               use `xvn optimize run-cycle` if you need the legacy no-strategy-id path"
          )));
      }
      run_cycle_cmd(args).await
  }
  ```

  For the clap-level argument, you can mark it required at the `Run` subcommand level by wrapping `RunCycleArgs` in a newtype struct `RunArgs` that re-declares `--strategy` as required, or simply use the runtime check above (simpler, avoids struct duplication).

- [ ] **Step 5: Copy implementation functions from `autooptimizer.rs` into `optimize.rs`**

  Move these functions verbatim from `autooptimizer.rs` to `optimize.rs`, making them `pub` so `autooptimizer.rs` can re-use them:

  - `run_cycle_cmd(args: RunCycleArgs) -> CliResult<()>` (line ~1168, ~300 lines)
  - `run_mutate_once(args: MutateOnceArgs) -> CliResult<()>` (line ~1030, ~130 lines)
  - `run_demo_cmd(args: DemoArgs) -> CliResult<()>` (find via `Op::Demo`)
  - All shared helpers called by the above: `load_ar_config`, `build_dispatch`, `default_blob_dir`, `open_and_migrate_db`, `load_strategy_blob`, `validate_budget_usd`, `require_launchable_provider`, `ipc_send_event`, `insert_lineage_node`, `propose`, `gate_passes`, `load_metering_catalogs` (copy or re-export from `autooptimizer.rs`)

  After copying, update the `run` dispatcher in `optimize.rs`:

  ```rust
  pub async fn run(cmd: OptimizeCmd) -> CliResult<()> {
      match cmd.action {
          OptimizeAction::Run(args) => run_optimize(args).await,
          OptimizeAction::RunCycle(args) => run_cycle_cmd(args).await,
          OptimizeAction::MutateOnce(args) => run_mutate_once(args).await,
          OptimizeAction::Demo(args) => run_demo_cmd(args).await,
          OptimizeAction::Inspect(args) => run_inspect(args).await,
      }
  }
  ```

- [ ] **Step 6: Run compile check**

  ```bash
  scripts/cargo build -p xvision-cli 2>&1 | grep "error\[" | head -20
  ```
  Fix any compile errors. Common issues: missing imports, duplicate function definitions (if copies weren't cleaned from `autooptimizer.rs` yet).

- [ ] **Step 7: Run CLI tests**

  ```bash
  scripts/cargo test -p xvision-cli 2>&1 | tail -15
  ```
  Expected: all pass including `migration_tests::run_cycle_args_strategy_is_required`.

- [ ] **Step 8: Commit**

  ```bash
  git add crates/xvision-cli/src/commands/optimize.rs \
          crates/xvision-cli/src/commands/autooptimizer.rs
  git commit -m "feat(cli): migrate optimizer cycle commands into xvn optimize"
  ```

---

## Task 4: CLI — `xvn optimizer` deprecation shim (depends on Task 3)

**Files:**
- Modify: `crates/xvision-cli/src/commands/autooptimizer.rs`
- Modify: `crates/xvision-cli/src/lib.rs`

- [ ] **Step 1: Write a test that `xvn optimizer run-cycle` delegates to `optimize.rs`**

  In `autooptimizer.rs` at the bottom:

  ```rust
  #[cfg(test)]
  mod shim_tests {
      #[test]
      fn run_cycle_shim_is_annotated_deprecated() {
          // This test documents the intent rather than testing runtime behavior:
          // Op::RunCycle handler now just calls commands::optimize::run_cycle_cmd.
          // Compile-time check: ensure the type still exists and builds.
          use super::Op;
          let _ = std::mem::discriminant(&Op::MutateOnce(Default::default()));
      }
  }
  ```

- [ ] **Step 2: Remove the moved implementations from `autooptimizer.rs`**

  Delete from `autooptimizer.rs`:
  - `run_cycle_cmd` function body (replaced by `commands::optimize::run_cycle_cmd`)
  - `run_mutate_once` function body
  - `run_demo_cmd` function body
  - All helpers that were moved to `optimize.rs` (they're now imported from there)

  Replace the `Op` dispatch in `autooptimizer.rs::run()` with deprecation shims:

  ```rust
  pub async fn run(cmd: AutoOptimizerCmd) -> CliResult<()> {
      match cmd.op {
          // Pattern distillation verbs — deprecated; the autooptimizer cycle handles
          // prompt improvement internally. These will be removed in a future release.
          Op::Run(args) => {
              eprintln!(
                  "warning: `xvn optimizer run` is deprecated. Pattern distillation is now \
                   handled internally by the optimizer. Use `xvn optimize run-cycle` to run \
                   an optimization cycle."
              );
              run_distill(args).await
          }
          Op::Ls(args) => {
              eprintln!("warning: `xvn optimizer ls` is deprecated. Use the dashboard Optimizer panel.");
              run_list(args).await
          }
          Op::Inspect(args) => {
              eprintln!("warning: `xvn optimizer inspect` is deprecated. Use `xvn optimize inspect`.");
              run_inspect(args).await
          }
          Op::Gate(args) => {
              eprintln!("warning: `xvn optimizer gate` is deprecated. Gating is handled automatically by the optimizer cycle.");
              run_gate(args).await
          }
          Op::Activate(args) | Op::Promote(args) => {
              eprintln!("warning: `xvn optimizer activate` is deprecated. Use the dashboard Optimizer panel to promote experiments.");
              run_activate(args).await
          }
          Op::Retire(args) | Op::Demote(args) => {
              eprintln!("warning: `xvn optimizer retire` is deprecated.");
              run_retire(args).await
          }
          Op::Lineage(cmd) => {
              eprintln!("warning: `xvn optimizer lineage` is deprecated. Use the dashboard Optimizer panel.");
              match cmd.op {
                  LineageOp::Ls(args) => lineage_ls(args).await,
                  LineageOp::Show(args) => lineage_show(args).await,
              }
          }
          // Cycle verbs — now canonical in `xvn optimize`, delegated here.
          Op::MutateOnce(args) => {
              eprintln!("warning: `xvn optimizer mutate-once` is deprecated. Use `xvn optimize mutate-once`.");
              crate::commands::optimize::run_mutate_once_pub(args).await
          }
          Op::RunCycle(args) => {
              eprintln!("warning: `xvn optimizer run-cycle` is deprecated. Use `xvn optimize run-cycle`.");
              crate::commands::optimize::run_cycle_cmd_pub(args).await
          }
          Op::Demo(args) => {
              eprintln!("warning: `xvn optimizer demo` is deprecated. Use `xvn optimize demo`.");
              crate::commands::optimize::run_demo_cmd_pub(args).await
          }
      }
  }
  ```

  Note: `run_cycle_cmd_pub`, `run_mutate_once_pub`, `run_demo_cmd_pub` are the `pub` versions of those functions in `optimize.rs` (rename them to `pub` in Task 3 Step 5).

- [ ] **Step 3: Update the `lib.rs` command description for `xvn optimizer`**

  In `lib.rs` around line 228, update the doc comment:

  ```rust
  /// DEPRECATED: use `xvn optimize` instead. Retained for backwards compatibility.
  /// Offline optimizer operations over memory Observations.
  #[command(name = "optimizer")]
  AutoOptimizer(commands::autooptimizer::AutoOptimizerCmd),
  ```

- [ ] **Step 4: Build and test**

  ```bash
  scripts/cargo build -p xvision-cli 2>&1 | grep "error\[" | head -10
  scripts/cargo test -p xvision-cli 2>&1 | tail -10
  ```
  Expected: clean build, all tests pass.

- [ ] **Step 5: Smoke test the deprecation messages**

  ```bash
  cargo run -p xvision-cli -- optimizer run-cycle --help 2>&1 | head -5
  ```
  Expected: deprecation warning printed to stderr, help text shown.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/xvision-cli/src/commands/autooptimizer.rs \
          crates/xvision-cli/src/lib.rs
  git commit -m "feat(cli): deprecate xvn optimizer, delegate cycle commands to xvn optimize"
  ```

---

## Task 5: Frontend — `OriginDiffPanel` component

**Files:**
- Create: `frontend/web/src/features/autooptimizer/panels/OriginDiffPanel.tsx`
- Create: `frontend/web/src/features/autooptimizer/panels/OriginDiffPanel.test.tsx`
- Modify: `frontend/web/src/features/autooptimizer/api.ts`

- [ ] **Step 1: Add API hook to `api.ts`**

  In `api.ts`, after `useBlob` (around line 458), add:

  ```typescript
  export interface StrategyDiff {
    prose: Array<{ agent_role: string; before: string; after: string }>;
    params: Array<{ key: string; before: unknown; after: unknown }>;
    tools: { added: string[]; removed: string[] };
    filter: Array<{ path: string; before: unknown; after: unknown }>;
  }

  export interface OriginDiffResponse {
    origin_hash: string;
    diff: StrategyDiff;
  }

  export async function getOriginDiff(hash: string): Promise<OriginDiffResponse> {
    return apiFetch<OriginDiffResponse>(
      `/api/optimizer/strategy/${encodeURIComponent(hash)}/diff/origin`
    );
  }

  export function useOriginDiff(hash: string | null | undefined) {
    return useQuery({
      queryKey: [...autooptimizerKeys.all, "origin-diff", hash ?? ""] as const,
      queryFn: () => getOriginDiff(hash!),
      enabled: !!hash,
    });
  }
  ```

- [ ] **Step 2: Write failing test for `OriginDiffPanel`**

  Create `OriginDiffPanel.test.tsx`:

  ```tsx
  import { describe, it, expect, vi, beforeEach } from "vitest";
  import { render, screen } from "@testing-library/react";
  import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
  import { OriginDiffPanel } from "./OriginDiffPanel";
  import * as api from "../api";

  vi.mock("../api", async () => {
    const actual = await vi.importActual<typeof import("../api")>("../api");
    return { ...actual, useOriginDiff: vi.fn() };
  });

  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={new QueryClient()}>
      {children}
    </QueryClientProvider>
  );

  describe("OriginDiffPanel", () => {
    beforeEach(() => vi.clearAllMocks());

    it("shows loading state while fetching", () => {
      vi.mocked(api.useOriginDiff).mockReturnValue({
        data: undefined,
        isLoading: true,
        isError: false,
      } as ReturnType<typeof api.useOriginDiff>);
      render(<OriginDiffPanel hash="abc123" />, { wrapper });
      expect(screen.getByText(/loading/i)).toBeTruthy();
    });

    it("renders prose changes", () => {
      vi.mocked(api.useOriginDiff).mockReturnValue({
        data: {
          origin_hash: "deadbeef",
          diff: {
            prose: [{ agent_role: "trader", before: "buy low", after: "sell high" }],
            params: [],
            tools: { added: [], removed: [] },
            filter: [],
          },
        },
        isLoading: false,
        isError: false,
      } as ReturnType<typeof api.useOriginDiff>);
      render(<OriginDiffPanel hash="abc123" />, { wrapper });
      expect(screen.getByText("trader")).toBeTruthy();
      expect(screen.getByText("sell high")).toBeTruthy();
    });

    it("renders empty state when no changes", () => {
      vi.mocked(api.useOriginDiff).mockReturnValue({
        data: {
          origin_hash: "deadbeef",
          diff: { prose: [], params: [], tools: { added: [], removed: [] }, filter: [] },
        },
        isLoading: false,
        isError: false,
      } as ReturnType<typeof api.useOriginDiff>);
      render(<OriginDiffPanel hash="abc123" />, { wrapper });
      expect(screen.getByText(/no changes/i)).toBeTruthy();
    });
  });
  ```

  Run:
  ```bash
  cd /Users/edkennedy/Code/xvision/frontend/web
  npx vitest run src/features/autooptimizer/panels/OriginDiffPanel.test.tsx 2>&1 | tail -15
  ```
  Expected: FAIL (component not yet created).

- [ ] **Step 3: Create `OriginDiffPanel.tsx`**

  ```tsx
  import { HashSigil } from "../ui/HashSigil";
  import { useOriginDiff } from "../api";

  interface Props {
    hash: string;
  }

  function DiffRow({ label, before, after }: { label: string; before: string; after: string }) {
    return (
      <div className="space-y-1">
        <div className="text-[11px] font-medium text-text-2 uppercase tracking-wider">{label}</div>
        <div className="grid grid-cols-2 gap-3">
          <div className="rounded border border-border bg-surface-muted p-2 font-mono text-[11px] text-text-3 line-through">
            {before || <span className="italic text-text-4">empty</span>}
          </div>
          <div className="rounded border border-border bg-surface-muted p-2 font-mono text-[11px]">
            {after || <span className="italic text-text-4">empty</span>}
          </div>
        </div>
      </div>
    );
  }

  export function OriginDiffPanel({ hash }: Props) {
    const { data, isLoading, isError } = useOriginDiff(hash);

    if (isLoading) {
      return <p className="text-[12px] text-text-3">Loading origin diff…</p>;
    }
    if (isError || !data) {
      return <p className="text-[12px] text-danger">Could not load origin diff.</p>;
    }

    const { origin_hash, diff } = data;
    const hasChanges =
      diff.prose.length > 0 ||
      diff.params.length > 0 ||
      diff.tools.added.length > 0 ||
      diff.tools.removed.length > 0 ||
      diff.filter.length > 0;

    return (
      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <HashSigil hash={origin_hash} size={24} />
          <span className="font-mono text-[11px] text-text-3">
            origin: {origin_hash.slice(0, 10)}
          </span>
        </div>

        {!hasChanges ? (
          <p className="text-[12px] text-text-3">No changes from originating strategy.</p>
        ) : (
          <div className="space-y-4">
            {diff.prose.map((p, i) => (
              <DiffRow key={i} label={`prose · ${p.agent_role}`} before={p.before} after={p.after} />
            ))}
            {diff.params.map((p, i) => (
              <DiffRow
                key={i}
                label={`param · ${p.key}`}
                before={String(p.before)}
                after={String(p.after)}
              />
            ))}
            {diff.tools.added.map((t, i) => (
              <DiffRow key={`add-${i}`} label="tool added" before="" after={t} />
            ))}
            {diff.tools.removed.map((t, i) => (
              <DiffRow key={`rm-${i}`} label="tool removed" before={t} after="" />
            ))}
            {diff.filter.map((f, i) => (
              <DiffRow
                key={i}
                label={`filter · ${f.path}`}
                before={String(f.before)}
                after={String(f.after)}
              />
            ))}
          </div>
        )}
      </div>
    );
  }
  ```

- [ ] **Step 4: Run tests**

  ```bash
  npx vitest run src/features/autooptimizer/panels/OriginDiffPanel.test.tsx 2>&1 | tail -15
  ```
  Expected: all 3 tests PASS.

- [ ] **Step 5: Run full frontend test suite**

  ```bash
  npx vitest run 2>&1 | tail -10
  ```
  Expected: all pass.

- [ ] **Step 6: Commit**

  ```bash
  cd /Users/edkennedy/Code/xvision
  git add frontend/web/src/features/autooptimizer/panels/OriginDiffPanel.tsx \
          frontend/web/src/features/autooptimizer/panels/OriginDiffPanel.test.tsx \
          frontend/web/src/features/autooptimizer/api.ts
  git commit -m "feat(frontend): add OriginDiffPanel and useOriginDiff hook"
  ```

---

## Task 6: Frontend — `StrategyInspector` screen + route wiring (depends on Tasks 2 + 5)

**Files:**
- Create: `frontend/web/src/features/autooptimizer/screens/StrategyInspector.tsx`
- Create: `frontend/web/src/features/autooptimizer/screens/StrategyInspector.test.tsx`
- Modify: `frontend/web/src/features/autooptimizer/api.ts`
- Modify: `frontend/web/src/features/autooptimizer/screens/ExperimentDetail.tsx`
- Modify: `frontend/web/src/routes.tsx`

- [ ] **Step 1: Add promote API hook to `api.ts`**

  In `api.ts`, after `useOriginDiff`:

  ```typescript
  export async function promoteStrategy(hash: string): Promise<{ strategy_id: string }> {
    return apiFetch<{ strategy_id: string }>(
      `/api/optimizer/strategy/${encodeURIComponent(hash)}/promote`,
      { method: "POST", body: "{}" }
    );
  }
  ```

- [ ] **Step 2: Write failing tests**

  Create `StrategyInspector.test.tsx`:

  ```tsx
  import { describe, it, expect, vi, beforeEach } from "vitest";
  import { render, screen, fireEvent, waitFor } from "@testing-library/react";
  import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
  import { MemoryRouter, Route, Routes } from "react-router-dom";
  import { StrategyInspector } from "./StrategyInspector";
  import * as api from "../api";

  vi.mock("../api", async () => {
    const actual = await vi.importActual<typeof import("../api")>("../api");
    return { ...actual, useBlob: vi.fn(), useLineageNode: vi.fn(), promoteStrategy: vi.fn() };
  });
  vi.mock("./../../panels/OriginDiffPanel", () => ({ OriginDiffPanel: () => <div>origin-diff</div> }));
  vi.mock("./../../panels/ParentDiffPanel", () => ({ ParentDiffPanel: () => <div>parent-diff</div> }));

  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <MemoryRouter initialEntries={["/optimizer/strategy/abc123"]}>
        <Routes>
          <Route path="/optimizer/strategy/:hash" element={children} />
          <Route path="/strategies" element={<div>strategies-page</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );

  describe("StrategyInspector", () => {
    beforeEach(() => vi.clearAllMocks());

    it("renders strategy manifest when blob loaded", () => {
      vi.mocked(api.useBlob).mockReturnValue({
        data: { manifest: { display_name: "Test Strategy", id: "s1" } },
        isLoading: false, isError: false,
      } as ReturnType<typeof api.useBlob>);
      vi.mocked(api.useLineageNode).mockReturnValue({
        data: { bundle_hash: "abc123", parent_hash: null, gate_verdict: "pass", status: "active", cycle_id: null, created_at: "" },
        isLoading: false, isError: false,
      } as ReturnType<typeof api.useLineageNode>);
      render(<StrategyInspector />, { wrapper });
      expect(screen.getByText("Test Strategy")).toBeTruthy();
    });

    it("shows Promote to Eval button", () => {
      vi.mocked(api.useBlob).mockReturnValue({ data: { manifest: {} }, isLoading: false, isError: false } as ReturnType<typeof api.useBlob>);
      vi.mocked(api.useLineageNode).mockReturnValue({ data: { bundle_hash: "abc123", parent_hash: null, gate_verdict: "pass", status: "active", cycle_id: null, created_at: "" }, isLoading: false, isError: false } as ReturnType<typeof api.useLineageNode>);
      render(<StrategyInspector />, { wrapper });
      expect(screen.getByRole("button", { name: /promote to eval/i })).toBeTruthy();
    });

    it("navigates to /strategies after promote", async () => {
      vi.mocked(api.useBlob).mockReturnValue({ data: { manifest: {} }, isLoading: false, isError: false } as ReturnType<typeof api.useBlob>);
      vi.mocked(api.useLineageNode).mockReturnValue({ data: { bundle_hash: "abc123", parent_hash: null, gate_verdict: "pass", status: "active", cycle_id: null, created_at: "" }, isLoading: false, isError: false } as ReturnType<typeof api.useLineageNode>);
      vi.mocked(api.promoteStrategy).mockResolvedValue({ strategy_id: "opt-abc12300" });
      render(<StrategyInspector />, { wrapper });
      fireEvent.click(screen.getByRole("button", { name: /promote to eval/i }));
      await waitFor(() => screen.getByText("strategies-page"));
    });
  });
  ```

  Run:
  ```bash
  cd /Users/edkennedy/Code/xvision/frontend/web
  npx vitest run src/features/autooptimizer/screens/StrategyInspector.test.tsx 2>&1 | tail -15
  ```
  Expected: FAIL (component not yet created).

- [ ] **Step 3: Create `StrategyInspector.tsx`**

  ```tsx
  import { useState } from "react";
  import { useNavigate, useParams } from "react-router-dom";
  import { Topbar } from "@/components/shell/Topbar";
  import { Breadcrumb } from "../ui/Breadcrumb";
  import { HashSigil } from "../ui/HashSigil";
  import { GateBadge } from "../ui/GateBadge";
  import { GateScorecard } from "../panels/GateScorecard";
  import { ParentDiffPanel } from "../panels/ParentDiffPanel";
  import { OriginDiffPanel } from "../panels/OriginDiffPanel";
  import {
    useBlob,
    useLineageNode,
    useExperimentDetail,
    formatGateVerdict,
    promoteStrategy,
  } from "../api";

  function CollapsibleSection({ title, children }: { title: string; children: React.ReactNode }) {
    const [open, setOpen] = useState(true);
    return (
      <div className="rounded-md border border-border">
        <button
          className="flex w-full items-center justify-between px-4 py-3 text-left text-[13px] font-semibold"
          onClick={() => setOpen(!open)}
        >
          {title}
          <span className="text-text-3">{open ? "▲" : "▼"}</span>
        </button>
        {open && <div className="border-t border-border p-4">{children}</div>}
      </div>
    );
  }

  export function StrategyInspector() {
    const { hash = "" } = useParams<{ hash: string }>();
    const navigate = useNavigate();
    const [promoting, setPromoting] = useState(false);

    const { data: blob, isLoading: blobLoading, isError: blobError } = useBlob(hash);
    const { data: node, isLoading: nodeLoading, isError: nodeError } = useLineageNode(hash);
    const { data: detail } = useExperimentDetail(hash);

    const handlePromote = async () => {
      setPromoting(true);
      try {
        await promoteStrategy(hash);
        navigate("/strategies");
      } finally {
        setPromoting(false);
      }
    };

    const manifest = (blob as { manifest?: Record<string, unknown> })?.manifest;

    return (
      <>
        <Topbar
          title="Optimizer"
          sub="Strategy inspector"
          back={{ to: `/optimizer/experiment/${hash}`, label: "Back to Experiment" }}
        />
        <div className="space-y-5">
          <Breadcrumb
            items={[
              { label: "OPTIMIZER", to: "/optimizer" },
              { label: "experiment", to: node?.cycle_id ? `/optimizer/cycle/${encodeURIComponent(node.cycle_id)}` : undefined },
              { label: "strategy" },
            ]}
          />

          {blobLoading || nodeLoading ? (
            <p className="text-[12px] text-text-3">Loading strategy…</p>
          ) : blobError || nodeError ? (
            <p className="text-[12px] text-danger">Could not load strategy.</p>
          ) : (
            <>
              {/* ── Strategy identity ─────────────────────────────────────── */}
              <section className="flex items-start gap-4 rounded-md border border-border bg-surface-card p-5">
                <HashSigil hash={hash} size={72} />
                <div className="min-w-0">
                  <div className="mb-1 text-[8.5px] uppercase tracking-widest text-text-3">
                    Optimizer · Candidate Strategy
                  </div>
                  <h1 className="m-0 font-mono text-[22px] tracking-tight">
                    {(manifest?.display_name as string) || hash.slice(0, 16)}
                  </h1>
                  {node && (
                    <div className="mt-1 flex items-center gap-2">
                      <GateBadge verdict={formatGateVerdict(node.gate_verdict)} status={node.status} />
                      <span className="font-mono text-[11px] text-text-3">
                        {hash.slice(0, 16)}
                      </span>
                    </div>
                  )}
                </div>
              </section>

              {/* ── Raw strategy content (mirroring strategy page) ────────── */}
              {blob && (
                <section className="rounded-md border border-border bg-surface-card p-5">
                  <h2 className="m-0 mb-3 text-[15px] font-semibold">Strategy Content</h2>
                  <pre className="overflow-x-auto rounded bg-surface-muted p-3 font-mono text-[11px]">
                    {JSON.stringify(blob, null, 2)}
                  </pre>
                </section>
              )}

              {/* ── Optimizer Lineage ─────────────────────────────────────── */}
              <section className="space-y-3">
                <h2 className="m-0 text-[15px] font-semibold">Optimizer Lineage</h2>

                <GateScorecard gate_record={detail?.gate_record ?? null} />

                <CollapsibleSection title="Diff from parent">
                  <ParentDiffPanel childHash={hash} parentHash={node?.parent_hash ?? null} />
                </CollapsibleSection>

                <CollapsibleSection title="Diff from originating strategy">
                  <OriginDiffPanel hash={hash} />
                </CollapsibleSection>
              </section>

              {/* ── Promote to Eval ───────────────────────────────────────── */}
              <section className="rounded-md border border-border bg-surface-card p-5">
                <h2 className="m-0 mb-2 text-[15px] font-semibold">Promote to Eval</h2>
                <p className="mb-3 text-[12px] text-text-3">
                  Saves this strategy to your strategies folder. You can then start an eval run from the strategies page.
                </p>
                <button
                  className="rounded bg-brand px-4 py-2 text-[13px] font-medium text-white disabled:opacity-50"
                  onClick={handlePromote}
                  disabled={promoting}
                >
                  {promoting ? "Promoting…" : "Promote to Eval"}
                </button>
              </section>
            </>
          )}
        </div>
      </>
    );
  }
  ```

- [ ] **Step 4: Run tests**

  ```bash
  npx vitest run src/features/autooptimizer/screens/StrategyInspector.test.tsx 2>&1 | tail -15
  ```
  Expected: all 3 tests PASS.

- [ ] **Step 5: Add "View Strategy" link to `ExperimentDetail.tsx`**

  In `ExperimentDetail.tsx`, after the hash sigil hero block (after line ~60), add a navigation link:

  ```tsx
  import { Link } from "react-router-dom";

  // Inside the hero section, after the GateBadge row:
  <Link
    to={`/optimizer/strategy/${encodeURIComponent(node.bundle_hash)}`}
    className="mt-2 inline-block text-[11px] text-brand underline"
  >
    View strategy →
  </Link>
  ```

- [ ] **Step 6: Wire route in `routes.tsx`**

  In `routes.tsx`, after the existing optimizer screen imports (around lines 61–64), add:

  ```tsx
  const OptimizerStrategyInspector = lazy(() =>
    import("./features/autooptimizer/screens/StrategyInspector").then((m) => ({
      default: m.StrategyInspector,
    }))
  );
  ```

  In the route definitions, inside the same optimizer route group as `optimizer/cycle/:id` and `optimizer/experiment/:hash`, add:

  ```tsx
  { path: "optimizer/strategy/:hash", element: <OptimizerStrategyInspector /> },
  ```

- [ ] **Step 7: Run full frontend test suite**

  ```bash
  npx vitest run 2>&1 | tail -10
  ```
  Expected: all tests pass.

- [ ] **Step 8: Commit**

  ```bash
  cd /Users/edkennedy/Code/xvision
  git add frontend/web/src/features/autooptimizer/screens/StrategyInspector.tsx \
          frontend/web/src/features/autooptimizer/screens/StrategyInspector.test.tsx \
          frontend/web/src/features/autooptimizer/screens/ExperimentDetail.tsx \
          frontend/web/src/features/autooptimizer/api.ts \
          frontend/web/src/routes.tsx
  git commit -m "feat(frontend): Strategy Inspector screen at /optimizer/strategy/:hash"
  ```

---

## Task 7: Documentation subagent (depends on Tasks 3–6)

**Files:**
- Modify: `.claude/skills/xvision-cli/SKILL.md`
- Modify: `.claude/skills/xvision-cli-qa/SKILL.md`

- [ ] **Step 1: Spawn a documentation subagent with the following prompt**

  Dispatch a fresh subagent (Haiku is sufficient) with this exact prompt:

  > Update the xvision CLI skill files to reflect the optimizer CLI migration. The changes are:
  >
  > 1. `xvn optimize` is now the canonical optimizer CLI. It has these sub-commands:
  >    - `run --strategy <id> [--cycles N] [--mock]` — run the full optimizer cycle against a strategy
  >    - `run-cycle` — same as run but `--strategy` is optional (for scripted use)
  >    - `mutate-once --parent-bundle-hash <hex>` — single mutation proposal
  >    - `demo` — replay a saved cycle from a fixture
  >    - `inspect --run <id>` — inspect a persisted optimization run
  >
  > 2. `xvn optimizer` is deprecated. All its sub-commands print a deprecation warning and delegate to `xvn optimize`. Remove `xvn optimizer` from examples and replace with `xvn optimize`.
  >
  > 3. The old `xvn optimize run --agent/--slot/--corpus/--metric` signature no longer exists. Replace any examples of it with `xvn optimize run --strategy <id>`.
  >
  > Files to update:
  > - `.claude/skills/xvision-cli/SKILL.md`
  > - `.claude/skills/xvision-cli-qa/SKILL.md`
  >
  > Make targeted replacements only — do not restructure unrelated sections.

- [ ] **Step 2: Review subagent's changes**

  Read the updated skill files and verify:
  - Old `xvn optimizer` references are replaced or removed
  - Old `xvn optimize --agent/--slot` examples are removed
  - New `xvn optimize run --strategy <id>` is correctly documented

- [ ] **Step 3: Commit**

  ```bash
  git add .claude/skills/xvision-cli/SKILL.md \
          .claude/skills/xvision-cli-qa/SKILL.md
  git commit -m "docs(skills): update xvn optimize docs, deprecate xvn optimizer references"
  ```

---

## Final verification

After all tasks complete:

```bash
# Full build
scripts/cargo build --workspace 2>&1 | tail -5

# Full Rust test suite
scripts/cargo test --workspace 2>&1 | tail -10

# Full frontend test suite
cd frontend/web && npx vitest run 2>&1 | tail -10
```

All must pass before pushing.

```bash
git push
```
