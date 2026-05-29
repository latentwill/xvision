use anyhow::Result;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};

use super::lineage::{LineageNode, LineageStore};

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
            let sorted = sort_by_score(leaves, score_field);
            top_k_select(sorted, m, *k)
        }
        ParentPolicy::EpsilonGreedy { epsilon, score_field } => {
            let sorted = sort_by_score(leaves, score_field);
            epsilon_greedy_select(sorted, m, *epsilon, rng_seed)
        }
    })
}

// Proxy score from first 8 bytes of bundle_hash (big-endian u64, normalized to
// [0.0, 1.0]). Temporary stub — real metrics live behind a blob-store reader
// not yet implemented. Replace with MetricsSnapshot decode once available.
fn score_node(node: &LineageNode, _field: &ScoreField) -> f64 {
    let b = node.bundle_hash.as_bytes();
    let hi = u64::from_be_bytes(b[..8].try_into().expect("bundle_hash is 32 bytes"));
    hi as f64 / u64::MAX as f64
}

fn sort_by_score(mut leaves: Vec<LineageNode>, field: &ScoreField) -> Vec<LineageNode> {
    leaves.sort_by(|a, b| {
        score_node(b, field)
            .partial_cmp(&score_node(a, field))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    leaves
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
