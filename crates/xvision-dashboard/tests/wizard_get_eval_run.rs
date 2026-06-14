//! W4 — `get_eval_run` chat-tool parity on failed / null-metric runs.
//!
//! Finding #4: the chat tool `get_eval_run` called `api_eval::get` which
//! returned the raw `Run` struct. For failed runs the nested
//! `metrics: Option<MetricsSummary>` field is `None` and serialises as
//! `"metrics": null`, which gives the model an opaque null blob instead
//! of the flat, agent-readable `RunSummary` shape the REST endpoint already
//! returns. The model circuit-breaker then fires because it cannot extract
//! any usable field from the result.
//!
//! These tests pin the *correct* behaviour:
//!
//! 1. A **failed** run (metrics = None, error set) must produce a well-formed
//!    tool result with `status = "failed"`, the error string present, and all
//!    metric fields absent / null at the top level — NOT `"metrics": null` in
//!    a nested blob (the raw `Run` shape).
//!
//! 2. A **missing** id must produce a typed not-found message (contains
//!    "not found" or the id) so the model can report the problem ONCE rather
//!    than retrying into the circuit-breaker.
//!
//! 3. A **completed** run's tool result must carry the same core fields
//!    (`status`, `sharpe`, `total_return_pct`, `max_drawdown_pct`) as the
//!    REST `RunSummary` shape — flat at the top level, NOT nested under
//!    `"metrics"`.

use std::sync::Arc;

use tempfile::TempDir;
use xvision_dashboard::wizard_loop::{WizardEvent, WizardLoop};
use xvision_dashboard::AppState;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};
use xvision_engine::eval::run::{MetricsSummary, Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;

// ── helpers ──────────────────────────────────────────────────────────────────

async fn boot() -> (AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init AppState");
    (state, tmp)
}

/// Drain all events from the wizard loop and return them.
async fn drain(wl: &mut WizardLoop) -> Vec<WizardEvent> {
    let mut out = vec![];
    while let Some(ev) = wl.next_event().await {
        out.push(ev);
    }
    out
}

/// Extract the first `ToolResult` event for the given tool name.
fn first_tool_result<'a>(events: &'a [WizardEvent], tool: &str) -> Option<&'a serde_json::Value> {
    events.iter().find_map(|ev| match ev {
        WizardEvent::ToolResult { tool: t, result, .. } if t == tool => Some(result),
        _ => None,
    })
}

// ── test 1: failed run returns well-formed flat summary ───────────────────────

/// A run in `Failed` state (metrics = None, error set) must produce a
/// tool result that:
///   - Is a well-formed JSON object with top-level `"id"` and `"status"`.
///   - Has `status = "failed"`.
///   - Has the run's `error` string at the top level.
///   - Does NOT have a nested `"metrics"` key — the RunSummary shape is flat.
///
/// Before the fix, `get_eval_run` returned the raw `Run` struct which
/// serialises `metrics: None` as `"metrics": null`, an opaque blob the
/// model could not reason over.
#[tokio::test]
async fn get_eval_run_failed_returns_well_formed_summary() {
    let (state, tmp) = boot().await;

    // Seed a failed run with no metrics.
    let store = RunStore::new(state.pool.clone());
    let run = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let run_id = run.id.clone();
    store.create(&run).await.unwrap();
    store
        .update_status(&run_id, RunStatus::Failed, Some("market data timeout"))
        .await
        .unwrap();

    // Script the model: (1) call get_eval_run, (2) emit a done text.
    let mock: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use("tu_1", "get_eval_run", serde_json::json!({ "id": run_id })),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: "This run failed with: market data timeout".into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 20,
        },
    ]));

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "what happened to my latest run?".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let result = first_tool_result(&events, "get_eval_run").expect("get_eval_run ToolResult must be present");

    // Must be a well-formed RunSummary object: has "id" at top level.
    // A handler-level error would be {"error": "..."} with no "id".
    assert!(
        result.get("id").is_some(),
        "get_eval_run on a failed run must return a RunSummary object (has 'id'); \
         got: {result}"
    );
    assert_eq!(
        result["id"].as_str().unwrap(),
        run_id,
        "result must carry the run id"
    );

    // status must be "failed".
    assert_eq!(
        result["status"].as_str().unwrap(),
        "failed",
        "status must be 'failed'"
    );

    // The run's error string must be present at the top level.
    let error_val = result
        .get("error")
        .expect("error field must be present in RunSummary for a failed run");
    let error_str = error_val.as_str().expect("error must be a string");
    assert!(
        error_str.contains("market data timeout") || error_str.contains("timeout"),
        "error field must reflect the failure reason; got: {error_str:?}"
    );

    // Must NOT have a nested "metrics" key — that is the raw Run struct shape.
    // RunSummary exposes metric values as flat top-level fields instead.
    assert!(
        result.get("metrics").is_none(),
        "get_eval_run must return the flat RunSummary shape, not a nested 'metrics' blob; \
         got: {result}"
    );
}

// ── test 2: missing run returns a clean not-found descriptor ─────────────────

/// When the run id does not exist, `get_eval_run` must return a tool result
/// that is a JSON object with an `"error"` key whose value is a descriptive
/// string — NOT a bare null or an empty object.
///
/// The message must reference "not found" (or the id) so the model can
/// report it clearly ONCE rather than retrying into the circuit-breaker.
#[tokio::test]
async fn get_eval_run_missing_id_returns_descriptive_not_found() {
    let (state, tmp) = boot().await;
    let missing_id = "01HNOTEXISTRUN999999999999";

    let mock: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use("tu_1", "get_eval_run", serde_json::json!({ "id": missing_id })),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: "That run could not be found.".into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 20,
        },
    ]));

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "show me run 01HNOTEXISTRUN999999999999".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let result = first_tool_result(&events, "get_eval_run").expect("get_eval_run ToolResult must be present");

    // Must be a JSON object (not null).
    assert!(
        result.is_object(),
        "tool result for missing run must be a JSON object, not null; got: {result}"
    );

    // For a not-found id, the wizard wraps the error as {"error": "<msg>"}.
    let err_val = result
        .get("error")
        .expect("not-found result must have an 'error' key");
    let err_str = err_val
        .as_str()
        .expect("error value must be a string, not an object or null");

    assert!(!err_str.is_empty(), "error string must not be empty");

    // The error message must be diagnostic: reference the id or say "not found".
    assert!(
        err_str.contains(missing_id) || err_str.to_lowercase().contains("not found"),
        "not-found error must reference the missing id or say 'not found'; got: {err_str:?}"
    );
}

// ── test 3: completed run shape parity with REST RunSummary ──────────────────

/// A completed run's tool result must contain the same core metric fields as
/// the REST `GET /api/eval/runs/:id` → `RunDetail.summary` → `RunSummary`:
///   `status`, `sharpe`, `total_return_pct`, `max_drawdown_pct`
/// These must be at the top level (flat shape), NOT nested under `"metrics"`.
#[tokio::test]
async fn get_eval_run_completed_has_flat_metric_fields() {
    let (state, tmp) = boot().await;

    let store = RunStore::new(state.pool.clone());
    let run = Run::new_queued("agent-y".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let run_id = run.id.clone();
    store.create(&run).await.unwrap();

    // Finalize the run with metrics (sets status = completed + writes metrics_json).
    let metrics = MetricsSummary {
        sharpe: 1.23,
        max_drawdown_pct: 5.0,
        total_return_pct: 42.0,
        win_rate: 0.6,
        n_trades: 20,
        n_decisions: 100,
        inference_cost_quote_total: None,
        net_return_pct: None,
        baselines: None,
    };
    store.finalize(&run_id, &metrics).await.unwrap();

    let mock: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use("tu_1", "get_eval_run", serde_json::json!({ "id": run_id })),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: "The completed run has sharpe 1.23.".into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 20,
        },
    ]));

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "how did the latest run perform?".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let result = first_tool_result(&events, "get_eval_run").expect("get_eval_run ToolResult must be present");

    // Must be a well-formed RunSummary (has "id").
    assert!(
        result.get("id").is_some(),
        "completed run result must be a RunSummary object; got: {result}"
    );

    assert_eq!(result["status"].as_str().unwrap(), "completed");

    // Metric fields must be flat — NOT nested under "metrics".
    assert!(
        result.get("metrics").is_none(),
        "result must use flat RunSummary shape, not nested 'metrics' key; got: {result}"
    );
    assert!(
        result.get("sharpe").is_some(),
        "sharpe must be a top-level field; got: {result}"
    );
    assert!(
        result.get("total_return_pct").is_some(),
        "total_return_pct must be a top-level field; got: {result}"
    );
    assert!(
        result.get("max_drawdown_pct").is_some(),
        "max_drawdown_pct must be a top-level field; got: {result}"
    );

    // Values must match what we finalized.
    let sharpe = result["sharpe"].as_f64().unwrap();
    assert!(
        (sharpe - 1.23).abs() < 0.001,
        "sharpe should be ~1.23; got {sharpe}"
    );
    let total_return = result["total_return_pct"].as_f64().unwrap();
    assert!(
        (total_return - 42.0).abs() < 0.001,
        "total_return_pct should be ~42.0; got {total_return}"
    );
}
