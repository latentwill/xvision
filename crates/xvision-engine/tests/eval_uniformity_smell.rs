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

mod support;

use chrono::Utc;
use sqlx::{Row, SqlitePool};
use support::eval_review_pool_with_migrations as pool_with_migrations;
use xvision_engine::eval::review::{run_auto_review, AutoReviewOptions, AutoReviewOutcome, ReviewVerdict};
use xvision_engine::eval::{MetricsSummary, Run, RunMode, RunStatus, RunStore};

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

async fn seed_decisions(
    pool: &SqlitePool,
    run_id: &str,
    rows: impl IntoIterator<Item = (usize, &'static str, f64, String)>,
) {
    for (i, action, conviction, justification) in rows {
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
        .bind(action)
        .bind(conviction)
        .bind(justification)
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
    seed_decisions(
        &pool,
        &run.id,
        (0..50).map(|i| (i, "long_open", 0.42, "stub Gemini Flash 3.1 response".to_string())),
    )
    .await;

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
    assert_eq!(
        check, "smell:uniformity",
        "produced_by_check must be smell:uniformity"
    );
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

    seed_decisions(
        &pool,
        &run.id,
        (0..15_usize).map(|i| {
            let action = match i % 3 {
                0 => "long_open",
                1 => "flat",
                _ => "short_open",
            };
            (
                i,
                action,
                0.3 + i as f64 * 0.01,
                format!("unique analysis for bar {i}"),
            )
        }),
    )
    .await;

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
