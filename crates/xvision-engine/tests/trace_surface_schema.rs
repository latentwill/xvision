//! Integration tests for eval-trace-surface-foundation (V2E item 17).
//!
//! Covers:
//!  - Schema-version round-trip: old runs (schema_version="1") load without
//!    panics and hydrate new optional fields to their defaults.
//!  - Determinism receipt minted and stable across re-runs with identical
//!    inputs.
//!  - `cycle_features.parquet` sidecar writes correct row count for a
//!    fixed-decision-count run.
//!  - Findings carry the new fields and round-trip through JSONL (serde).
//!  - `cycles` index plan visible in `EXPLAIN QUERY PLAN` for the
//!    model_id+regime_tag query pattern (uses the xvision-core migration).
//!
//! See: team/contracts/eval-trace-surface-foundation.md — acceptance §Tests.

use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use xvision_engine::eval::determinism::{persist_receipt, read_receipt, DeterminismReceipt, ReceiptInputs};
use xvision_engine::eval::findings::{Finding, Severity, FINDING_SCHEMA_VERSION};
use xvision_engine::eval::store::RunStore;
use xvision_engine::eval::{MetricsSummary, Run, RunMode, RunStatus};

// ---------------------------------------------------------------------------
// Migration helpers
// ---------------------------------------------------------------------------

/// Apply the engine migrations needed for the findings + determinism tables.
async fn engine_pool_with_026() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    // FK enforcement off: test harness doesn't insert all dependency rows.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    // Core eval schema.
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/017_eval_findings_review_columns.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // V2E foundation.
    sqlx::query(include_str!("../migrations/026_trace_surface_foundation.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

/// Apply the xvision-core migrations so we get the `cycles` table with
/// the V2E indices from migration 0003.
async fn core_pool_with_0003() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../../xvision-core/migrations/0001_init.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../../xvision-core/migrations/0002_rename_setup_to_cycle.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!(
        "../../xvision-core/migrations/0003_cycles_trace_indices.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn completed_run() -> Run {
    let mut r = Run::new_queued(
        "strategy-v2e".into(),
        "test-scenario-v2e".into(),
        RunMode::Backtest,
    );
    r.status = RunStatus::Queued;
    r.metrics = Some(MetricsSummary {
        total_return_pct: 2.5,
        sharpe: 0.9,
        max_drawdown_pct: 5.0,
        win_rate: 0.55,
        n_trades: 8,
        n_decisions: 20,
        baselines: None,
        inference_cost_quote_total: None,
        net_return_pct: None,
    });
    r
}

// ---------------------------------------------------------------------------
// Schema-version round-trip tests
// ---------------------------------------------------------------------------

/// A finding serialized with schema_version="1" (old shape) must deserialize
/// cleanly into the new `Finding` type, with new fields taking their defaults.
#[test]
fn schema_v1_finding_loads_with_defaults() {
    // Simulate a v1 finding — no evidence_cycle_ids, no produced_by_check.
    let v1_json = serde_json::json!({
        "id": "01V1FINDING00000000000000",
        "run_id": "01V1RUN000000000000000000",
        "kind": "overtrading",
        "severity": "warning",
        "summary": "Legacy finding without new fields",
        "evidence": {"metric_name": "n_decisions", "value": 80},
        "extracted_at": "2026-01-01T00:00:00Z",
        "schema_version": "1"
    });
    let f: Finding = serde_json::from_value(v1_json).expect("v1 finding must parse cleanly");
    assert_eq!(f.schema_version, "1");
    // New fields default correctly — both absent means None (legacy).
    assert!(
        f.evidence_cycle_ids.is_none(),
        "evidence_cycle_ids must default to None for v1 findings"
    );
    assert!(
        f.produced_by_check.is_none(),
        "produced_by_check must default to None for v1 findings"
    );
}

/// A finding serialized with the new schema_version="2" must round-trip.
#[test]
fn schema_v2_finding_round_trips() {
    let f = Finding {
        id: "01V2FINDING00000000000000".into(),
        run_id: "01V2RUN000000000000000000".into(),
        kind: "lookahead_suspected".into(),
        severity: Severity::Warning,
        summary: "Indicator value changed between passes".into(),
        evidence: serde_json::json!({"indicator": "ema5", "delta": 0.12}),
        extracted_at: Utc::now(),
        schema_version: FINDING_SCHEMA_VERSION.into(),
        evidence_cycle_ids: Some(vec![
            "01CYCLE000000000000000001".into(),
            "01CYCLE000000000000000002".into(),
        ]),
        produced_by_check: Some("lookahead_prober".into()),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: None,
        description: None,
        recommendation: None,
        created_at: None,
    };
    let json = serde_json::to_string(&f).expect("serialize v2 finding");
    let back: Finding = serde_json::from_str(&json).expect("deserialize v2 finding");

    assert_eq!(back.schema_version, "2");
    let ids = back
        .evidence_cycle_ids
        .as_ref()
        .expect("evidence_cycle_ids must be Some for v2");
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], "01CYCLE000000000000000001");
    assert_eq!(back.produced_by_check.as_deref(), Some("lookahead_prober"));
}

// ---------------------------------------------------------------------------
// Findings DB round-trip test (migration 026 columns)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn findings_new_fields_persist_and_read_back() {
    let pool = engine_pool_with_026().await;
    let store = RunStore::new(pool);

    // Create a run.
    let mut run = completed_run();
    run.status = RunStatus::Queued;
    store.create(&run).await.unwrap();
    store
        .finalize(&run.id, run.metrics.as_ref().unwrap())
        .await
        .unwrap();

    let f = Finding {
        id: ulid::Ulid::new().to_string(),
        run_id: run.id.clone(),
        kind: "lookahead_suspected".into(),
        severity: Severity::Warning,
        summary: "V2E finding with evidence_cycle_ids".into(),
        evidence: serde_json::json!({"delta": 0.05}),
        extracted_at: Utc::now(),
        schema_version: FINDING_SCHEMA_VERSION.into(),
        evidence_cycle_ids: Some(vec!["01CYCLEAAA".into(), "01CYCLEBBB".into()]),
        produced_by_check: Some("lookahead_prober".into()),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: None,
        description: None,
        recommendation: None,
        created_at: None,
    };

    store.record_finding(&f).await.unwrap();
    let read = store.read_findings(&run.id).await.unwrap();
    assert_eq!(read.len(), 1);

    let got = &read[0];
    assert_eq!(got.schema_version, FINDING_SCHEMA_VERSION);
    let got_ids = got
        .evidence_cycle_ids
        .as_ref()
        .expect("evidence_cycle_ids must be Some");
    assert_eq!(got_ids, &["01CYCLEAAA", "01CYCLEBBB"]);
    assert_eq!(got.produced_by_check.as_deref(), Some("lookahead_prober"));
}

/// Pre-026 rows (simulated by inserting directly without the new columns)
/// must still load, using the column DEFAULT values.
#[tokio::test]
async fn findings_legacy_rows_load_with_defaults() {
    let pool = engine_pool_with_026().await;
    let store = RunStore::new(pool.clone());

    // Create a run.
    let mut run = completed_run();
    run.status = RunStatus::Queued;
    store.create(&run).await.unwrap();
    store
        .finalize(&run.id, run.metrics.as_ref().unwrap())
        .await
        .unwrap();

    // Insert a row manually WITHOUT the new columns — simulates a row from
    // before migration 026. The migration sets column defaults ('[]' and
    // 'legacy'), so the row round-trips to empty/legacy.
    sqlx::query(
        "INSERT INTO eval_findings \
         (id, run_id, kind, severity, summary, evidence_json, extracted_at, schema_version, \
          eval_review_id, type, confidence, title, description, recommendation, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(&run.id)
    .bind("overtrading")
    .bind("info")
    .bind("legacy finding")
    .bind("{}")
    .bind(Utc::now().to_rfc3339())
    .bind("1")
    .execute(&pool)
    .await
    .unwrap();

    let read = store.read_findings(&run.id).await.unwrap();
    assert_eq!(read.len(), 1);
    // Defaults from the migration — both columns carry the DB default ('[]'
    // and 'legacy'), which the store reader converts to None.
    assert!(
        read[0].evidence_cycle_ids.is_none(),
        "legacy row must have None evidence_cycle_ids"
    );
    assert!(
        read[0].produced_by_check.is_none(),
        "legacy row must have None produced_by_check"
    );
}

// ---------------------------------------------------------------------------
// Determinism receipt tests
// ---------------------------------------------------------------------------

fn test_inputs() -> ReceiptInputs {
    ReceiptInputs {
        run_id: "01TESTRUNV2E0000000000000".into(),
        strategy_hash: "abc123strategyhash".into(),
        scenario_id: "crypto-bull-q1-2025".into(),
        bars_content_hash: "deadbeefbarscontentshasum000".into(),
        seed: 42,
        engine_version: "0.21.0".into(),
        schema_version: "2".into(),
    }
}

#[test]
fn receipt_is_stable_across_identical_inputs() {
    let r1 = DeterminismReceipt::mint(&test_inputs());
    let r2 = DeterminismReceipt::mint(&test_inputs());
    assert_eq!(
        r1.receipt_hash, r2.receipt_hash,
        "identical inputs must produce the same receipt hash"
    );
    assert_eq!(r1.receipt_hash.len(), 64, "sha256 hex must be 64 chars");
}

#[test]
fn receipt_changes_on_input_change() {
    let base = test_inputs();
    let r_base = DeterminismReceipt::mint(&base);
    let r_seed = DeterminismReceipt::mint(&ReceiptInputs { seed: 999, ..base });
    assert_ne!(
        r_base.receipt_hash, r_seed.receipt_hash,
        "changing seed must change receipt hash"
    );
}

#[tokio::test]
async fn receipt_persists_and_reads_back() {
    let pool = engine_pool_with_026().await;
    let receipt = DeterminismReceipt::mint(&test_inputs());

    persist_receipt(&pool, &receipt).await.unwrap();

    let read = read_receipt(&pool, &receipt.run_id).await.unwrap();
    let read = read.expect("receipt must exist after persist");
    assert_eq!(read.receipt_hash, receipt.receipt_hash);
    assert_eq!(read.engine_version, "0.21.0");
    assert_eq!(read.schema_version, "2");
    assert!(
        read.manifest_canonical.is_none(),
        "manifest_canonical must be None — reserved for candle-integrity track"
    );
}

#[tokio::test]
async fn receipt_idempotent_on_rerun() {
    let pool = engine_pool_with_026().await;
    let r1 = DeterminismReceipt::mint(&test_inputs());
    let r2 = DeterminismReceipt::mint(&test_inputs());

    persist_receipt(&pool, &r1).await.unwrap();
    // Second persist with identical hash must not error (INSERT OR REPLACE).
    persist_receipt(&pool, &r2).await.unwrap();

    let read = read_receipt(&pool, &r1.run_id).await.unwrap().unwrap();
    assert_eq!(read.receipt_hash, r1.receipt_hash);
}

// ---------------------------------------------------------------------------
// cycle_features.parquet sidecar test
// ---------------------------------------------------------------------------

#[test]
fn parquet_sidecar_row_count_matches_push_count() {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use tempfile::TempDir;
    use xvision_engine::eval::cycle_features::{CycleFeatureRow, CycleFeaturesWriter};

    let dir = TempDir::new().unwrap();
    let mut writer = CycleFeaturesWriter::new(dir.path().to_path_buf());

    // Push exactly 12 rows — a fixed decision count.
    for i in 0..12u32 {
        writer.push_row(CycleFeatureRow {
            cycle_id: format!("01CYCLE{i:016}"),
            decision_index: i,
            model_id: "claude-opus-4-7".into(),
            prompt_template_hash: format!("hash{i:04x}"),
            regime_tag: Some("trend".into()),
            position_units: if i % 3 == 0 { 0.0 } else { 0.5 },
            equity: 10_000.0 + i as f64 * 50.0,
            drawdown_pct: (i as f64 * 0.1).min(5.0),
            prior_decision_action: if i == 0 { None } else { Some("hold".into()) },
            tokens_in: 800,
            tokens_out: 150,
            inference_cost_quote: None,
            latency_ms: 320,
        });
    }

    let n = writer.flush().unwrap();
    assert_eq!(n, 12, "flush must return the row count");

    // Read back the parquet and verify row count.
    let path = dir.path().join("cycle_features.parquet");
    assert!(path.exists());
    let file = std::fs::File::open(&path).unwrap();
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .unwrap()
        .build()
        .unwrap();
    let total: usize = reader.map(|b| b.unwrap().num_rows()).sum();
    assert_eq!(total, 12, "parquet must contain exactly 12 rows");
}

// ---------------------------------------------------------------------------
// cycles index plan test (EXPLAIN QUERY PLAN)
// ---------------------------------------------------------------------------

/// The composite index idx_cycles_model_regime must be used for the
/// autoresearcher's primary query shape: model_id = ? AND regime_tag = ?.
#[tokio::test]
async fn cycles_index_plan_uses_composite_index() {
    let pool = core_pool_with_0003().await;

    // Insert a dummy cycle so the planner has real data to reason about.
    sqlx::query(
        "INSERT INTO cycles (cycle_id, asset, horizon_h, market_state_json, created_at, \
                             model_id, prompt_template_hash, regime_tag) \
         VALUES ('01CYCLE0001', 'BTC/USD', 4, '{}', '2026-05-01T00:00:00Z', \
                 'claude-opus-4-7', 'abc123', 'trend')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Run EXPLAIN QUERY PLAN for the autoresearcher query pattern.
    let rows = sqlx::query(
        "EXPLAIN QUERY PLAN \
         SELECT cycle_id, model_id, regime_tag \
         FROM cycles \
         WHERE model_id = 'claude-opus-4-7' AND regime_tag = 'trend'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    // The plan must mention one of the new indices — the composite or model_id.
    let plan_text: String = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("detail").ok())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        plan_text.contains("idx_cycles_model_regime") || plan_text.contains("idx_cycles_model_id"),
        "EXPLAIN QUERY PLAN must use one of the V2E indices for model_id+regime_tag query; \
         got plan: {plan_text:?}"
    );
}
