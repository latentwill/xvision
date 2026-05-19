//! Integration tests for the rule-based eval-review auto-runner.
//!
//! See `crates/xvision-engine/src/eval/review/auto.rs`. These tests
//! exercise the runner against a real SQLite-backed `RunStore` (in
//! memory), including the idempotency guard on a second invocation
//! for the same run.

use chrono::Utc;
use sqlx::{Row, SqlitePool};
use ulid::Ulid;
use xvision_engine::eval::findings::{Finding, Severity};
use xvision_engine::eval::review::{
    fire_auto_review, run_auto_review, AutoReviewOptions, AutoReviewOutcome, ReviewStatus, ReviewVerdict,
    AUTO_AGENT_PROFILE_ID,
};
use xvision_engine::eval::{MetricsSummary, Run, RunMode, RunStatus, RunStore};

/// Apply every migration that touches eval state. Mirrors the prefix
/// `ApiContext::open` walks at startup so the FK from `eval_reviews`
/// into `agent_profiles` resolves cleanly.
async fn pool_with_migrations() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    for sql in [
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/017_eval_findings_review_columns.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

async fn finalized_run(store: &RunStore) -> Run {
    let mut r = Run::new_queued("agent-h".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    r.status = RunStatus::Queued;
    store.create(&r).await.unwrap();
    let metrics = MetricsSummary {
        total_return_pct: 4.2,
        sharpe: 0.6,
        max_drawdown_pct: 6.1,
        win_rate: 0.55,
        n_trades: 18,
        n_decisions: 60,
    };
    store.begin_running(&r.id).await.unwrap();
    store.finalize(&r.id, &metrics).await.unwrap();
    r.metrics = Some(metrics);
    r.status = RunStatus::Completed;
    r
}

fn finding(run_id: &str, severity: Severity, kind: &str, summary: &str) -> Finding {
    Finding {
        id: Ulid::new().to_string(),
        run_id: run_id.into(),
        kind: kind.into(),
        severity,
        summary: summary.into(),
        evidence: serde_json::Value::Null,
        extracted_at: Utc::now(),
        schema_version: "v1".into(),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: None,
        description: None,
        recommendation: None,
        created_at: None,
    }
}

#[tokio::test]
async fn auto_review_writes_one_row_with_expected_verdict_and_score_band() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    // Two warnings, one info → expect "weak" with score in [26, 50].
    for f in [
        finding(
            &run.id,
            Severity::Warning,
            "underperformance",
            "Win rate below threshold.",
        ),
        finding(
            &run.id,
            Severity::Warning,
            "win_rate_anomaly",
            "Streaky win pattern.",
        ),
        finding(
            &run.id,
            Severity::Info,
            "regime_fit_mismatch",
            "Bull regime mismatch.",
        ),
    ] {
        store.record_finding(&f).await.unwrap();
    }

    let outcome = run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .unwrap();
    match outcome {
        AutoReviewOutcome::Inserted { verdict, score, .. } => {
            assert_eq!(verdict, ReviewVerdict::Weak);
            assert!(
                (26..=50).contains(&score),
                "score {score} not in weak band [26, 50]"
            );
        }
        AutoReviewOutcome::AlreadyExists { .. } => panic!("expected Inserted on first call"),
    }

    let reviews = store.list_reviews_for_run(&run.id).await.unwrap();
    assert_eq!(reviews.len(), 1, "expected exactly one eval_reviews row");
    let r = &reviews[0];
    assert_eq!(r.status, ReviewStatus::Completed);
    assert_eq!(r.verdict, Some(ReviewVerdict::Weak));
    assert_eq!(r.agent_profile_id, AUTO_AGENT_PROFILE_ID);
    assert!(r.summary.as_deref().unwrap_or("").len() <= 240);
    let raw = r.raw_output_json.as_deref().expect("raw_output_json set");
    let v: serde_json::Value = serde_json::from_str(raw).expect("raw is valid JSON");
    assert_eq!(v["auto_runner"]["verdict"], "weak");
    assert!(v["findings"].is_array());
    assert_eq!(v["findings"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn auto_review_failed_verdict_for_critical_finding() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    store
        .record_finding(&finding(
            &run.id,
            Severity::Critical,
            "risk_violation",
            "Position size exceeded limit.",
        ))
        .await
        .unwrap();

    let outcome = run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .unwrap();
    match outcome {
        AutoReviewOutcome::Inserted { verdict, score, .. } => {
            assert_eq!(verdict, ReviewVerdict::Failed);
            assert!((0..=25).contains(&score), "score {score} not in failed band");
        }
        _ => panic!("expected Inserted"),
    }
}

#[tokio::test]
async fn auto_review_inconclusive_when_no_findings() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let outcome = run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .unwrap();
    match outcome {
        AutoReviewOutcome::Inserted { verdict, score, .. } => {
            assert_eq!(verdict, ReviewVerdict::Inconclusive);
            assert_eq!(score, 50);
        }
        _ => panic!("expected Inserted"),
    }
}

#[tokio::test]
async fn auto_review_promising_when_only_info_findings() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    for _ in 0..2 {
        store
            .record_finding(&finding(&run.id, Severity::Info, "observation", "Noted a thing."))
            .await
            .unwrap();
    }

    let outcome = run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .unwrap();
    match outcome {
        AutoReviewOutcome::Inserted { verdict, score, .. } => {
            assert_eq!(verdict, ReviewVerdict::Promising);
            assert!((75..=100).contains(&score), "score {score} not in promising band");
        }
        _ => panic!("expected Inserted"),
    }
}

#[tokio::test]
async fn second_invocation_does_not_create_duplicate_row() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    store
        .record_finding(&finding(
            &run.id,
            Severity::Warning,
            "overtrading",
            "High decision frequency.",
        ))
        .await
        .unwrap();
    store
        .record_finding(&finding(
            &run.id,
            Severity::Warning,
            "underperformance",
            "Returns below baseline.",
        ))
        .await
        .unwrap();

    // First call → Inserted.
    let first = run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .unwrap();
    let first_id = match first {
        AutoReviewOutcome::Inserted { review_id, .. } => review_id,
        _ => panic!("first call should insert"),
    };

    // Second call → AlreadyExists (same id).
    let second = run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .unwrap();
    match second {
        AutoReviewOutcome::AlreadyExists { review_id } => {
            assert_eq!(review_id, first_id);
        }
        _ => panic!("second call should hit idempotency guard"),
    }

    let reviews = store.list_reviews_for_run(&run.id).await.unwrap();
    assert_eq!(
        reviews.len(),
        1,
        "idempotency: expected single row, got {}",
        reviews.len()
    );
}

#[tokio::test]
async fn fire_auto_review_is_best_effort_and_does_not_panic_on_missing_run() {
    // Run id that doesn't exist in any table — finalize seam should
    // still complete without panic. Findings read returns Ok([]) for a
    // missing run, so the wrapper writes an `inconclusive` review row
    // referencing a non-existent run id. The eval_reviews FK into
    // eval_runs is enforced; SQLite's foreign-key enforcement is off
    // by default for `:memory:` without `PRAGMA foreign_keys = ON`,
    // so the row inserts and the wrapper succeeds.
    //
    // This test pins the "never panic, never propagate" contract.
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    fire_auto_review(&store, "does-not-exist").await;
}

#[tokio::test]
async fn auto_review_respects_explicit_agent_profile_id() {
    // Two distinct agent_profile_ids → two distinct rows. The
    // idempotency guard is per-(run, agent_profile_id) pair.
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool.clone());
    let run = finalized_run(&store).await;

    store
        .record_finding(&finding(&run.id, Severity::Info, "observation", "fyi."))
        .await
        .unwrap();

    // Default profile.
    let _ = run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .unwrap();

    // Explicit, distinct profile (use another seeded id).
    let _ = run_auto_review(
        &store,
        &run.id,
        AutoReviewOptions {
            agent_profile_id: Some("reasoning-agent".into()),
        },
    )
    .await
    .unwrap();

    // Two rows now.
    let n: i64 = sqlx::query("SELECT COUNT(*) AS c FROM eval_reviews WHERE eval_run_id = ?")
        .bind(&run.id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("c")
        .unwrap();
    assert_eq!(n, 2, "expected one row per agent_profile_id");
}
