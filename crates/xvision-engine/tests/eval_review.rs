//! Tests for the eval-review data-model foundation (migrations 016/017,
//! `EvalReview` / `AgentProfile` types, review-linked finding columns).
//!
//! Scope is persistence only — review-payload assembly, prompt/response
//! contract, and API/CLI wiring belong to downstream execution-board
//! tracks (`eval-review-agent-engine`, `eval-review-api-cli`,
//! `eval-review-run-detail-ui`).

use chrono::Utc;
use sqlx::{Row, SqlitePool};
use xvision_engine::eval::findings::{Finding, Severity};
use xvision_engine::eval::review::{EvalReview, ReviewStatus, ReviewVerdict};
use xvision_engine::eval::{MetricsSummary, Run, RunMode, RunStatus, RunStore};

/// Build an in-memory pool with every migration that touches eval state
/// applied — the same prefix `ApiContext::open` walks at startup.
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

#[tokio::test]
async fn migration_seeds_four_canonical_agent_profiles() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);

    let profiles = store.list_agent_profiles(true).await.unwrap();
    let ids: Vec<&str> = profiles.iter().map(|p| p.id.as_str()).collect();

    assert!(ids.contains(&"fast-trader-agent"), "missing fast-trader-agent");
    assert!(ids.contains(&"reasoning-agent"), "missing reasoning-agent");
    assert!(ids.contains(&"risk-agent"), "missing risk-agent");
    assert!(ids.contains(&"research-agent"), "missing research-agent");

    // Every seeded profile must have a non-empty system_prompt and a real
    // provider/model. The engine track relies on these defaults to avoid
    // a "configure your providers" hop on first review.
    for p in &profiles {
        assert!(!p.system_prompt.is_empty(), "{}: empty system_prompt", p.id);
        assert!(!p.provider.is_empty(), "{}: empty provider", p.id);
        assert!(!p.model.is_empty(), "{}: empty model", p.id);
        assert!(p.enabled, "{}: should default to enabled", p.id);
        assert!(p.temperature >= 0.0 && p.temperature <= 1.0);
        assert!(p.max_tokens > 0);
    }
}

#[tokio::test]
async fn agent_profile_seed_is_idempotent() {
    // Re-running the migration must not duplicate or overwrite seed rows.
    let pool = pool_with_migrations().await;
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM agent_profiles")
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("c")
        .unwrap();
    assert_eq!(count, 4, "seed rows duplicated on re-apply");
}

#[tokio::test]
async fn list_agent_profiles_respects_enabled_filter() {
    let pool = pool_with_migrations().await;
    sqlx::query("UPDATE agent_profiles SET enabled = 0 WHERE id = 'fast-trader-agent'")
        .execute(&pool)
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let enabled_only = store.list_agent_profiles(true).await.unwrap();
    let all = store.list_agent_profiles(false).await.unwrap();
    assert_eq!(enabled_only.len(), 3);
    assert_eq!(all.len(), 4);
    assert!(enabled_only.iter().all(|p| p.id != "fast-trader-agent"));
}

#[tokio::test]
async fn get_agent_profile_returns_none_for_unknown_id() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let got = store.get_agent_profile("not-a-real-profile").await.unwrap();
    assert!(got.is_none());
}

#[tokio::test]
async fn review_create_get_round_trip() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let review = EvalReview::new_queued(run.id.clone(), "reasoning-agent".into());
    store.create_review(&review).await.unwrap();

    let got = store.get_review(&review.id).await.unwrap().expect("review present");
    assert_eq!(got.id, review.id);
    assert_eq!(got.eval_run_id, run.id);
    assert_eq!(got.agent_profile_id, "reasoning-agent");
    assert_eq!(got.status, ReviewStatus::Queued);
    assert!(got.verdict.is_none());
    assert!(got.summary.is_none());
    assert!(got.error.is_none());
}

#[tokio::test]
async fn review_status_machine_transitions() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let review = EvalReview::new_queued(run.id.clone(), "risk-agent".into());
    store.create_review(&review).await.unwrap();

    // queued → running
    let advanced = store.begin_review_running(&review.id).await.unwrap();
    assert!(advanced);
    let mid = store.get_review(&review.id).await.unwrap().unwrap();
    assert_eq!(mid.status, ReviewStatus::Running);

    // running → completed (verdict + score + raw json persisted)
    let completed = store
        .complete_review(
            &review.id,
            ReviewVerdict::Promising,
            0.82,
            71,
            "Strategy shows positive expectancy with manageable drawdown.",
            r#"{"verdict":"promising"}"#,
        )
        .await
        .unwrap();
    assert!(completed);

    let done = store.get_review(&review.id).await.unwrap().unwrap();
    assert_eq!(done.status, ReviewStatus::Completed);
    assert_eq!(done.verdict, Some(ReviewVerdict::Promising));
    assert_eq!(done.confidence, Some(0.82));
    assert_eq!(done.score, Some(71));
    assert_eq!(
        done.raw_output_json.as_deref(),
        Some(r#"{"verdict":"promising"}"#)
    );

    // Terminal reviews never revive.
    let revived = store.begin_review_running(&done.id).await.unwrap();
    assert!(!revived);
}

#[tokio::test]
async fn review_fail_records_error_and_blocks_further_transitions() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let review = EvalReview::new_queued(run.id.clone(), "fast-trader-agent".into());
    store.create_review(&review).await.unwrap();
    store.begin_review_running(&review.id).await.unwrap();

    let failed = store
        .fail_review(&review.id, "provider 'anthropic' returned 500")
        .await
        .unwrap();
    assert!(failed);

    let got = store.get_review(&review.id).await.unwrap().unwrap();
    assert_eq!(got.status, ReviewStatus::Failed);
    assert_eq!(
        got.error.as_deref(),
        Some("provider 'anthropic' returned 500")
    );

    // Completing a failed review must not flip it back to completed.
    let revived = store
        .complete_review(
            &review.id,
            ReviewVerdict::Inconclusive,
            0.0,
            0,
            "ignored",
            "{}",
        )
        .await
        .unwrap();
    assert!(!revived);
}

#[tokio::test]
async fn list_reviews_for_run_orders_newest_first() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let mut a = EvalReview::new_queued(run.id.clone(), "reasoning-agent".into());
    a.created_at = Utc::now() - chrono::Duration::seconds(60);
    a.updated_at = a.created_at;
    store.create_review(&a).await.unwrap();

    let b = EvalReview::new_queued(run.id.clone(), "risk-agent".into());
    store.create_review(&b).await.unwrap();

    let reviews = store.list_reviews_for_run(&run.id).await.unwrap();
    assert_eq!(reviews.len(), 2);
    assert_eq!(reviews[0].id, b.id, "newest review must come first");
    assert_eq!(reviews[1].id, a.id);
}

#[tokio::test]
async fn legacy_finding_round_trips_unchanged() {
    // Extractor-shaped findings (no review parent) must round-trip through
    // the v2 schema unchanged so existing callers keep working.
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let f = Finding {
        id: ulid::Ulid::new().to_string(),
        run_id: run.id.clone(),
        kind: "overtrading".into(),
        severity: Severity::Warning,
        summary: "30 decisions in 4h".into(),
        evidence: serde_json::json!({"metric_name": "n_decisions", "value": 30}),
        extracted_at: Utc::now(),
        schema_version: "1".into(),
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
    assert_eq!(got.kind, "overtrading");
    assert_eq!(got.severity, Severity::Warning);
    assert!(got.eval_review_id.is_none());
    assert!(got.review_type.is_none());
    assert!(got.title.is_none());
    assert!(got.confidence.is_none());
}

#[tokio::test]
async fn review_finding_round_trips_with_v2_columns_populated() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let review = EvalReview::new_queued(run.id.clone(), "reasoning-agent".into());
    store.create_review(&review).await.unwrap();

    let created_at = Utc::now();
    let f = Finding {
        id: ulid::Ulid::new().to_string(),
        run_id: run.id.clone(),
        // Legacy fields map review type/title to kind/summary so existing
        // readers see something sensible until they move to v2.
        kind: "performance".into(),
        severity: Severity::Warning,
        summary: "Total return below baseline".into(),
        evidence: serde_json::json!({
            "kind": "metric",
            "reference": "total_return_pct",
            "value": -3.2
        }),
        extracted_at: created_at,
        schema_version: "2".into(),
        eval_review_id: Some(review.id.clone()),
        review_type: Some("performance".into()),
        confidence: Some(0.74),
        title: Some("Total return below baseline".into()),
        description: Some("Strategy lost 3.2% over the test window; baseline was flat.".into()),
        recommendation: Some("Re-test in a trending regime to isolate sensitivity.".into()),
        created_at: Some(created_at),
    };
    store.record_finding(&f).await.unwrap();

    let by_run = store.read_findings(&run.id).await.unwrap();
    assert_eq!(by_run.len(), 1);
    let got = &by_run[0];
    assert_eq!(got.eval_review_id.as_deref(), Some(review.id.as_str()));
    assert_eq!(got.review_type.as_deref(), Some("performance"));
    assert_eq!(got.confidence, Some(0.74));
    assert_eq!(got.title.as_deref(), Some("Total return below baseline"));
    assert!(got.description.is_some());
    assert!(got.recommendation.is_some());
    assert!(got.created_at.is_some());

    // The dedicated review accessor returns only review-linked rows.
    let by_review = store.read_findings_for_review(&review.id).await.unwrap();
    assert_eq!(by_review.len(), 1);
    assert_eq!(by_review[0].id, f.id);
}

#[tokio::test]
async fn read_findings_for_review_excludes_legacy_extractor_rows() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    // Legacy finding (no review parent).
    let legacy = Finding {
        id: ulid::Ulid::new().to_string(),
        run_id: run.id.clone(),
        kind: "tail_risk".into(),
        severity: Severity::Critical,
        summary: "fat tail".into(),
        evidence: serde_json::json!({"sigma": 4.2}),
        extracted_at: Utc::now(),
        schema_version: "1".into(),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: None,
        description: None,
        recommendation: None,
        created_at: None,
    };
    store.record_finding(&legacy).await.unwrap();

    let review = EvalReview::new_queued(run.id.clone(), "risk-agent".into());
    store.create_review(&review).await.unwrap();
    let review_finding = Finding {
        id: ulid::Ulid::new().to_string(),
        run_id: run.id.clone(),
        kind: "risk".into(),
        severity: Severity::Critical,
        summary: "Position sizing exceeded policy".into(),
        evidence: serde_json::json!({"kind": "trade", "reference": "decision_4"}),
        extracted_at: Utc::now(),
        schema_version: "2".into(),
        eval_review_id: Some(review.id.clone()),
        review_type: Some("risk".into()),
        confidence: Some(0.91),
        title: Some("Position sizing exceeded policy".into()),
        description: Some("Position 4 was 2.1x policy.".into()),
        recommendation: Some("Cap order size in risk gate.".into()),
        created_at: Some(Utc::now()),
    };
    store.record_finding(&review_finding).await.unwrap();

    let all = store.read_findings(&run.id).await.unwrap();
    assert_eq!(all.len(), 2);

    let scoped = store.read_findings_for_review(&review.id).await.unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].id, review_finding.id);
}

#[test]
fn review_status_round_trips_for_every_variant() {
    for s in [
        ReviewStatus::Queued,
        ReviewStatus::Running,
        ReviewStatus::Completed,
        ReviewStatus::Failed,
    ] {
        let parsed = ReviewStatus::parse(s.as_str()).unwrap();
        assert_eq!(parsed, s);
    }
    assert!(ReviewStatus::Completed.is_terminal());
    assert!(ReviewStatus::Failed.is_terminal());
    assert!(!ReviewStatus::Queued.is_terminal());
    assert!(!ReviewStatus::Running.is_terminal());
}

#[test]
fn review_verdict_round_trips_for_every_variant() {
    for v in [
        ReviewVerdict::Promising,
        ReviewVerdict::Weak,
        ReviewVerdict::Failed,
        ReviewVerdict::Inconclusive,
    ] {
        let parsed = ReviewVerdict::parse(v.as_str()).unwrap();
        assert_eq!(parsed, v);
    }
}
