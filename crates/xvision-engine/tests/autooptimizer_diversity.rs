use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::diversity::{
    compute_diversity_score, diversity_decay_for_cycle, record_embedding,
};
use xvision_engine::autooptimizer::gate::GateVerdict;
use xvision_engine::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};
use xvision_observability::BlobStore;

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/048_autooptimizer.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/049_autooptimizer_diversity.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn make_node(seed: &[u8], cycle: &str, status: LineageStatus) -> LineageNode {
    LineageNode {
        bundle_hash: ContentHash::of_bytes(seed),
        parent_hash: None,
        diff_hash: None,
        metrics_day_hash: None,
        metrics_untouched_hash: None,
        gate_verdict: GateVerdict::Pass,
        status,
        cycle_id: Some(cycle.to_string()),
        created_at: Utc::now(),
    }
}

/// Migration 049 must apply to an in-memory pool alongside all prior
/// migrations without error.
#[tokio::test]
async fn migration_applies_cleanly() {
    let _pool = fresh_pool().await;
    // If sqlx::migrate! errors, the test panics — no explicit assertion needed.
}

/// The first node in a cycle has no prior embeddings; its diversity score
/// must be 1.0.
#[tokio::test]
async fn compute_diversity_score_is_1_for_first_node() {
    let pool = fresh_pool().await;
    let dir = TempDir::new().unwrap();
    let blob = BlobStore::new(dir.path().join("blobs"));
    let lineage = LineageStore::new(pool.clone());

    let node = make_node(b"first-node", "cycle-first", LineageStatus::Active);
    lineage.insert(&node).await.unwrap();

    record_embedding(&pool, &blob, &node.bundle_hash, &[1.0_f32, 0.0, 0.0])
        .await
        .unwrap();
    let score = compute_diversity_score(&pool, &blob, &node.bundle_hash)
        .await
        .unwrap();

    assert!(
        (score - 1.0).abs() < 1e-9,
        "first node with no prior embeddings must score 1.0, got {score}"
    );
}

/// A second node whose embedding is identical to the first must receive a
/// diversity score near 0 (cosine similarity = 1.0 → distance ≈ 0).
#[tokio::test]
async fn compute_diversity_score_lower_for_near_duplicate() {
    let pool = fresh_pool().await;
    let dir = TempDir::new().unwrap();
    let blob = BlobStore::new(dir.path().join("blobs"));
    let lineage = LineageStore::new(pool.clone());

    let node_a = make_node(b"dup-a", "cycle-dup", LineageStatus::Active);
    let node_b = make_node(b"dup-b", "cycle-dup", LineageStatus::Active);
    lineage.insert(&node_a).await.unwrap();
    lineage.insert(&node_b).await.unwrap();

    let emb: &[f32] = &[1.0, 0.0, 0.0];

    // Record + compute for node_a first (no priors → 1.0).
    record_embedding(&pool, &blob, &node_a.bundle_hash, emb)
        .await
        .unwrap();
    let score_a = compute_diversity_score(&pool, &blob, &node_a.bundle_hash)
        .await
        .unwrap();

    // Record + compute for node_b (node_a is now a prior with identical emb).
    record_embedding(&pool, &blob, &node_b.bundle_hash, emb)
        .await
        .unwrap();
    let score_b = compute_diversity_score(&pool, &blob, &node_b.bundle_hash)
        .await
        .unwrap();

    assert!(
        (score_a - 1.0).abs() < 1e-9,
        "first node must score 1.0, got {score_a}"
    );
    assert!(
        score_b < 0.01,
        "near-duplicate node must score near 0, got {score_b}"
    );
}

/// `diversity_decay_for_cycle` must return the same value on repeated calls
/// given the same set of stored embeddings / scores.
#[tokio::test]
async fn diversity_decay_for_cycle_is_deterministic() {
    let pool = fresh_pool().await;
    let dir = TempDir::new().unwrap();
    let blob = BlobStore::new(dir.path().join("blobs"));
    let lineage = LineageStore::new(pool.clone());
    let cycle = "cycle-det";

    // Three mutually orthogonal embeddings — maximum pairwise diversity.
    let embeddings: [&[f32]; 3] = [&[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0]];
    for (i, emb) in embeddings.iter().enumerate() {
        let seed = format!("det-node-{i}");
        let node = make_node(seed.as_bytes(), cycle, LineageStatus::Active);
        lineage.insert(&node).await.unwrap();
        record_embedding(&pool, &blob, &node.bundle_hash, emb)
            .await
            .unwrap();
        compute_diversity_score(&pool, &blob, &node.bundle_hash)
            .await
            .unwrap();
    }

    let d1 = diversity_decay_for_cycle(&pool, cycle).await.unwrap();
    let d2 = diversity_decay_for_cycle(&pool, cycle).await.unwrap();

    assert_eq!(d1, d2, "diversity_decay_for_cycle must be deterministic");
    assert!(
        d1 > 0.0,
        "orthogonal embeddings produce non-zero average diversity score, got {d1}"
    );
}
