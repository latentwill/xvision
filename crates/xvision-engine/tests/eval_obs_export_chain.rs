//! Regression guard for `fix/eval-obs-recording` — the CLI-launched eval
//! path was recording ZERO observability spans / events / model_calls, so
//! the trace dock showed nothing for any `xvn eval run`.
//!
//! Two independent gaps were fixed:
//!
//!   GAP 1 — the CLI eval `ApiContext` had no obs bus, so the executor's
//!           `emit_*` calls were silent no-ops (`spans: []`). The CLI now
//!           wires an `ObsRunEventBus` + `SqliteRecorder` onto the ctx, the
//!           same way the dashboard's in-process ctx does.
//!
//!   GAP 2 — the Cline trader path (`execute_slot_cline`) returned token
//!           usage but never emitted `ModelCallFinished`, so even with the
//!           bus on, `model_calls` stayed empty. The success path now emits
//!           it with the real `step.usage` tokens + provider/model/span.
//!
//! These tests exercise the REAL emit → bus → `SqliteRecorder` → SQLite →
//! `build_export` chain (no hand-published synthetic events, no fabricated
//! spans). Before the fix:
//!   * `obs_wired_backtest_executor_persists_spans_to_sqlite` — passes for
//!     spans (the executor always emitted them when handed a bus); its job
//!     is to lock the SQLite-persistence + `build_export` half that the
//!     pre-existing `NoopRecorder` tests never covered, so a future
//!     regression in the recorder/export path is caught.
//!   * `cline_slot_success_emits_model_call_finished_row` — FAILS before the
//!     GAP-2 fix (the Cline success path emitted no `ModelCallFinished`, so
//!     zero `model_calls` rows land).

#![allow(deprecated)] // canonical_scenarios() — same pattern as eval_outcome_observability.rs

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use sqlx::SqlitePool;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::execute_cline::{execute_slot_cline, ClineSlotInput, TrajectoryMode};
use xvision_engine::agent::llm::{LlmDispatch, ResponseSchema};
use xvision_engine::agent::observability::{fresh_span_id, ObsEmitter, ObsRetentionPolicy};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;
use xvision_observability::types::RunStatus;
use xvision_observability::{
    AgentRunRecorder, BlobStore, ObservabilityConfig, RetentionMode, RunEventBus, SqliteRecorder,
};

mod support;

use support::eval_harness::{fresh_store, sequenced_dispatch, strategy_with};

/// Poll a count(*) query until it reaches `expected` (or times out). The
/// bus consumer is a background task; the recorder writes asynchronously,
/// so we wait for the rows rather than racing the drain.
async fn wait_for_count(pool: &SqlitePool, sql: &str, expected: i64) -> i64 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let (n,): (i64,) = sqlx::query_as(sql).fetch_one(pool).await.unwrap();
        if n >= expected || std::time::Instant::now() >= deadline {
            return n;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

// ── GAP 1 ────────────────────────────────────────────────────────────────
// An obs-wired backtest executor persists spans into SQLite, and
// `build_export` returns a NON-empty spans tree. This is the half the
// CLI ctx wiring unlocks: with a real bus + SqliteRecorder, a real eval
// produces a populated trace export instead of `spans: []`.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn obs_wired_backtest_executor_persists_spans_to_sqlite() {
    let store = fresh_store().await;
    let pool = store.pool().clone();
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTOBSEXPORTCHAIN00000A";
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // A small deterministic bar series: open → holds. The open fill drives
    // the agent.decision + decision.model + broker.call span set.
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let bars: Vec<Ohlcv> = (0..6)
        .map(|i| Ohlcv {
            timestamp: start + Duration::days(i as i64),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1_000.0,
        })
        .collect();
    let dispatch: Arc<dyn LlmDispatch> =
        sequenced_dispatch(&["long_open", "hold", "hold", "hold", "hold", "hold"]);
    let tools = Arc::new(ToolRegistry::empty());

    // The REAL persistence chain: bus → SqliteRecorder → the same pool the
    // store (and `build_export`) read from. No NoopRecorder, no synthetic
    // events.
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));
    let obs = ObsEmitter::new(bus.clone(), run.id.clone());

    // The eval path opens the RunStarted lifecycle before the executor
    // runs (so the agent_runs FK row exists); mirror that here.
    obs.emit_run_started("obs export chain", "full_debug").await;

    let executor = Executor::with_bars(bars).with_observability(obs.clone());
    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    obs.emit_run_finished(RunStatus::Completed, None).await;

    // Deterministically drain every published event to the recorder before
    // reading back — mirrors the CLI's post-run flush.
    bus.quiesce().await;
    let span_count = wait_for_count(&pool, "SELECT COUNT(*) FROM spans", 2).await;
    assert!(
        span_count >= 2,
        "an obs-wired backtest must persist spans to SQLite (got {span_count}); \
         a real eval should never show `spans: []`"
    );

    // `build_export` over the SAME pool must return a non-empty spans tree
    // with at least an agent.run root + an agent.decision node.
    let export = xvision_observability::build_export(&pool, &run.id)
        .await
        .expect("build_export must succeed");
    assert!(
        !export.spans.is_empty(),
        "build_export must return a non-empty spans tree for a real eval run"
    );

    // Flatten the span tree and confirm the canonical decision span set the
    // trace dock renders. (The run itself lives in `agent_runs`, not as a
    // span row, so the tree is rooted at the per-bar decision spans.)
    let kinds = flatten_kinds(&export.spans);
    assert!(
        kinds.iter().any(|k| k == "agent.decision"),
        "export must contain at least one agent.decision span; kinds = {kinds:?}"
    );
    assert!(
        kinds.iter().any(|k| k == "decision.model"),
        "export must contain at least one decision.model span (the model \
         invocation the trace dock surfaces); kinds = {kinds:?}"
    );
}

fn flatten_kinds(nodes: &[xvision_observability::SpanNode]) -> Vec<String> {
    let mut out = Vec::new();
    for n in nodes {
        out.push(n.row.kind.clone());
        out.extend(flatten_kinds(&n.children));
    }
    out
}

// ── GAP 3 — batch-path obs regression (`fix/eval-obs-batch`) ───────────────
//
// `xvn eval batch` is a SEPARATE CLI entry from `xvn eval run`: it built its
// `ApiContext` via a bus-less `open_ctx` and looped `eval::run_with_deps` per
// scenario. Because the batch ctx left `obs_event_bus: None`, `run_inner`'s
// `obs_emitter` was `None` and EVERY `emit_*` was a silent no-op — so the
// SAME root cause as GAP 1 produced TWO visible symptoms for batch runs:
//
//   Bug 1 — no spans were emitted/persisted (the trace dock was blank).
//   Bug 2 — no `RunFinished` event fired, so the obs recorder never
//           transitioned `agent_runs.status` from its
//           `ensure_agent_run_baseline` default `'running'` to `'completed'`;
//           the runs looked stuck forever.
//
// The fix wires a `RunEventBus` + `SqliteRecorder` onto the batch ctx
// (mirroring the single-run `wire_obs_bus`) and `quiesce()`s before exit.
//
// `run_with_deps` itself can't be driven cheaply here (it now demands a
// launchable Cline runtime — the whole `api_eval_run.rs` surface is red on
// this tree for exactly that reason). So these two tests pin BOTH halves at
// the lowest faithful layer: the SAME lifecycle `run_inner` drives —
// `ensure_agent_run_baseline` → `emit_run_started` → executor span emit →
// `emit_run_finished(Completed)` → bus → `SqliteRecorder` → SQLite — gated on
// the bus exactly as `run_inner` gates it. No fabricated spans, no
// hand-published synthetic events.
//
// The positive test asserts the fix's effect; the negative test omits the
// bus (the pre-fix batch ctx) and proves BOTH symptoms, so the positive
// assertions are not vacuously true.

/// Build the deterministic backtest fixture the lifecycle tests share: a
/// flash-crash scenario, a small hold-after-open bar series, and a sequenced
/// dispatch. Returns the store, the queued run, the strategy, scenario,
/// bars, dispatch, and tools.
async fn lifecycle_fixture() -> (
    xvision_engine::eval::store::RunStore,
    Run,
    xvision_engine::strategies::Strategy,
    xvision_engine::eval::scenario::Scenario,
    Vec<Ohlcv>,
    Arc<dyn LlmDispatch>,
    Arc<ToolRegistry>,
) {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTOBSBATCHLIFECYCLE0000A";
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let bars: Vec<Ohlcv> = (0..6)
        .map(|i| Ohlcv {
            timestamp: start + Duration::days(i as i64),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1_000.0,
        })
        .collect();
    let dispatch: Arc<dyn LlmDispatch> =
        sequenced_dispatch(&["long_open", "hold", "hold", "hold", "hold", "hold"]);
    let tools = Arc::new(ToolRegistry::empty());
    (store, run, strategy, scenario, bars, dispatch, tools)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn obs_wired_eval_lifecycle_records_spans_and_completes_agent_run() {
    // The faithful batch path with the bus wired (the fix's effect): the
    // exact emit sequence `run_inner` performs when `ctx.obs_event_bus` is
    // `Some` — baseline → RunStarted → executor spans → RunFinished(Completed).
    let (store, mut run, strategy, scenario, bars, dispatch, tools) = lifecycle_fixture().await;
    let pool = store.pool().clone();

    // `run_inner` opens the agent_runs baseline (status 'running') BEFORE the
    // obs lifecycle; mirror that so the RunFinished UPDATE has a row to flip.
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();

    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));
    let obs = ObsEmitter::new(bus.clone(), run.id.clone());

    obs.emit_run_started("eval:Backtest:flash-crash", "hash_only")
        .await;

    let executor = Executor::with_bars(bars).with_observability(obs.clone());
    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    // The success path's terminal event — the half the batch ctx was missing.
    obs.emit_run_finished(RunStatus::Completed, None).await;

    // Drain the bus to SQLite — the CLI's post-run `quiesce`.
    bus.quiesce().await;

    // Bug 1: a non-empty spans tree must persist (not `spans: []`).
    let span_count = wait_for_count(&pool, "SELECT COUNT(*) FROM spans", 2).await;
    assert!(
        span_count >= 2,
        "an obs-wired eval lifecycle must persist spans (got {span_count})"
    );
    let export = xvision_observability::build_export(&pool, &run.id)
        .await
        .expect("build_export must succeed");
    assert!(
        !export.spans.is_empty(),
        "build_export must return a non-empty spans tree"
    );
    let kinds = flatten_kinds(&export.spans);
    assert!(
        kinds.iter().any(|k| k == "agent.decision"),
        "export must contain at least one agent.decision span; kinds = {kinds:?}"
    );

    // Bug 2: the RunFinished(Completed) event must flip agent_runs.status from
    // the baseline 'running' to 'completed'. (The agent.run lifecycle lives in
    // `agent_runs`, not as a span row, so assert it directly.)
    let status = wait_for_status(&pool, &run.id, "completed").await;
    assert_eq!(
        status.as_deref(),
        Some("completed"),
        "RunFinished(Completed) must transition agent_runs.status to 'completed'; \
         a stuck 'running' is the batch-runs-look-stuck bug"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_bus_eval_lifecycle_leaves_run_stuck_running_and_spans_empty() {
    // Negative control: the SAME run with NO obs bus reproduces BOTH bugs —
    // the pre-fix batch ctx (`open_ctx` leaves `obs_event_bus: None`, so the
    // engine builds no `ObsEmitter` and every `emit_*` is skipped). This
    // proves the positive test above is not vacuously passing.
    let (store, mut run, strategy, scenario, bars, dispatch, tools) = lifecycle_fixture().await;
    let pool = store.pool().clone();

    // `run_inner` writes the baseline (status 'running') unconditionally, even
    // on a bus-less ctx — so the row exists and is stuck, never absent.
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();

    // NO ObsEmitter is attached — exactly what a bus-less ctx yields. The
    // executor runs without observability, so it emits no spans.
    let executor = Executor::with_bars(bars);
    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete even without an obs bus");

    // Bug 1, pre-fix: no spans were ever emitted, so none persist.
    let span_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spans")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        span_count, 0,
        "a bus-less eval emits no spans — the trace-dock-blank bug"
    );
    let export = xvision_observability::build_export(&pool, &run.id)
        .await
        .expect("build_export must succeed");
    assert!(
        export.spans.is_empty(),
        "a bus-less eval must have an empty spans tree"
    );

    // Bug 2, pre-fix: with no RunFinished event the baseline stays 'running'.
    let (status,): (String,) = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
        .bind(&run.id)
        .fetch_one(&pool)
        .await
        .expect("the unconditional baseline row must exist");
    assert_eq!(
        status, "running",
        "without the obs bus the RunFinished event never fires, so the \
         agent_runs baseline stays stuck at 'running'"
    );
}

/// Poll `agent_runs.status` until it reaches `expected` (or times out). The
/// recorder applies `RunFinished` on the bus's background task, so we wait
/// for the transition rather than racing the drain.
async fn wait_for_status(pool: &SqlitePool, run_id: &str, expected: &str) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(pool)
            .await
            .unwrap();
        if status.as_deref() == Some(expected) || std::time::Instant::now() >= deadline {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

// ── GAP 2 ────────────────────────────────────────────────────────────────
// The Cline trader success path emits `ModelCallFinished`, so a
// `model_calls` row lands with the sidecar's reported token usage.

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

async fn spawn_mock(cfg: serde_json::Value) -> (AgentClient, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&cfg).unwrap(),
    )
    .expect("write cfg");
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)");
    (client, dir)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cline_slot_success_emits_model_call_finished_row() {
    // SQLite over the obs schema (migrations 002 + 013 + 018), the same
    // pool the recorder writes to and `build_export` reads.
    let store = fresh_store().await;
    let pool = store.pool().clone();

    let run_id = "cycle-obs::trader".to_string();
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));
    let obs = ObsEmitter::new(bus.clone(), run_id.clone());

    // The backtest executor opens the RunStarted lifecycle + a
    // `decision.model` span and threads its id into the Cline input
    // (`backtest.rs` ~1801). Reproduce that threading so the emitted
    // ModelCallFinished attaches to a real span id.
    obs.emit_run_started("cline model_call row", "full_debug").await;
    let model_call_span_id = fresh_span_id();
    obs.emit_model_call_started(
        &model_call_span_id,
        None,
        "anthropic",
        "claude-sonnet-4-6",
        Some("trader"),
        None,
        None,
    )
    .await;

    // Drive one trader slot through the mock sidecar — it returns
    // usage {input_tokens: 11, output_tokens: 7}.
    let (client, _dir) = spawn_mock(json!({
        "decisionJson": r#"{"action":"long_open","conviction":0.8,"justification":"mock"}"#
    }))
    .await;
    let client = Arc::new(client);

    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let entry = ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-sonnet-4-6".into()],
    };

    let input = ClineSlotInput {
        slot: &slot,
        provider_entry: &entry,
        api_key: Some("test-key".into()),
        system_prompt: "Decide whether to trade.".into(),
        upstream_inputs: json!({"market_data": {"bar_history": [{"c": 100.0}]}}),
        response_schema: ResponseSchema::trader_output(),
        allowed_tools: vec!["indicators.rsi".into()],
        max_tokens: Some(4096),
        max_wall_ms: None,
        run_id: run_id.clone(),
        cline_client: client.clone(),
        trajectory_mode: TrajectoryMode::Record,
        record_slot_role: None,
        obs: Some(obs.clone()),
        model_call_span_id: Some(model_call_span_id.clone()),
        reasoning_effort: None,
    };

    let resp = execute_slot_cline(input)
        .await
        .expect("cline slot must produce an LlmResponse");
    assert_eq!(resp.input_tokens, 11);
    assert_eq!(resp.output_tokens, 7);

    // Shut the mock sidecar down now we have the response, so its node IPC
    // reader task stops competing with the bus consumer for the test
    // runtime's worker threads (otherwise the consumer can be starved and
    // the drain below times out).
    Arc::try_unwrap(client).ok().unwrap().shutdown().await.unwrap();

    obs.emit_span_finished_ok(&model_call_span_id).await;
    obs.emit_run_finished(RunStatus::Completed, None).await;

    // Deterministically drain every published event to the recorder — the
    // same flush `xvn eval run` performs before exit. `quiesce` guarantees
    // the `ModelCallFinished` has been handled, so the row is present.
    bus.quiesce().await;

    // A model_calls row must land, carrying the sidecar's token usage.
    let n = wait_for_count(&pool, "SELECT COUNT(*) FROM model_calls", 1).await;
    assert!(
        n >= 1,
        "the Cline trader success path must write a model_calls row \
         (got {n}); without it `model_calls: 0` even with the bus wired"
    );

    let (provider, model, in_tok, out_tok): (String, String, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT provider, model, input_token_count, output_token_count \
             FROM model_calls WHERE span_id = ?",
    )
    .bind(&model_call_span_id)
    .fetch_one(&pool)
    .await
    .expect("the model_calls row must key on the threaded model_call span id");
    assert_eq!(provider, "anthropic");
    assert_eq!(model, "claude-sonnet-4-6");
    assert_eq!(in_tok, Some(11), "input tokens must come from step.usage");
    assert_eq!(out_tok, Some(7), "output tokens must come from step.usage");
}

// ── GAP 3 ────────────────────────────────────────────────────────────────
// The trace inspector showed only `prompt.hash` / `response.hash` even under
// `full_debug` retention because the Cline trader emit was the HASH-ONLY
// variant (`emit_model_call_finished`) which hard-codes
// `prompt_text: None` / `response_text: None`. The success path now emits
// `emit_model_call_finished_with_payloads`, so under `full_debug` (with a
// BlobStore wired) the `model_calls` row carries `prompt_payload_ref` +
// `response_payload_ref`, and `build_export_with_blobs` reconstructs the
// operator-visible prompt + response TEXT — not just the hash.
//
// Two retention modes are exercised through the REAL emit → bus → recorder →
// blob → export chain (no hand-written payload rows):
//   * `full_debug` → both refs land, export resolves non-empty prompt +
//     response text. FAILS before the fix (hash-only emit writes no refs).
//   * `hash_only`  → hashes record, but NO payload refs (gating still holds).

/// A retention policy with both store flags ON at the given mode, mirroring
/// `agent_observability_blob.rs::policy_for`. Under `HashOnly` the store
/// flags are moot — the emitter never writes bodies regardless.
fn policy_for(mode: RetentionMode) -> ObsRetentionPolicy {
    let mut cfg = ObservabilityConfig::default();
    cfg.retention.mode = mode;
    cfg.retention.store_prompts = true;
    cfg.retention.store_responses = true;
    cfg.retention.max_payload_bytes = 200_000;
    ObsRetentionPolicy::from_config(&cfg)
}

/// Drive one trader slot through the mock sidecar with `obs` wired (already
/// carrying retention + blob store), returning the `model_call` span id the
/// emit keys on. Mirrors `cline_slot_success_emits_model_call_finished_row`'s
/// setup so both tests share the real execute_slot_cline path.
async fn drive_cline_slot(run_id: &str, obs: &ObsEmitter) -> String {
    let model_call_span_id = fresh_span_id();
    obs.emit_run_started("cline payload row", "full_debug").await;
    obs.emit_model_call_started(
        &model_call_span_id,
        None,
        "anthropic",
        "claude-sonnet-4-6",
        Some("trader"),
        None,
        None,
    )
    .await;

    let (client, _dir) = spawn_mock(json!({
        "decisionJson": r#"{"action":"long_open","conviction":0.8,"justification":"mock"}"#
    }))
    .await;
    let client = Arc::new(client);

    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let entry = ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-sonnet-4-6".into()],
    };

    let input = ClineSlotInput {
        slot: &slot,
        provider_entry: &entry,
        api_key: Some("test-key".into()),
        system_prompt: "Decide whether to trade.".into(),
        upstream_inputs: json!({"market_data": {"bar_history": [{"c": 100.0}]}}),
        response_schema: ResponseSchema::trader_output(),
        allowed_tools: vec!["indicators.rsi".into()],
        max_tokens: Some(4096),
        max_wall_ms: None,
        run_id: run_id.to_string(),
        cline_client: client.clone(),
        trajectory_mode: TrajectoryMode::Record,
        record_slot_role: None,
        obs: Some(obs.clone()),
        model_call_span_id: Some(model_call_span_id.clone()),
        reasoning_effort: None,
    };

    execute_slot_cline(input)
        .await
        .expect("cline slot must produce an LlmResponse");

    // Shut the mock sidecar down so its IPC reader stops competing for the
    // runtime's worker threads (otherwise the bus drain below can starve).
    Arc::try_unwrap(client).ok().unwrap().shutdown().await.unwrap();

    obs.emit_span_finished_ok(&model_call_span_id).await;
    obs.emit_run_finished(RunStatus::Completed, None).await;
    model_call_span_id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cline_slot_full_debug_records_prompt_and_response_payloads() {
    let store = fresh_store().await;
    let pool = store.pool().clone();
    let blob_dir = TempDir::new().expect("blob tempdir");
    let blob = BlobStore::new(blob_dir.path());

    let run_id = "cycle-obs-payload::trader".to_string();
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));
    let obs = ObsEmitter::new(bus.clone(), run_id.clone())
        .with_retention(policy_for(RetentionMode::FullDebug))
        .with_blob_store(blob.clone());

    let span_id = drive_cline_slot(&run_id, &obs).await;

    bus.quiesce().await;
    wait_for_count(&pool, "SELECT COUNT(*) FROM model_calls", 1).await;

    // The model_calls row must carry BOTH payload refs under full_debug —
    // proving the Cline emit switched to the payload-aware variant.
    let (prompt_ref, response_ref): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT prompt_payload_ref, response_payload_ref \
             FROM model_calls WHERE span_id = ?",
    )
    .bind(&span_id)
    .fetch_one(&pool)
    .await
    .expect("model_calls row must key on the threaded model_call span id");
    let prompt_ref = prompt_ref.expect(
        "full_debug Cline trader run must populate prompt_payload_ref \
         (else the inspector falls back to showing prompt.hash)",
    );
    let response_ref = response_ref.expect(
        "full_debug Cline trader run must populate response_payload_ref \
         (else the inspector falls back to showing response.hash)",
    );
    assert!(!prompt_ref.is_empty());
    assert!(!response_ref.is_empty());

    // The operator-visible outcome: build_export_with_blobs reconstructs the
    // actual prompt + response TEXT for the model call, not just the hash.
    let export = xvision_observability::build_export_with_blobs(&pool, &run_id, Some(&blob))
        .await
        .expect("build_export_with_blobs must succeed");
    let mc = export
        .model_calls
        .iter()
        .find(|m| m.span_id == span_id)
        .expect("export must contain the trader model call");

    let prompt_text = mc
        .prompt_text
        .as_deref()
        .expect("export must inline non-null prompt text from the blob");
    let response_text = mc
        .response_text
        .as_deref()
        .expect("export must inline non-null response text from the blob");

    // The persisted prompt blob is the canonical LlmRequest JSON; it must
    // carry the slot's system prompt + the rendered user turn (the bytes the
    // operator wants to read instead of `sha256:…`).
    assert!(
        prompt_text.contains("Decide whether to trade."),
        "prompt blob must include the slot system prompt; got: {prompt_text}"
    );
    assert!(
        prompt_text.contains("submit_decision"),
        "prompt blob must include the rendered user instructions; got: {prompt_text}"
    );
    // The response blob is the decision JSON the agent submitted.
    assert!(
        response_text.contains("long_open"),
        "response blob must be the submitted decision JSON; got: {response_text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cline_slot_hash_only_records_no_payload_refs() {
    let store = fresh_store().await;
    let pool = store.pool().clone();
    let blob_dir = TempDir::new().expect("blob tempdir");
    let blob = BlobStore::new(blob_dir.path());

    let run_id = "cycle-obs-hashonly::trader".to_string();
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));
    let obs = ObsEmitter::new(bus.clone(), run_id.clone())
        .with_retention(policy_for(RetentionMode::HashOnly))
        .with_blob_store(blob.clone());

    let span_id = drive_cline_slot(&run_id, &obs).await;

    bus.quiesce().await;
    wait_for_count(&pool, "SELECT COUNT(*) FROM model_calls", 1).await;

    // hash_only: the row records, hashes are present, but NO payload refs —
    // proving the emitter still gates bodies under retention.
    let (prompt_hash, prompt_ref, response_ref): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT prompt_hash, prompt_payload_ref, response_payload_ref \
                 FROM model_calls WHERE span_id = ?",
    )
    .bind(&span_id)
    .fetch_one(&pool)
    .await
    .expect("model_calls row must exist even under hash_only");
    assert!(
        prompt_hash.starts_with("sha256:"),
        "hash_only must still record the prompt hash; got: {prompt_hash}"
    );
    assert!(
        prompt_ref.is_none(),
        "hash_only must NOT populate prompt_payload_ref; got: {prompt_ref:?}"
    );
    assert!(
        response_ref.is_none(),
        "hash_only must NOT populate response_payload_ref; got: {response_ref:?}"
    );

    // No blob bytes written under hash_only.
    let entries: Vec<_> = std::fs::read_dir(blob_dir.path())
        .map(|d| d.flatten().collect())
        .unwrap_or_default();
    assert!(
        entries.is_empty(),
        "hash_only must not write blobs; found: {entries:?}"
    );
}
