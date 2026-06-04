//! F13/F19: completed mutation cycles are first-class "historic runs" derived
//! from the lineage graph. Verifies the list/detail aggregation over
//! `lineage_nodes` grouped by `cycle_id`.

use chrono::{TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::cycle_runs::{get_cycle_run, list_cycle_runs};
use xvision_engine::autooptimizer::gate::GateVerdict;
use xvision_engine::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};

async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    xvision_engine::autooptimizer::lineage::ensure_lineage_schema(&pool)
        .await
        .unwrap();
    pool
}

fn node(seed: &[u8], status: LineageStatus, cycle: &str, hour: u32) -> LineageNode {
    LineageNode {
        bundle_hash: ContentHash::of_bytes(seed),
        parent_hash: None,
        gate_verdict: GateVerdict::Pass,
        status,
        cycle_id: Some(cycle.to_string()),
        created_at: Utc.with_ymd_and_hms(2026, 6, 4, hour, 0, 0).unwrap(),
        diversity_score: None,
    }
}

#[tokio::test]
async fn list_cycle_runs_groups_nodes_by_cycle_id() {
    let pool = fresh_pool().await;
    let store = LineageStore::new(pool.clone());
    // cycle A: 1 kept + 1 dropped; cycle B: 1 kept. Plus a NULL-cycle root.
    store
        .insert(&node(b"a1", LineageStatus::Active, "cycle-A", 10))
        .await
        .unwrap();
    store
        .insert(&node(b"a2", LineageStatus::Rejected, "cycle-A", 11))
        .await
        .unwrap();
    store
        .insert(&node(b"b1", LineageStatus::Active, "cycle-B", 12))
        .await
        .unwrap();
    let mut root = node(b"root", LineageStatus::Active, "ignored", 9);
    root.cycle_id = None;
    store.insert(&root).await.unwrap();

    let runs = list_cycle_runs(&pool, 50, 0).await.unwrap();
    // NULL-cycle root is excluded; two cycles remain, newest (B) first.
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].cycle_id, "cycle-B");
    assert_eq!(runs[1].cycle_id, "cycle-A");

    let a = &runs[1];
    assert_eq!(a.node_count, 2);
    assert_eq!(a.active_count, 1);
    assert_eq!(a.rejected_count, 1);
}

#[tokio::test]
async fn get_cycle_run_returns_detail_with_nodes() {
    let pool = fresh_pool().await;
    let store = LineageStore::new(pool.clone());
    store
        .insert(&node(b"a1", LineageStatus::Active, "cycle-A", 10))
        .await
        .unwrap();
    store
        .insert(&node(b"a2", LineageStatus::Rejected, "cycle-A", 11))
        .await
        .unwrap();

    let detail = get_cycle_run(&pool, "cycle-A")
        .await
        .unwrap()
        .expect("cycle exists");
    assert_eq!(detail.summary.cycle_id, "cycle-A");
    assert_eq!(detail.summary.node_count, 2);
    assert_eq!(detail.summary.active_count, 1);
    assert_eq!(detail.nodes.len(), 2);
    // Ordered oldest-first.
    assert_eq!(detail.nodes[0].node.bundle_hash, ContentHash::of_bytes(b"a1"));

    // Unknown cycle → None (so the CLI falls back to the distillation ledger).
    assert!(get_cycle_run(&pool, "no-such-cycle").await.unwrap().is_none());
}

#[tokio::test]
async fn get_cycle_run_enriches_metrics_provenance_and_honesty() {
    let pool = fresh_pool().await;
    let store = LineageStore::new(pool.clone());
    let n = node(b"a1", LineageStatus::Active, "cycle-A", 10);
    store.insert(&n).await.unwrap();
    let hash = n.bundle_hash.to_hex();

    // Seed the F13 side tables the cycle writes during a real run.
    sqlx::query("INSERT INTO lineage_node_metrics (bundle_hash, metrics_day_json, metrics_untouched_json) VALUES (?, ?, ?)")
        .bind(&hash)
        .bind(r#"{"total_return_pct":1.0,"sharpe":1.5,"max_drawdown_pct":2.0,"win_rate":0.6,"n_trades":3,"n_decisions":5}"#)
        .bind(r#"{"total_return_pct":0.5,"sharpe":1.1,"max_drawdown_pct":1.0,"win_rate":0.5,"n_trades":2,"n_decisions":4}"#)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO mutator_attribution (bundle_hash, provider, model, prompt_version, proposed_at, delta_sharpe) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&hash).bind("openrouter").bind("google/gemini-3.1-flash-lite").bind("v1").bind("2026-06-04T10:00:00Z").bind(0.4_f64)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO cycle_honesty_checks (cycle_id, passed, sabotage_variant, message, gate_verdict, parent_hash, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind("cycle-A").bind(1_i64).bind("kill-trades").bind("correctly rejected").bind("fail").bind(&hash).bind("2026-06-04T10:00:00Z")
        .execute(&pool).await.unwrap();

    let detail = get_cycle_run(&pool, "cycle-A")
        .await
        .unwrap()
        .expect("cycle exists");
    let cn = &detail.nodes[0];
    assert_eq!(cn.metrics_day.as_ref().unwrap().sharpe, 1.5);
    assert_eq!(cn.metrics_untouched.as_ref().unwrap().sharpe, 1.1);
    let prov = cn.provenance.as_ref().expect("provenance present");
    assert_eq!(prov.provider, "openrouter");
    assert_eq!(prov.delta_sharpe, Some(0.4));
    let h = detail.honesty_check.as_ref().expect("honesty check present");
    assert!(h.passed);
    assert_eq!(h.sabotage_variant, "kill-trades");
}
