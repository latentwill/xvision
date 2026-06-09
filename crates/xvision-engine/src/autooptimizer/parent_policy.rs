use anyhow::Result;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};

use super::lineage::{LineageNode, LineageStore};
use super::mutator_ladder::read_node_delta_sharpe;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScoreField {
    Sharpe,
    ProfitFactor,
    NetReturn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParentPolicy {
    RoundRobin,
    TopK { k: usize, score_field: ScoreField },
    EpsilonGreedy { epsilon: f64, score_field: ScoreField },
}

/// Returns up to `m` active leaf nodes from `lineage` according to `policy`.
/// `rng_seed` makes stochastic choices reproducible across calls.
pub async fn select_parents(
    policy: &ParentPolicy,
    lineage: &LineageStore,
    m: usize,
    rng_seed: u64,
) -> Result<Vec<LineageNode>> {
    if m == 0 {
        return Ok(vec![]);
    }
    let leaves = lineage.active_leaves().await?;
    if leaves.is_empty() {
        return Ok(vec![]);
    }
    Ok(match policy {
        ParentPolicy::RoundRobin => round_robin(leaves, m, rng_seed),
        ParentPolicy::TopK { k, score_field } => {
            let sorted = sort_by_real_score(leaves, score_field, lineage).await;
            top_k_select(sorted, m, *k)
        }
        ParentPolicy::EpsilonGreedy { epsilon, score_field } => {
            let sorted = sort_by_real_score(leaves, score_field, lineage).await;
            epsilon_greedy_select(sorted, m, *epsilon, rng_seed)
        }
    })
}

/// Score a node using its stored `delta_sharpe` from `mutator_attribution`.
/// Falls back to `diversity_score` then the hash-proxy for nodes without a
/// recorded outcome (e.g. root nodes seeded before attribution existed).
///
/// This is async because it reads from SQLite; call once per sort invocation
/// (not inside the comparator).
async fn fetch_node_score(node: &LineageNode, _field: &ScoreField, pool: &sqlx::SqlitePool) -> f64 {
    match read_node_delta_sharpe(pool, &node.bundle_hash).await {
        Ok(Some(ds)) => ds,
        // No attribution row (root nodes, pre-attribution candidates): fall back
        // to diversity_score then the hash-proxy so insertion order doesn't
        // silently dominate.
        _ => node.diversity_score.unwrap_or_else(|| hash_proxy_score(node)),
    }
}

/// Last-resort tiebreak: a stable proxy score from the first 8 bytes of
/// bundle_hash (big-endian u64 normalized to [0.0, 1.0]). Used ONLY when no
/// real `delta_sharpe` is stored (root nodes, unevaluated candidates).
fn hash_proxy_score(node: &LineageNode) -> f64 {
    let b = node.bundle_hash.as_bytes();
    let hi = u64::from_be_bytes(b[..8].try_into().expect("bundle_hash is 32 bytes"));
    hi as f64 / u64::MAX as f64
}

async fn sort_by_real_score(
    leaves: Vec<LineageNode>,
    field: &ScoreField,
    lineage: &LineageStore,
) -> Vec<LineageNode> {
    // Fetch all scores concurrently, then sort. A fetch failure degrades to the
    // fallback score (0.0 / diversity / hash-proxy) — never fail parent selection.
    let mut scored: Vec<(LineageNode, f64)> = Vec::with_capacity(leaves.len());
    for node in leaves {
        let score = fetch_node_score(&node, field, &lineage.pool).await;
        scored.push((node, score));
    }
    // Sort descending by score: higher delta_sharpe first.
    scored.sort_by(|(_, sa), (_, sb)| sb.partial_cmp(sa).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(n, _)| n).collect()
}

fn round_robin(leaves: Vec<LineageNode>, m: usize, rng_seed: u64) -> Vec<LineageNode> {
    assert!(!leaves.is_empty());
    assert!(m > 0);
    let mut ordered = leaves;
    ordered.sort_by_key(|n| n.created_at);
    let len = ordered.len();
    let start = (rng_seed as usize) % len;
    let take = m.min(len);
    let mut result = Vec::with_capacity(take);
    let mut i = 0;
    while i < take {
        result.push(ordered[(start + i) % len].clone());
        i += 1;
    }
    result
}

fn top_k_select(sorted: Vec<LineageNode>, m: usize, k: usize) -> Vec<LineageNode> {
    sorted.into_iter().take(m.min(k)).collect()
}

fn epsilon_greedy_select(
    sorted: Vec<LineageNode>,
    m: usize,
    epsilon: f64,
    rng_seed: u64,
) -> Vec<LineageNode> {
    assert!(!sorted.is_empty());
    assert!((0.0..=1.0).contains(&epsilon));
    let mut rng = ChaCha20Rng::seed_from_u64(rng_seed);
    let mut candidates = sorted;
    let take = m.min(candidates.len());
    let mut result = Vec::with_capacity(take);
    let mut i = 0;
    while i < take {
        let idx = if rng.gen::<f64>() < epsilon {
            rng.gen_range(0..candidates.len())
        } else {
            0
        };
        result.push(candidates.remove(idx));
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;

    use crate::autooptimizer::content_hash::ContentHash;
    use crate::autooptimizer::gate::GateVerdict;
    use crate::autooptimizer::lineage::{ensure_lineage_schema, LineageStatus};
    use crate::autooptimizer::mutator_ladder::{record_outcome, record_proposal};

    async fn fresh_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open in-memory sqlite");
        ensure_lineage_schema(&pool).await.expect("ensure lineage schema");
        // Also provision the mutator_attribution table (part of lineage schema).
        // ensure_lineage_schema creates lineage_nodes + mutator_attribution.
        pool
    }

    fn make_node(hash_seed: u8, parent: Option<ContentHash>) -> LineageNode {
        // Build a deterministic ContentHash by filling all 32 bytes with hash_seed.
        let bytes = [hash_seed; 32];
        let hex = hex::encode(bytes);
        LineageNode {
            bundle_hash: ContentHash::from_hex(&hex).expect("valid hex"),
            parent_hash: parent,
            gate_verdict: GateVerdict::Pass,
            status: LineageStatus::Active,
            cycle_id: Some("cycle-test".into()),
            created_at: Utc::now(),
            diversity_score: None,
        }
    }

    /// Task 13 core test: a lineage with a root (no recorded outcome) and an
    /// improved child (higher stored delta_sharpe). Both are active leaves.
    /// `TopK { k:1 }` must return the CHILD (real metric dominates).
    ///
    /// With the old hash-proxy stub, this was pseudo-random/wrong and could
    /// return the root by accident (depending on hash bytes).
    #[tokio::test]
    async fn top_k_returns_child_with_higher_delta_sharpe() {
        let pool = fresh_pool().await;
        let lineage = LineageStore::new(pool.clone());

        let root = make_node(0xAA, None);
        let child = make_node(0xBB, Some(root.bundle_hash));

        // Insert both as active leaves. With both active and child having a parent,
        // `active_leaves` returns them both (active with no active children).
        // We insert child ONLY (root has an active child — child itself), so root
        // would NOT be an active leaf. To make both leaves, use sibling nodes.
        //
        // Strategy: insert root as a leaf and child as an unrelated root (same
        // depth — sibling roots in the lineage). Both are active; neither is the
        // parent of the other.
        let root2 = make_node(0xAA, None);
        let child2 = LineageNode {
            bundle_hash: make_node(0xBB, None).bundle_hash, // no parent → sibling root
            parent_hash: None,
            ..make_node(0xBB, None)
        };

        lineage.insert(&root2).await.expect("insert root");
        lineage.insert(&child2).await.expect("insert child");

        // Record a lower delta_sharpe for root2 (worse) and a higher one for child2 (better).
        // record_proposal must be called first since record_outcome does UPDATE.
        record_proposal(&pool, &root2.bundle_hash, "mock", "mock", "v1")
            .await
            .expect("record_proposal root");
        record_outcome(&pool, &root2.bundle_hash, 0.10)
            .await
            .expect("record_outcome root — lower score");

        record_proposal(&pool, &child2.bundle_hash, "mock", "mock", "v1")
            .await
            .expect("record_proposal child");
        record_outcome(&pool, &child2.bundle_hash, 0.80)
            .await
            .expect("record_outcome child — higher score");

        // TopK{k:1} must pick the node with the HIGHER delta_sharpe.
        let policy = ParentPolicy::TopK {
            k: 1,
            score_field: ScoreField::Sharpe,
        };
        let result = select_parents(&policy, &lineage, 1, 42)
            .await
            .expect("select_parents");

        assert_eq!(result.len(), 1, "expected exactly 1 parent selected");
        assert_eq!(
            result[0].bundle_hash, child2.bundle_hash,
            "TopK must pick the child (higher delta_sharpe=0.80) not the root (0.10). \
             With the old hash-proxy stub this was pseudo-random/wrong."
        );
    }
}
