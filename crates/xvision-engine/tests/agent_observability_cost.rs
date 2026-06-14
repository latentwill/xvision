//! Regression for `model-call-cost-usd-population` (F-11 sub).
//!
//! Before this track: every `emit_model_call_finished*` call site
//! passed `cost_usd: None`, so the `model_calls.cost_usd` column was
//! NULL for 2,757 of 2,757 audited rows even when the operator had a
//! priced catalog (OpenRouter) cached on disk. The emit-side wiring
//! now consults the catalogs threaded onto `ObsEmitter` via
//! `with_catalogs(...)` and resolves cost from `(input_tokens *
//! pricing_in + output_tokens * pricing_out) / 1_000_000`, matching
//! the canonical formula in `xvision_engine::eval::cost`.
//!
//! Three properties exercised here:
//!
//! 1. With a priced catalog wired, `emit_model_call_finished` publishes
//!    a `ModelCallFinished` event whose `cost_usd` equals a direct
//!    `compute_token_cost_usd_from_catalog` call on the same inputs.
//! 2. With no priced catalog for the requested model, `cost_usd` stays
//!    `None` and the (provider, model) pair is logged at most once per
//!    process — verified via repeated emits.
//! 3. The payload-bearing companion
//!    (`emit_model_call_finished_with_payloads`) follows the exact
//!    same resolution rule.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde_json::Value;
use tracing_subscriber::prelude::*;
use xvision_core::providers::{Catalog, ModelEntry};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::eval::cost::compute_token_cost_usd_from_catalog;
use xvision_observability::{
    AgentRunRecorder, ModelCallFinishedEvent, NoopRecorder, RunEvent, RunEventBus, RunStartedEvent,
    SpanFinishedEvent, SpanKind, SpanStartedEvent, SpanStatus, SqliteRecorder,
};

const UNPRICED_LOG_PREFIX: &str = "model_calls.cost_usd: no priced catalog entry";

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnpricedLog {
    provider: String,
    model: String,
    message: String,
}

#[derive(Clone, Default)]
struct CapturedUnpricedLogs {
    entries: Arc<Mutex<Vec<UnpricedLog>>>,
}

struct UnpricedLogLayer {
    captured: CapturedUnpricedLogs,
}

impl<S> tracing_subscriber::Layer<S> for UnpricedLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let mut visitor = UnpricedLogVisitor::default();
        event.record(&mut visitor);
        if visitor
            .message
            .as_deref()
            .is_some_and(|message| message.starts_with(UNPRICED_LOG_PREFIX))
        {
            self.captured.entries.lock().unwrap().push(UnpricedLog {
                provider: visitor.provider.unwrap_or_default(),
                model: visitor.model.unwrap_or_default(),
                message: visitor.message.unwrap_or_default(),
            });
        }
    }
}

#[derive(Default)]
struct UnpricedLogVisitor {
    provider: Option<String>,
    model: Option<String>,
    message: Option<String>,
}

impl tracing::field::Visit for UnpricedLogVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "provider" => self.provider = Some(value.to_string()),
            "model" => self.model = Some(value.to_string()),
            "message" => self.message = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let value = format!("{value:?}").trim_matches('"').to_string();
        match field.name() {
            "provider" => self.provider = Some(value),
            "model" => self.model = Some(value),
            "message" => self.message = Some(value),
            _ => {}
        }
    }
}

/// OpenRouter-shaped Claude Opus 4.7 entry. Matches the fixture in
/// `xvision_engine::eval::cost::tests` so the asserted cost stays in
/// sync with the canonical-pricing test there.
fn priced_opus_entry() -> ModelEntry {
    ModelEntry {
        id: "anthropic/claude-opus-4.7".into(),
        display_name: Some("Anthropic: Claude Opus 4.7".into()),
        context_window: Some(200_000),
        max_output_tokens: Some(8192),
        supports_reasoning: None,
        supports_tools: Some(true),
        pricing_per_million_input_usd: Some(15.0),
        pricing_per_million_output_usd: Some(75.0),
        raw: Value::Null,
    }
}

/// A catalog with no priced rows — same shape Anthropic's bare
/// `/v1/models` endpoint surfaces (ids only, no `pricing`).
fn unpriced_anthropic_entry() -> ModelEntry {
    ModelEntry {
        id: "claude-sonnet-4-6".into(),
        display_name: Some("Claude Sonnet 4.6".into()),
        context_window: Some(200_000),
        max_output_tokens: Some(8192),
        supports_reasoning: None,
        supports_tools: Some(true),
        pricing_per_million_input_usd: None,
        pricing_per_million_output_usd: None,
        raw: Value::Null,
    }
}

fn catalog(provider: &str, models: Vec<ModelEntry>) -> Arc<Catalog> {
    Arc::new(Catalog {
        provider: provider.into(),
        fetched_at: Utc::now(),
        source_url: "test://catalog".into(),
        models,
    })
}

/// Yield-and-drain helper. Same pattern as
/// `agent_observability_blob::collect_events`.
async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

fn finished_events(events: &[RunEvent]) -> Vec<&ModelCallFinishedEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::ModelCallFinished(m) => Some(m),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn priced_model_resolves_cost_via_wired_catalog() {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let cat = catalog("openrouter", vec![priced_opus_entry()]);
    let mut catalogs = HashMap::new();
    catalogs.insert("openrouter".to_string(), cat.clone());

    let emitter = ObsEmitter::new(bus.clone(), "run-priced").with_catalogs(catalogs);

    // 10_000 prompt + 2_000 completion tokens — same mix the
    // canonical-pricing test in `eval::cost` uses ($0.30 total at
    // OpenRouter Claude Opus 4.7 rates).
    emitter
        .emit_model_call_finished(
            "span-priced",
            "openrouter",
            "anthropic/claude-opus-4.7",
            Some(10_000),
            Some(2_000),
            None,
            "sha256:test".to_string(),
            Some("sha256:test".to_string()),
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let finished = finished_events(&events);
    assert_eq!(finished.len(), 1, "exactly one ModelCallFinished expected");
    let m = finished[0];

    let expected = compute_token_cost_usd_from_catalog(10_000, 2_000, "anthropic/claude-opus-4.7", &cat)
        .expect("priced model resolves via canonical helper");
    let actual = m.cost_usd.expect("priced model produces Some(cost_usd)");
    assert!(
        (actual - expected).abs() < 1e-12,
        "emit-side cost_usd ({actual}) must match canonical compute_token_cost_usd_from_catalog ({expected})",
    );
    // Direct numeric sanity: $0.15 (input) + $0.15 (output) = $0.30
    assert!((actual - 0.30).abs() < 1e-9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn priced_model_via_provider_fallback_when_slot_provider_string_mismatches() {
    // The slot's `provider` field can be operator-typed and won't
    // always match `ProviderEntry.name`. The emitter falls back to
    // scanning every wired catalog by model id so cost still resolves.
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let cat = catalog("openrouter", vec![priced_opus_entry()]);
    let mut catalogs = HashMap::new();
    catalogs.insert("openrouter".to_string(), cat.clone());

    let emitter = ObsEmitter::new(bus.clone(), "run-priced-fallback").with_catalogs(catalogs);

    // Slot reports `provider = "anthropic"` but the priced catalog
    // lives under "openrouter". The fallback scan must still find the
    // model id and produce a non-None cost.
    emitter
        .emit_model_call_finished(
            "span-fallback",
            "anthropic",
            "anthropic/claude-opus-4.7",
            Some(1_000),
            Some(1_000),
            None,
            "sha256:test".to_string(),
            None,
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let m = finished_events(&events).into_iter().next().unwrap();
    assert!(
        m.cost_usd.is_some(),
        "fallback scan should locate the priced model and populate cost_usd",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unpriced_model_leaves_cost_none_and_repeats_dont_panic() {
    // Anthropic catalogs land here (no pricing on the wire). Two
    // back-to-back emits with the same (provider, model) must both
    // resolve to `cost_usd = None` and produce at most one unpriced
    // debug log for that provider/model pair.
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let cat = catalog("anthropic", vec![unpriced_anthropic_entry()]);
    let mut catalogs = HashMap::new();
    catalogs.insert("anthropic".to_string(), cat);

    let emitter = ObsEmitter::new(bus.clone(), "run-unpriced").with_catalogs(catalogs);
    let model = format!("claude-sonnet-4-6-unpriced-{}", std::process::id());
    let captured = CapturedUnpricedLogs::default();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::DEBUG)
        .with(UnpricedLogLayer {
            captured: captured.clone(),
        });
    let _subscriber_guard = tracing::subscriber::set_default(subscriber);

    for span in ["span-a", "span-b"] {
        emitter
            .emit_model_call_finished(
                span,
                "anthropic",
                &model,
                Some(100),
                Some(100),
                None,
                "sha256:test".to_string(),
                None,
            )
            .await;
    }

    let logs = captured.entries.lock().unwrap().clone();
    assert_eq!(
        logs.len(),
        1,
        "unpriced provider/model pair must emit one deduped debug log: {logs:?}",
    );
    assert_eq!(logs[0].provider, "anthropic");
    assert_eq!(logs[0].model, model);

    let events = collect_events(&bus, &recorder).await;
    let finished = finished_events(&events);
    assert_eq!(finished.len(), 2);
    for m in finished {
        assert!(
            m.cost_usd.is_none(),
            "unpriced model must publish cost_usd = None, got: {:?}",
            m.cost_usd,
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_catalogs_wired_leaves_cost_none() {
    // Default `ObsEmitter::new` has no catalogs — preserves legacy
    // behaviour for callers that haven't been threaded yet.
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let emitter = ObsEmitter::new(bus.clone(), "run-no-catalog");
    emitter
        .emit_model_call_finished(
            "span-nocat",
            "openrouter",
            "anthropic/claude-opus-4.7",
            Some(1_000),
            Some(1_000),
            None,
            "sha256:test".to_string(),
            None,
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let m = finished_events(&events).into_iter().next().unwrap();
    assert!(m.cost_usd.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn caller_supplied_cost_wins_over_catalog_lookup() {
    // Preserves the existing operator-trusted out-of-band cost paths
    // for Anthropic / bare OpenAI. If a future call site computes
    // cost from a richer wire signal (e.g. Anthropic usage block
    // with cache token classes), we don't want the catalog to
    // overwrite that with the cheaper-but-coarser per-Mtok number.
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let cat = catalog("openrouter", vec![priced_opus_entry()]);
    let mut catalogs = HashMap::new();
    catalogs.insert("openrouter".to_string(), cat);

    let emitter = ObsEmitter::new(bus.clone(), "run-caller-cost").with_catalogs(catalogs);
    emitter
        .emit_model_call_finished(
            "span-caller",
            "openrouter",
            "anthropic/claude-opus-4.7",
            Some(10_000),
            Some(2_000),
            Some(0.42), // caller-supplied wins
            "sha256:test".to_string(),
            None,
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let m = finished_events(&events).into_iter().next().unwrap();
    assert_eq!(m.cost_usd, Some(0.42));
}

/// Integration: drive priced model calls through a real
/// `SqliteRecorder` (the same persister the dashboard wires up) and
/// confirm `SUM(cost_usd) FROM model_calls` is positive. Closes the
/// "2,757/2,757 NULL" gap from the audit context end-to-end at the
/// recorder boundary without scaffolding the full eval executor.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sqlite_recorder_persists_positive_cost_usd_for_priced_runs() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

    // Temp-file SQLite — `:memory:` would give each pool connection
    // its own private DB. See `eval_observability::setup_pool` for
    // the same rationale.
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("cost.db");
    Box::leak(Box::new(tmp));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    // FKs reference `agent_runs`/`spans`; we apply migration 018
    // only, so cross-migration FKs (eval_runs, cli_jobs) would
    // refuse the inserts. Configure the pool instead of issuing a
    // connection-local PRAGMA so recorder operations cannot land on a
    // different connection with foreign keys enabled.
    let options = SqliteConnectOptions::from_str(&url).unwrap().foreign_keys(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();

    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));

    let cat = catalog("openrouter", vec![priced_opus_entry()]);
    let mut catalogs = HashMap::new();
    catalogs.insert("openrouter".to_string(), cat.clone());

    let run_id = "cost-int-run-1";
    let emitter = ObsEmitter::new(bus.clone(), run_id).with_catalogs(catalogs);
    let token_pairs = [(1_000_u32, 200_u32), (2_500, 500), (4_000, 1_000)];
    let expected_sum: f64 = token_pairs
        .iter()
        .map(|(input_tokens, output_tokens)| {
            compute_token_cost_usd_from_catalog(
                *input_tokens as u64,
                *output_tokens as u64,
                "anthropic/claude-opus-4.7",
                &cat,
            )
            .expect("priced catalog produces cost")
        })
        .sum();

    // Register the run row so spans have a parent.
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.to_string(),
        objective: "cost integration".to_string(),
        strategy_id: None,
        eval_run_id: Some(run_id.to_string()),
        source_cli_job_id: None,
        started_at: chrono::Utc::now(),
        retention_mode: "hash_only".to_string(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    // Three model calls — covers the "multiple decisions per run"
    // case a real backtest produces. Spans must exist before
    // `ModelCallFinished` lands or the recorder's INSERT FK to
    // spans.id fails (even with FKs OFF, missing rows mean the JOIN
    // we SUM over is empty).
    for (i, (in_tok, out_tok)) in token_pairs.iter().enumerate() {
        let span_id = format!("span-cost-{i}");
        bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
            span_id: span_id.clone(),
            run_id: run_id.to_string(),
            parent_span_id: None,
            kind: SpanKind::DecisionModel,
            name: "openrouter/anthropic/claude-opus-4.7".to_string(),
            started_at: chrono::Utc::now(),
            otel_trace_id: None,
            otel_span_id: None,
            attributes_json: None,
        }))
        .await;
        emitter
            .emit_model_call_finished(
                &span_id,
                "openrouter",
                "anthropic/claude-opus-4.7",
                Some(*in_tok),
                Some(*out_tok),
                None,
                "sha256:test".to_string(),
                Some("sha256:test".to_string()),
            )
            .await;
        bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
            span_id,
            ended_at: chrono::Utc::now(),
            status: SpanStatus::Ok,
            error_json: None,
        }))
        .await;
    }
    bus.quiesce().await;
    // Belt-and-braces flush — the recorder consumer task is async,
    // give it a slice to drain.
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        bus.quiesce().await;
    }

    // Sum cost_usd across the specific rows this test emitted.
    let (row_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM model_calls WHERE span_id LIKE 'span-cost-%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        row_count, 3,
        "all emitted model call rows must be persisted before cost aggregation"
    );
    let (sum,): (Option<f64>,) =
        sqlx::query_as("SELECT SUM(cost_usd) FROM model_calls WHERE span_id LIKE 'span-cost-%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let sum = sum.expect("at least one row with non-NULL cost_usd");
    assert!(
        (sum - expected_sum).abs() < 1e-12,
        "persisted sum(model_calls.cost_usd) must include all three priced rows; expected {expected_sum}, got {sum}",
    );

    let (unscoped_sum,): (Option<f64>,) = sqlx::query_as("SELECT SUM(cost_usd) FROM model_calls")
        .fetch_one(&pool)
        .await
        .unwrap();
    let unscoped_sum = unscoped_sum.expect("at least one row with non-NULL cost_usd");
    assert!(
        unscoped_sum > 0.0,
        "expected sum(model_calls.cost_usd) > 0 for priced run, got {unscoped_sum}",
    );

    // Sanity: every row has a non-NULL cost. The audit baseline was
    // 2,757/2,757 NULL — this is the regression bar we promise.
    let (null_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM model_calls WHERE span_id LIKE 'span-cost-%' AND cost_usd IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(null_count, 0, "no priced row should land with NULL cost_usd");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_payloads_companion_uses_same_resolution_rule() {
    // `emit_model_call_finished_with_payloads` is the production
    // call site (`agent::execute`), so the cost-resolution wiring
    // there is the load-bearing change.
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let cat = catalog("openrouter", vec![priced_opus_entry()]);
    let mut catalogs = HashMap::new();
    catalogs.insert("openrouter".to_string(), cat.clone());

    let emitter = ObsEmitter::new(bus.clone(), "run-with-payloads-cost").with_catalogs(catalogs);

    emitter
        .emit_model_call_finished_with_payloads(
            "span-wp",
            "openrouter",
            "anthropic/claude-opus-4.7",
            Some(10_000),
            Some(2_000),
            None,
            "sha256:test".to_string(),
            None,
            None,
            None,
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let m = finished_events(&events).into_iter().next().unwrap();
    let expected =
        compute_token_cost_usd_from_catalog(10_000, 2_000, "anthropic/claude-opus-4.7", &cat).unwrap();
    let actual = m.cost_usd.unwrap();
    assert!((actual - expected).abs() < 1e-12);
}
