//! Integration test: uniformity smell-tests wired into the auto-reviewer.
//!
//! Verifies that `run_auto_review` detects and persists a
//! `uniform_justification` critical finding when run against a seeded
//! `eval_decisions` table where all rows share the same justification text.
//!
//! The test also confirms:
//!   - the auto-reviewer emits verdict=failed (critical finding present)
//!   - exactly one critical uniformity finding row exists in `eval_findings`
//!   - the finding carries `kind="uniform_justification"` and
//!     `produced_by_check="smell:uniformity"`

use chrono::Utc;
use sqlx::{Row, SqlitePool};
use ulid::Ulid;
use xvision_engine::eval::review::{run_auto_review, AutoReviewOptions, AutoReviewOutcome, ReviewVerdict};
use xvision_engine::eval::{MetricsSummary, Run, RunMode, RunStatus, RunStore};

async fn pool_with_migrations() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    for sql in [
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/026_trace_surface_foundation.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

async fn finalized_run(store: &RunStore) -> Run {
    let mut r = Run::new_queued("agent-stub".into(), "scenario-stub".into(), RunMode::Backtest);
    r.status = RunStatus::Queued;
    store.create(&r).await.unwrap();
    let metrics = MetricsSummary {
        total_return_pct: -7.84,
        sharpe: -7.84,
        max_drawdown_pct: 42.0,
        win_rate: 0.0,
        n_trades: 0,
        n_decisions: 50,
        baselines: None,
        ..Default::default()
    };
    store.begin_running(&r.id).await.unwrap();
    store.finalize(&r.id, &metrics).await.unwrap();
    r.status = RunStatus::Completed;
    r
}

/// Seed `n` identical decisions directly into `eval_decisions`.
/// We insert them via raw SQL because `RunStore` doesn't expose a
/// bulk-insert API — the executor writes decisions one-by-one via
/// `record_decision`, but for the test we bypass that to keep it fast.
async fn seed_identical_decisions(pool: &SqlitePool, run_id: &str, n: usize) {
    for i in 0..n {
        let ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO eval_decisions \
             (run_id, decision_index, timestamp, asset, action, conviction, justification) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(i as i64)
        .bind(&ts)
        .bind("BTC/USDT")
        .bind("long_open")
        .bind(0.42_f64)
        .bind("stub Gemini Flash 3.1 response")
        .execute(pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn auto_review_detects_uniform_justification_in_50_row_run() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool.clone());
    let run = finalized_run(&store).await;

    // Seed 50 decisions all with the same justification — mirrors the
    // QA rerun `01KS4D0MZBD5VGEQ9ACJDRBFBG` (217 decisions, same text).
    seed_identical_decisions(&pool, &run.id, 50).await;

    let outcome = run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .expect("auto-review should not fail");

    // Verdict must be `failed` because the uniformity finding is critical.
    match outcome {
        AutoReviewOutcome::Inserted { verdict, score, .. } => {
            assert_eq!(
                verdict,
                ReviewVerdict::Failed,
                "expected failed verdict when uniform_justification finding present"
            );
            assert!(
                (0..=25).contains(&score),
                "score {score} should be in the failed band [0, 25]"
            );
        }
        AutoReviewOutcome::AlreadyExists { .. } => panic!("expected Inserted on first call"),
    }

    // Exactly one uniformity finding must be in eval_findings.
    let finding_count: i64 = sqlx::query(
        "SELECT COUNT(*) AS c FROM eval_findings \
         WHERE run_id = ? AND kind = 'uniform_justification'",
    )
    .bind(&run.id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .try_get("c")
    .unwrap();

    assert_eq!(
        finding_count, 1,
        "expected exactly one uniform_justification finding, got {finding_count}"
    );

    // Check the finding metadata.
    let row = sqlx::query(
        "SELECT severity, produced_by_check, summary FROM eval_findings \
         WHERE run_id = ? AND kind = 'uniform_justification'",
    )
    .bind(&run.id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let severity: String = row.try_get("severity").unwrap();
    let check: String = row.try_get("produced_by_check").unwrap();
    let summary: String = row.try_get("summary").unwrap();

    assert_eq!(severity, "critical", "uniformity finding must be critical");
    assert_eq!(check, "smell:uniformity", "produced_by_check must be smell:uniformity");
    assert!(
        summary.contains("50"),
        "summary should mention decision count (50): {summary}"
    );
}

#[tokio::test]
async fn auto_review_clean_run_no_uniformity_finding() {
    // Varied decisions: no uniformity finding should be emitted.
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool.clone());
    let run = finalized_run(&store).await;

    for i in 0..15_usize {
        let action = if i % 3 == 0 { "long_open" } else if i % 3 == 1 { "flat" } else { "short_open" };
        let ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO eval_decisions \
             (run_id, decision_index, timestamp, asset, action, conviction, justification) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&run.id)
        .bind(i as i64)
        .bind(&ts)
        .bind("BTC/USDT")
        .bind(action)
        .bind(0.3 + i as f64 * 0.01)
        .bind(format!("unique analysis for bar {i}"))
        .execute(&pool)
        .await
        .unwrap();
    }

    run_auto_review(&store, &run.id, AutoReviewOptions::default())
        .await
        .expect("auto-review should not fail");

    let uniformity_count: i64 = sqlx::query(
        "SELECT COUNT(*) AS c FROM eval_findings \
         WHERE run_id = ? AND kind IN \
         ('uniform_justification', 'uniform_decision', 'near_uniform_justification')",
    )
    .bind(&run.id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .try_get("c")
    .unwrap();

    assert_eq!(
        uniformity_count, 0,
        "expected no uniformity findings for varied decisions"
    );
}
