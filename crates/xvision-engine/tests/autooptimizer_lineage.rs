use chrono::{TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::gate::GateVerdict;
use xvision_engine::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};

async fn fresh_store() -> LineageStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE lineage_nodes (
            bundle_hash TEXT PRIMARY KEY,
            parent_hash TEXT REFERENCES lineage_nodes(bundle_hash),
            gate_verdict TEXT NOT NULL,
            status TEXT NOT NULL,
            cycle_id TEXT,
            created_at TEXT NOT NULL,
            diversity_score REAL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    LineageStore::new(pool)
}

fn make_node(seed: &[u8], parent: Option<ContentHash>, status: LineageStatus, cycle: &str) -> LineageNode {
    LineageNode {
        bundle_hash: ContentHash::of_bytes(seed),
        parent_hash: parent,
        gate_verdict: GateVerdict::Pass,
        status,
        cycle_id: Some(cycle.to_string()),
        created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
        diversity_score: None,
    }
}

#[tokio::test]
async fn insert_get_round_trip() {
    let store = fresh_store().await;
    let node = LineageNode {
        bundle_hash: ContentHash::of_bytes(b"node-a"),
        parent_hash: None,
        gate_verdict: GateVerdict::Fail {
            reason: "test rejection".into(),
        },
        status: LineageStatus::Rejected,
        cycle_id: Some("cycle-x".into()),
        created_at: Utc.with_ymd_and_hms(2026, 5, 29, 10, 0, 0).unwrap(),
        diversity_score: None,
    };
    store.insert(&node).await.unwrap();
    let back = store.get(&node.bundle_hash).await.unwrap().unwrap();
    assert_eq!(back.bundle_hash, node.bundle_hash);
    assert_eq!(back.parent_hash, None);
    assert!(matches!(back.gate_verdict, GateVerdict::Fail { .. }));
    assert_eq!(back.status, LineageStatus::Rejected);
    assert_eq!(back.cycle_id.as_deref(), Some("cycle-x"));
    assert_eq!(back.created_at, node.created_at);
}

#[tokio::test]
async fn children_of_returns_only_direct_children() {
    let store = fresh_store().await;
    let parent = make_node(b"parent", None, LineageStatus::Active, "c1");
    let child1 = make_node(b"child1", Some(parent.bundle_hash), LineageStatus::Active, "c1");
    let child2 = make_node(b"child2", Some(parent.bundle_hash), LineageStatus::Active, "c1");
    let grandchild = make_node(b"grand", Some(child1.bundle_hash), LineageStatus::Active, "c1");
    store.insert(&parent).await.unwrap();
    store.insert(&child1).await.unwrap();
    store.insert(&child2).await.unwrap();
    store.insert(&grandchild).await.unwrap();

    let children = store.children_of(&parent.bundle_hash).await.unwrap();
    assert_eq!(children.len(), 2);
    let hashes: Vec<ContentHash> = children.iter().map(|n| n.bundle_hash).collect();
    assert!(hashes.contains(&child1.bundle_hash));
    assert!(hashes.contains(&child2.bundle_hash));

    let grandchildren = store.children_of(&child1.bundle_hash).await.unwrap();
    assert_eq!(grandchildren.len(), 1);
    assert_eq!(grandchildren[0].bundle_hash, grandchild.bundle_hash);
}

#[tokio::test]
async fn active_leaves_excludes_nodes_with_active_descendant() {
    let store = fresh_store().await;
    let a = make_node(b"a", None, LineageStatus::Active, "c2");
    let b = make_node(b"b", Some(a.bundle_hash), LineageStatus::Active, "c2");
    let c = make_node(b"c", Some(b.bundle_hash), LineageStatus::Active, "c2");
    store.insert(&a).await.unwrap();
    store.insert(&b).await.unwrap();
    store.insert(&c).await.unwrap();

    let leaves = store.active_leaves().await.unwrap();
    assert_eq!(leaves.len(), 1);
    assert_eq!(leaves[0].bundle_hash, c.bundle_hash);

    let c_rejected = LineageNode {
        status: LineageStatus::Rejected,
        ..c
    };
    store.insert(&c_rejected).await.unwrap();
    let leaves2 = store.active_leaves().await.unwrap();
    assert_eq!(leaves2.len(), 1);
    assert_eq!(leaves2[0].bundle_hash, b.bundle_hash);
}

#[tokio::test]
async fn get_returns_none_for_absent_hash() {
    let store = fresh_store().await;
    let absent = ContentHash::of_bytes(b"never-inserted");
    let result = store.get(&absent).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn children_of_returns_empty_for_childless_node() {
    let store = fresh_store().await;
    let leaf = make_node(b"lone-leaf", None, LineageStatus::Active, "cx");
    store.insert(&leaf).await.unwrap();
    let children = store.children_of(&leaf.bundle_hash).await.unwrap();
    assert!(children.is_empty());
}

#[tokio::test]
async fn active_leaves_on_empty_store_returns_empty() {
    let store = fresh_store().await;
    let leaves = store.active_leaves().await.unwrap();
    assert!(leaves.is_empty());
}

#[tokio::test]
async fn active_leaves_excludes_all_rejected_nodes() {
    let store = fresh_store().await;
    let r1 = make_node(b"r1", None, LineageStatus::Rejected, "c3");
    let r2 = make_node(b"r2", None, LineageStatus::Rejected, "c3");
    store.insert(&r1).await.unwrap();
    store.insert(&r2).await.unwrap();
    let leaves = store.active_leaves().await.unwrap();
    assert!(leaves.is_empty(), "all-rejected store must have no active leaves");
}
