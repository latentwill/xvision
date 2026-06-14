//! WS-7 full-fidelity export ("the flywheel document").
//!
//! Seeds a run with spans + a variety of `events` rows (decision_completed,
//! risk_veto, filter_fired, regime_transition, order_state,
//! model_call_payload, tool_call_payload) + content-addressed blobs, then
//! asserts the export is COMPLETE and self-contained:
//!
//! (a) every seeded event kind appears in `export.events`, in timeline
//!     (`created_at`) order;
//! (b) blob-backed payloads are INLINED — the prompt/response text and the
//!     tool input/output text are present in the export, not just a ref;
//! (c) the Markdown contains the actual prompt text, a tool I/O, an order
//!     event, and a filter firing.
//!
//! This is the headline WS-7 fix: `build_export` previously omitted the
//! entire `events` table except `model_call_payload`.

use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};
use tempfile::TempDir;

use xvision_observability::{build_export_with_blobs, render_report, BlobStore};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

const RUN_ID: &str = "run_ws7_fidelity";

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

async fn exec(pool: &SqlitePool, sql: &str) {
    pool.execute(sqlx::query(sql)).await.unwrap();
}

/// Seed an event row directly. `created_at` is spaced so timeline order
/// is deterministic.
async fn seed_event(
    pool: &SqlitePool,
    id: &str,
    span_id: Option<&str>,
    kind: &str,
    payload_json: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(id)
    .bind(RUN_ID)
    .bind(span_id)
    .bind(kind)
    .bind(payload_json)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn export_includes_all_event_kinds_and_inlines_blobs() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    // Write blobs for the model call (prompt/response) + tool I/O.
    let prompt_ref = store.write(b"SYSTEM: you are a trader. Decide BTC/USD.").unwrap();
    let response_ref = store.write(b"I will go LONG with 0.5 size.").unwrap();
    let tool_in_ref = store.write(br#"{"symbol":"BTC/USD","tf":"1h"}"#).unwrap();
    let tool_out_ref = store.write(br#"{"bars":[{"c":64000.0}]}"#).unwrap();

    // Run + root span.
    exec(
        &pool,
        &format!(
            "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
             VALUES ('{RUN_ID}', 'WS-7 fidelity run', 'completed', \
             '2026-06-13T10:00:00Z', 'full_debug')"
        ),
    )
    .await;
    exec(
        &pool,
        &format!(
            "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
             VALUES ('span_root', '{RUN_ID}', 'agent.run', 'agent.run', 'ok', \
             '2026-06-13T10:00:01Z')"
        ),
    )
    .await;

    // Model-call span + row referencing the prompt/response blobs.
    exec(
        &pool,
        &format!(
            "INSERT INTO spans (id, run_id, parent_span_id, kind, name, status, started_at) \
             VALUES ('span_model', '{RUN_ID}', 'span_root', 'model.call', 'model.call', 'ok', \
             '2026-06-13T10:00:02Z')"
        ),
    )
    .await;
    sqlx::query(
        "INSERT INTO model_calls (span_id, provider, model, prompt_hash, \
             prompt_payload_ref, response_payload_ref) \
         VALUES ('span_model', 'anthropic', 'claude', 'sha256:p', ?1, ?2)",
    )
    .bind(prompt_ref.as_str())
    .bind(response_ref.as_str())
    .execute(&pool)
    .await
    .unwrap();

    // Tool-call span + row referencing the I/O blobs.
    exec(
        &pool,
        &format!(
            "INSERT INTO spans (id, run_id, parent_span_id, kind, name, status, started_at) \
             VALUES ('span_tool', '{RUN_ID}', 'span_root', 'tool.call', 'tool.call', 'ok', \
             '2026-06-13T10:00:03Z')"
        ),
    )
    .await;
    sqlx::query(
        "INSERT INTO tool_calls (span_id, tool_name, origin, input_hash, \
             input_payload_ref, output_payload_ref, side_effect_level, risk_level) \
         VALUES ('span_tool', 'fetch_bars', 'native', 'sha256:in', ?1, ?2, 'read_only', 'safe_read')",
    )
    .bind(tool_in_ref.as_str())
    .bind(tool_out_ref.as_str())
    .execute(&pool)
    .await
    .unwrap();

    // A variety of events rows in timeline order. Each represents a kind
    // that the old export dropped on the floor.
    // These two side-row events exist so the `model_call_payload` /
    // `tool_call_payload` kinds appear in the timeline, but they carry no
    // prompt/response/input/output here — proving the export falls back to
    // the content-addressed blob store to inline the actual bodies.
    seed_event(
        &pool,
        "evt_model_payload",
        Some("span_model"),
        "model_call_payload",
        r#"{"provider":"anthropic","model":"claude"}"#,
        "2026-06-13T10:00:02Z",
    )
    .await;
    seed_event(
        &pool,
        "evt_tool_payload",
        Some("span_tool"),
        "tool_call_payload",
        r#"{"tool_name":"fetch_bars"}"#,
        "2026-06-13T10:00:03Z",
    )
    .await;
    seed_event(
        &pool,
        "evt_decision",
        Some("span_model"),
        "decision_completed",
        r#"{"decision_index":7,"asset":"BTC/USD","action":"long"}"#,
        "2026-06-13T10:00:04Z",
    )
    .await;
    seed_event(
        &pool,
        "evt_risk_veto",
        Some("span_model"),
        "risk_veto",
        r#"{"reason":"exposure_cap","rule":"max_notional"}"#,
        "2026-06-13T10:00:05Z",
    )
    .await;
    seed_event(
        &pool,
        "evt_filter",
        None,
        "filter_fired",
        r#"{"filter":"min_volume","detail":"24h volume below floor"}"#,
        "2026-06-13T10:00:06Z",
    )
    .await;
    seed_event(
        &pool,
        "evt_regime",
        None,
        "regime_transition",
        r#"{"from":"range","to":"trend"}"#,
        "2026-06-13T10:00:07Z",
    )
    .await;
    seed_event(
        &pool,
        "evt_order",
        Some("span_tool"),
        "order_state",
        r#"{"state":"filled","broker_order_id":"ORD-123","fill_price":64000.0}"#,
        "2026-06-13T10:00:08Z",
    )
    .await;

    // Build the export WITH the blob store so refs inline.
    let export = build_export_with_blobs(&pool, RUN_ID, Some(&store))
        .await
        .unwrap();

    // (schema) shape changed → version bumped to v3.
    assert_eq!(export.schema_version, "xvn.agent_run.v3");

    // (a) every seeded event kind appears at least once.
    let kinds: Vec<&str> = export.events.iter().map(|e| e.kind.as_str()).collect();
    for required in [
        "model_call_payload",
        "tool_call_payload",
        "decision_completed",
        "risk_veto",
        "filter_fired",
        "regime_transition",
        "order_state",
    ] {
        assert!(
            kinds.contains(&required),
            "export.events is missing kind `{required}`; got {kinds:?}"
        );
    }

    // (a) timeline order: created_at must be non-decreasing.
    for w in export.events.windows(2) {
        assert!(
            w[0].created_at <= w[1].created_at,
            "events not in timeline order: {:?} then {:?}",
            w[0].created_at,
            w[1].created_at
        );
    }

    // (b) blob-backed model-call payloads are inlined (from prompt/response ref).
    let mc = &export.model_calls[0];
    assert_eq!(
        mc.prompt_text.as_deref(),
        Some("SYSTEM: you are a trader. Decide BTC/USD."),
        "prompt blob must be inlined into the export, got {:?}",
        mc.prompt_text
    );
    assert_eq!(
        mc.response_text.as_deref(),
        Some("I will go LONG with 0.5 size."),
        "response blob must be inlined into the export, got {:?}",
        mc.response_text
    );

    // (b) tool I/O blobs are inlined.
    let tc = &export.tool_calls[0];
    assert_eq!(
        tc.input_text.as_deref(),
        Some(r#"{"symbol":"BTC/USD","tf":"1h"}"#),
        "tool input blob must be inlined, got {:?}",
        tc.input_text
    );
    assert_eq!(
        tc.output_text.as_deref(),
        Some(r#"{"bars":[{"c":64000.0}]}"#),
        "tool output blob must be inlined, got {:?}",
        tc.output_text
    );

    // (c) the Markdown surfaces the actual content, agent-readable.
    let md = render_report(&export).markdown;
    assert!(
        md.contains("SYSTEM: you are a trader"),
        "markdown must contain the actual prompt text:\n{md}"
    );
    assert!(
        md.contains(r#"{"bars":[{"c":64000.0}]}"#) || md.contains("fetch_bars"),
        "markdown must contain a tool I/O:\n{md}"
    );
    assert!(
        md.contains("order_state") || md.contains("ORD-123"),
        "markdown must contain an order event:\n{md}"
    );
    assert!(
        md.contains("filter_fired") || md.contains("min_volume"),
        "markdown must contain a filter firing:\n{md}"
    );
    assert!(
        md.contains("risk_veto") || md.contains("exposure_cap"),
        "markdown must contain a risk gate:\n{md}"
    );
    assert!(
        md.contains("regime_transition") || md.contains("trend"),
        "markdown must contain a regime transition:\n{md}"
    );
}
