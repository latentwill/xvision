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
    assert_eq!(detail.nodes[0].bundle_hash, ContentHash::of_bytes(b"a1"));

    // Unknown cycle → None (so the CLI falls back to the distillation ledger).
    assert!(get_cycle_run(&pool, "no-such-cycle").await.unwrap().is_none());
}
