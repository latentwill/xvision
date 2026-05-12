//! Migration 007 trigger-based foreign key: eval_runs.scenario_id must
//! reference an existing scenarios.id row.
//!
//! See docs/superpowers/plans/2026-05-11-custom-scenario-2-scenario-table-cli.md
//! Task 7. SQLite can't ALTER TABLE ADD CONSTRAINT after the fact, so the
//! check fires from BEFORE-INSERT/UPDATE triggers (migration 007).

use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext};

#[tokio::test]
async fn run_insert_with_unknown_scenario_rejected() {
    let dir = tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap();

    let err = sqlx::query(
        "INSERT INTO eval_runs (id, strategy_bundle_hash, scenario_id, mode, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("r_bad")
    .bind("hash")
    .bind("sc_does_not_exist")
    .bind("backtest")
    .bind("queued")
    .bind("2026-05-11T00:00:00Z")
    .execute(&ctx.db)
    .await
    .unwrap_err();

    let msg = format!("{err}");
    assert!(
        msg.contains("foreign-key violation"),
        "expected FK trigger to fire on unknown scenario_id, got: {msg}"
    );
}

#[tokio::test]
async fn run_insert_with_seeded_canonical_scenario_succeeds() {
    let dir = tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap();

    // The first-run seed (run_seed_if_needed, called from ApiContext::open)
    // inserts the four canonical scenarios; 'crypto-bull-q1-2025' is one.
    sqlx::query(
        "INSERT INTO eval_runs (id, strategy_bundle_hash, scenario_id, mode, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("r_ok")
    .bind("hash")
    .bind("crypto-bull-q1-2025")
    .bind("backtest")
    .bind("queued")
    .bind("2026-05-11T00:00:00Z")
    .execute(&ctx.db)
    .await
    .expect("insert with seeded canonical scenario_id must succeed");
}

#[tokio::test]
async fn run_update_to_unknown_scenario_rejected() {
    let dir = tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap();

    // Seed a run pointing at a valid (canonical) scenario.
    sqlx::query(
        "INSERT INTO eval_runs (id, strategy_bundle_hash, scenario_id, mode, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("r_upd")
    .bind("hash")
    .bind("crypto-bull-q1-2025")
    .bind("backtest")
    .bind("queued")
    .bind("2026-05-11T00:00:00Z")
    .execute(&ctx.db)
    .await
    .unwrap();

    // Now repoint scenario_id at a non-existent row — the UPDATE trigger
    // must reject it.
    let err = sqlx::query("UPDATE eval_runs SET scenario_id = ? WHERE id = ?")
        .bind("sc_nope")
        .bind("r_upd")
        .execute(&ctx.db)
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("foreign-key violation"),
        "expected FK trigger to fire on UPDATE to unknown scenario_id, got: {msg}"
    );
}
