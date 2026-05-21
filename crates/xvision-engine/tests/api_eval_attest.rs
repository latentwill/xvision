//! Phase 3.D Task 11 — `api::eval::attest` tests. Covers happy-path
//! sign + persist, NotFound for unknown runs, Validation for
//! not-yet-finalized runs, signing-key bootstrap (auto-generate on
//! first call, reuse on second), and audit-row write.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::api::eval::{self};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::attestation::verify;
use xvision_engine::eval::run::{MetricsSummary, Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;

async fn ctx_with_eval_tables() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("api_eval_attest.sqlite");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
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
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    (ctx, dir)
}

async fn seed_completed_run(store: &RunStore, scenario_id: &str) -> Run {
    let run = Run::new_queued("h-strategy".into(), scenario_id.into(), RunMode::Backtest);
    store.create(&run).await.unwrap();

    let t0 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    for i in 0..3 {
        let ts = t0 + Duration::hours(i);
        store
            .record_equity(&run.id, ts, 10_000.0 + (i as f64) * 100.0)
            .await
            .unwrap();
    }
    let metrics = MetricsSummary {
        total_return_pct: 5.0,
        sharpe: 1.2,
        max_drawdown_pct: 2.0,
        win_rate: 0.6,
        n_trades: 3,
        n_decisions: 5,
        baselines: None,
        ..Default::default()
    };
    store.finalize(&run.id, &metrics).await.unwrap();
    // Re-read so we have completed_at + metrics on the run we hand back.
    store.get(&run.id).await.unwrap()
}

#[tokio::test]
async fn attest_signs_completed_run_and_persists_attestation() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = canonical_scenarios()[0].id.clone();
    let run = seed_completed_run(&store, &scenario_id).await;

    let att = eval::attest(&ctx, &run.id).await.unwrap();
    // Returned attestation is structurally sound + verifies against its
    // own pubkey.
    assert_eq!(att.scenario_id, scenario_id);
    assert_eq!(att.agent_id, run.agent_id);
    assert!(!att.signature_hex.is_empty());
    assert!(!att.signing_pubkey_hex.is_empty());
    verify(&att).expect("self-verify must succeed");

    // Re-read from the store: record_attestation has persisted it.
    let persisted = store
        .get_attestation(&run.id)
        .await
        .unwrap()
        .expect("attestation should be persisted");
    assert_eq!(persisted.signature_hex, att.signature_hex);
    assert_eq!(persisted.signing_pubkey_hex, att.signing_pubkey_hex);
}

#[tokio::test]
async fn attest_returns_not_found_for_unknown_run() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let err = eval::attest(&ctx, "no-such-run-id").await.unwrap_err();
    match err {
        ApiError::NotFound(msg) => {
            assert!(msg.contains("no-such-run-id"), "msg: {msg}")
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn attest_rejects_run_without_metrics() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = canonical_scenarios()[0].id.clone();
    // Queued — no metrics computed.
    let mut run = Run::new_queued("h-strategy".into(), scenario_id, RunMode::Backtest);
    store.create(&run).await.unwrap();
    run = store.get(&run.id).await.unwrap();

    let err = eval::attest(&ctx, &run.id).await.unwrap_err();
    match err {
        ApiError::Validation(msg) => {
            assert!(
                msg.to_lowercase().contains("metrics"),
                "Validation msg should mention metrics, got: {msg}",
            )
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn attest_rejects_run_with_unknown_scenario() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let run = seed_completed_run(&store, "fictional-scenario-id").await;

    let err = eval::attest(&ctx, &run.id).await.unwrap_err();
    match err {
        ApiError::Validation(msg) => {
            assert!(
                msg.contains("fictional-scenario-id"),
                "Validation msg should name the offending scenario, got: {msg}",
            )
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn attest_reuses_signing_key_across_calls() {
    // First attest auto-generates the key. Second attest on a different
    // run should reuse the same key — same pubkey hex.
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = canonical_scenarios()[0].id.clone();
    let run_a = seed_completed_run(&store, &scenario_id).await;
    let run_b = seed_completed_run(&store, &scenario_id).await;

    let a = eval::attest(&ctx, &run_a.id).await.unwrap();
    let b = eval::attest(&ctx, &run_b.id).await.unwrap();
    assert_eq!(
        a.signing_pubkey_hex, b.signing_pubkey_hex,
        "second attest must reuse the cached key, not regenerate"
    );

    // The key file exists at $xvn_home/identity/signing.key.
    let key_path = ctx.xvn_home.join("identity").join("signing.key");
    let bytes = std::fs::read(&key_path).expect("signing key should be cached on disk");
    assert_eq!(bytes.len(), 32, "key file must be raw 32 bytes");
}

#[tokio::test]
async fn attest_writes_audit_row() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = canonical_scenarios()[0].id.clone();
    let run = seed_completed_run(&store, &scenario_id).await;
    let _ = eval::attest(&ctx, &run.id).await.unwrap();

    let row = sqlx::query(
        "SELECT domain, operation, target, outcome FROM api_audit \
         WHERE operation = 'attest' ORDER BY occurred_at DESC LIMIT 1",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    use sqlx::Row;
    let domain: String = row.try_get("domain").unwrap();
    let operation: String = row.try_get("operation").unwrap();
    let target: Option<String> = row.try_get("target").unwrap();
    let outcome: String = row.try_get("outcome").unwrap();
    assert_eq!(domain, "eval");
    assert_eq!(operation, "attest");
    assert_eq!(outcome, "ok");
    assert_eq!(target.as_deref(), Some(run.id.as_str()));
}
