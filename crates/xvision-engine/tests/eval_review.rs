//! Tests for the eval-review data-model foundation (migrations 016/017,
//! `EvalReview` / `AgentProfile` types, review-linked finding columns).
//!
//! Scope is persistence only — review-payload assembly, prompt/response
//! contract, and API/CLI wiring belong to downstream execution-board
//! tracks (`eval-review-agent-engine`, `eval-review-api-cli`,
//! `eval-review-run-detail-ui`).

use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use xvision_engine::eval::findings::{Finding, Severity};
use xvision_engine::eval::review::{EvalReview, ReviewAnnotation, ReviewStatus, ReviewVerdict};
use xvision_engine::eval::{MetricsSummary, Run, RunMode, RunStatus, RunStore};

/// Build an in-memory pool with every migration that touches eval state
/// applied — the same prefix `ApiContext::open` walks at startup.
async fn pool_with_migrations() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        // V2E trace-surface: evidence_cycle_ids_json + produced_by_check columns.
        include_str!("../migrations/026_trace_surface_foundation.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

#[tokio::test]
async fn migrated_memory_pool_uses_only_one_connection() {
    let pool = pool_with_migrations().await;
    let mut conn = pool.acquire().await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_profiles")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(count, 4);
    assert!(
        pool.try_acquire().is_none(),
        "in-memory SQLite test pool must not open a second isolated connection"
    );
}

async fn finalized_run(store: &RunStore) -> Run {
    let mut r = Run::new_queued("agent-h".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    r.status = RunStatus::Queued;
    store.create(&r).await.unwrap();
    store.ensure_agent_run_baseline(&r.id, "hash_only").await.unwrap();
    let metrics = MetricsSummary {
        total_return_pct: 4.2,
        sharpe: 0.6,
        max_drawdown_pct: 6.1,
        win_rate: 0.55,
        n_trades: 18,
        n_decisions: 60,
        baselines: None,
        ..Default::default()
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
    sqlx::query(
        "UPDATE agent_profiles \
         SET system_prompt = 'custom operator prompt', provider = 'custom-provider', enabled = 0 \
         WHERE id = 'fast-trader-agent'",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/013_cli_jobs.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/018_agent_run_observability.sql"))
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

    let row = sqlx::query(
        "SELECT system_prompt, provider, enabled FROM agent_profiles WHERE id = 'fast-trader-agent'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let system_prompt: String = row.try_get("system_prompt").unwrap();
    let provider: String = row.try_get("provider").unwrap();
    let enabled: bool = row.try_get("enabled").unwrap();
    assert_eq!(system_prompt, "custom operator prompt");
    assert_eq!(provider, "custom-provider");
    assert!(
        !enabled,
        "seed re-apply must not overwrite customized enabled flag"
    );
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

    let got = store
        .get_review(&review.id)
        .await
        .unwrap()
        .expect("review present");
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
async fn complete_review_with_annotations_round_trips() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let review = EvalReview::new_queued(run.id.clone(), "risk-agent".into());
    store.create_review(&review).await.unwrap();
    store.begin_review_running(&review.id).await.unwrap();

    let annotations = vec![ReviewAnnotation {
        idx: 42,
        side: "top".into(),
        kind: "RISK".into(),
        title: "Drawdown cluster".into(),
        body: "Consecutive loss cluster after volatility expansion.".into(),
        conf: 0.81,
        action: "CAUTION".into(),
        danger: true,
        ts: Some(1_738_368_000.0),
    }];

    let completed = store
        .complete_review_with_annotations(
            &review.id,
            ReviewVerdict::Weak,
            0.74,
            41,
            "Review emitted chart annotations.",
            r#"{"verdict":"weak","annotations":[]}"#,
            &annotations,
        )
        .await
        .unwrap();
    assert!(completed);

    let done = store.get_review(&review.id).await.unwrap().unwrap();
    assert_eq!(done.status, ReviewStatus::Completed);
    assert_eq!(done.annotations, annotations);
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
    assert_eq!(got.error.as_deref(), Some("provider 'anthropic' returned 500"));

    // Completing a failed review must not flip it back to completed.
    let revived = store
        .complete_review(&review.id, ReviewVerdict::Inconclusive, 0.0, 0, "ignored", "{}")
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
        evidence_cycle_ids: None,
        produced_by_check: None,
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
        evidence_cycle_ids: None,
        produced_by_check: Some("review_engine".into()),
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
        evidence_cycle_ids: None,
        produced_by_check: None,
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
        evidence_cycle_ids: None,
        produced_by_check: Some("review_engine".into()),
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

#[tokio::test]
async fn complete_review_rejects_out_of_range_confidence() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let review = EvalReview::new_queued(run.id.clone(), "reasoning-agent".into());
    store.create_review(&review).await.unwrap();
    store.begin_review_running(&review.id).await.unwrap();

    for bad in [-0.0001, 1.0001, -1.0, 2.0, f64::NAN] {
        let err = store
            .complete_review(&review.id, ReviewVerdict::Promising, bad, 50, "x", "{}")
            .await
            .expect_err(&format!("confidence {bad} must be rejected"));
        let msg = err.to_string();
        assert!(
            msg.contains("confidence"),
            "error must name confidence (got: {msg})"
        );
    }

    // After every rejected attempt, the review must still be in `running`
    // — the validation guard short-circuits before the UPDATE.
    let got = store.get_review(&review.id).await.unwrap().unwrap();
    assert_eq!(got.status, ReviewStatus::Running);
}

#[tokio::test]
async fn complete_review_rejects_out_of_range_score() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    let review = EvalReview::new_queued(run.id.clone(), "reasoning-agent".into());
    store.create_review(&review).await.unwrap();
    store.begin_review_running(&review.id).await.unwrap();

    for bad in [-1, 101, i32::MIN, i32::MAX] {
        let err = store
            .complete_review(&review.id, ReviewVerdict::Promising, 0.5, bad, "x", "{}")
            .await
            .expect_err(&format!("score {bad} must be rejected"));
        let msg = err.to_string();
        assert!(msg.contains("score"), "error must name score (got: {msg})");
    }
}

#[tokio::test]
async fn complete_review_accepts_bounds_inclusive() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let run = finalized_run(&store).await;

    // confidence = 0.0, score = 0 (lower bound)
    let lo = EvalReview::new_queued(run.id.clone(), "reasoning-agent".into());
    store.create_review(&lo).await.unwrap();
    store.begin_review_running(&lo.id).await.unwrap();
    let ok = store
        .complete_review(&lo.id, ReviewVerdict::Failed, 0.0, 0, "low end", "{}")
        .await
        .expect("0.0 / 0 must be accepted");
    assert!(ok);

    // confidence = 1.0, score = 100 (upper bound)
    let hi = EvalReview::new_queued(run.id.clone(), "risk-agent".into());
    store.create_review(&hi).await.unwrap();
    store.begin_review_running(&hi.id).await.unwrap();
    let ok = store
        .complete_review(&hi.id, ReviewVerdict::Promising, 1.0, 100, "high end", "{}")
        .await
        .expect("1.0 / 100 must be accepted");
    assert!(ok);
}

#[tokio::test]
async fn db_check_constraint_rejects_out_of_range_confidence_score_on_raw_insert() {
    // Belt-and-suspenders: even if a bypass path constructs SQL that
    // skips `complete_review`'s validation, the DB CHECK constraints
    // from migration 016 must reject the row.
    let pool = pool_with_migrations().await;

    let run = {
        let store = RunStore::new(pool.clone());
        finalized_run(&store).await
    };

    for (confidence, score, label) in [
        (Some(-0.5_f64), Some(50_i64), "negative confidence"),
        (Some(1.5_f64), Some(50_i64), "confidence > 1.0"),
        (Some(0.5_f64), Some(-1_i64), "negative score"),
        (Some(0.5_f64), Some(101_i64), "score > 100"),
    ] {
        let id = ulid::Ulid::new().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let res = sqlx::query(
            "INSERT INTO eval_reviews \
             (id, eval_run_id, agent_profile_id, status, verdict, confidence, score, \
              summary, raw_output_json, error, created_at, updated_at) \
             VALUES (?, ?, ?, 'completed', 'promising', ?, ?, NULL, NULL, NULL, ?, ?)",
        )
        .bind(&id)
        .bind(&run.id)
        .bind("reasoning-agent")
        .bind(confidence)
        .bind(score)
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await;
        assert!(res.is_err(), "{label} must be rejected by CHECK constraint");
    }
}

#[tokio::test]
async fn row_to_review_fails_on_corrupted_score_overflow() {
    // SQLite stores INTEGER as i64; if a buggy migration or external
    // tool slips a value larger than i32 into `score`, the read path
    // must surface a clear error rather than silently wrap.
    let pool = pool_with_migrations().await;

    // Disable the CHECK constraint by inserting through a connection
    // that has `PRAGMA ignore_check_constraints = true`. Easier: use
    // a pool without the migration's CHECK and exercise just the read
    // logic. We need a fresh table without the CHECK to plant the bad
    // row.
    let raw_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE eval_reviews (
            id TEXT PRIMARY KEY,
            eval_run_id TEXT NOT NULL,
            agent_profile_id TEXT NOT NULL,
            status TEXT NOT NULL,
            verdict TEXT,
            confidence REAL,
            score INTEGER,
            summary TEXT,
            raw_output_json TEXT,
            annotations_json TEXT NOT NULL DEFAULT '[]',
            error TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .execute(&raw_pool)
    .await
    .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO eval_reviews \
         (id, eval_run_id, agent_profile_id, status, verdict, confidence, score, \
          summary, raw_output_json, error, created_at, updated_at) \
         VALUES (?, ?, ?, 'completed', 'promising', 0.5, ?, NULL, NULL, NULL, ?, ?)",
    )
    .bind("01CORRUPT0000000000000000")
    .bind("01RUN00000000000000000000")
    .bind("reasoning-agent")
    .bind((i32::MAX as i64) + 1)
    .bind(&now)
    .bind(&now)
    .execute(&raw_pool)
    .await
    .unwrap();

    let store = RunStore::new(raw_pool);
    let err = store
        .get_review("01CORRUPT0000000000000000")
        .await
        .expect_err("overflowed score must surface a read error");
    let msg = format!("{err:?}");
    assert!(msg.contains("score"), "error must name score (got: {msg})");
    assert!(
        msg.contains("does not fit"),
        "error must explain narrowing failure (got: {msg})"
    );
    // Quiet the unused-pool warning.
    let _ = pool;
}
