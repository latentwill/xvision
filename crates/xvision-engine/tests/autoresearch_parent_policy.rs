use chrono::{TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::gate::GateVerdict;
use xvision_engine::autoresearch::lineage::{LineageNode, LineageStatus, LineageStore};
use xvision_engine::autoresearch::parent_policy::{select_parents, ParentPolicy, ScoreField};

async fn fresh_store() -> LineageStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE lineage_nodes (
            bundle_hash TEXT PRIMARY KEY,
            parent_hash TEXT,
            diff_hash TEXT,
            metrics_day_hash TEXT,
            metrics_untouched_hash TEXT,
            gate_verdict TEXT NOT NULL,
            status TEXT NOT NULL,
            cycle_id TEXT,
            created_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    LineageStore::new(pool)
}

fn make_active_node_at(seed: &[u8], minute: u32) -> LineageNode {
    LineageNode {
        bundle_hash: ContentHash::of_bytes(seed),
        parent_hash: None,
        diff_hash: None,
        metrics_day_hash: None,
        metrics_untouched_hash: None,
        gate_verdict: GateVerdict::Pass,
        status: LineageStatus::Active,
        cycle_id: Some("test-cycle".to_string()),
        created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, minute, 0).unwrap(),
    }
}

fn make_active_node(seed: &[u8]) -> LineageNode {
    make_active_node_at(seed, 0)
}

#[tokio::test]
async fn empty_store_returns_empty() {
    let store = fresh_store().await;
    let result = select_parents(&ParentPolicy::RoundRobin, &store, 3, 0).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn returns_at_most_m_results() {
    let store = fresh_store().await;
    for seed in [b"at-a".as_ref(), b"at-b", b"at-c", b"at-d", b"at-e"] {
        store.insert(&make_active_node(seed)).await.unwrap();
    }
    for m in [1usize, 3, 5] {
        let result = select_parents(&ParentPolicy::RoundRobin, &store, m, 0).await.unwrap();
        assert!(result.len() <= m, "must not return more than m results");
    }
}

#[tokio::test]
async fn round_robin_cycles_through_all_active_leaves() {
    let store = fresh_store().await;
    let seeds: &[(&[u8], u32)] = &[
        (b"rr-a", 0),
        (b"rr-b", 1),
        (b"rr-c", 2),
        (b"rr-d", 3),
    ];
    let mut expected: Vec<ContentHash> = Vec::new();
    for (seed, minute) in seeds {
        let node = make_active_node_at(seed, *minute);
        expected.push(node.bundle_hash);
        store.insert(&node).await.unwrap();
    }
    let result = select_parents(&ParentPolicy::RoundRobin, &store, seeds.len(), 0).await.unwrap();
    assert_eq!(result.len(), seeds.len(), "must return all leaves when m == leaf count");
    let result_hashes: Vec<ContentHash> = result.iter().map(|n| n.bundle_hash).collect();
    for h in &expected {
        assert!(result_hashes.contains(h), "round-robin must include every active leaf");
    }
    let unique: std::collections::HashSet<_> = result_hashes.iter().collect();
    assert_eq!(unique.len(), seeds.len(), "round-robin must not return duplicates");
}

#[tokio::test]
async fn round_robin_different_seed_different_start() {
    let store = fresh_store().await;
    // Distinct minutes so sort order is deterministic: [node_a, node_b].
    let node_a = make_active_node_at(b"rr-start-a", 0);
    let node_b = make_active_node_at(b"rr-start-b", 1);
    store.insert(&node_a).await.unwrap();
    store.insert(&node_b).await.unwrap();
    // seed=0 → start = 0 % 2 = 0 → first is node_a
    // seed=1 → start = 1 % 2 = 1 → first is node_b
    let r0 = select_parents(&ParentPolicy::RoundRobin, &store, 1, 0).await.unwrap();
    let r1 = select_parents(&ParentPolicy::RoundRobin, &store, 1, 1).await.unwrap();
    assert_eq!(r0.len(), 1);
    assert_eq!(r1.len(), 1);
    assert_ne!(r0[0].bundle_hash, r1[0].bundle_hash, "different seed must start at different node");
}

#[tokio::test]
async fn top_k_picks_top_k_by_score() {
    let store = fresh_store().await;
    let seeds: &[&[u8]] = &[b"tk-a", b"tk-b", b"tk-c", b"tk-d", b"tk-e"];
    for seed in seeds {
        store.insert(&make_active_node(seed)).await.unwrap();
    }
    // Compute expected top-3 using the same proxy: first 8 bytes as big-endian u64.
    let mut scored: Vec<(ContentHash, u64)> = seeds
        .iter()
        .map(|s| {
            let h = ContentHash::of_bytes(s);
            let score = u64::from_be_bytes(h.as_bytes()[..8].try_into().unwrap());
            (h, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    let expected_top3: Vec<ContentHash> = scored.iter().take(3).map(|(h, _)| *h).collect();

    let policy = ParentPolicy::TopK { k: 3, score_field: ScoreField::Sharpe };
    let result = select_parents(&policy, &store, 3, 0).await.unwrap();
    assert_eq!(result.len(), 3);
    let result_hashes: Vec<ContentHash> = result.iter().map(|n| n.bundle_hash).collect();
    for h in &expected_top3 {
        assert!(result_hashes.contains(h), "top-K result must include highest-scored node");
    }
}

#[tokio::test]
async fn epsilon_greedy_with_zero_equals_top_k() {
    let store = fresh_store().await;
    for seed in [b"eg-a".as_ref(), b"eg-b", b"eg-c", b"eg-d"] {
        store.insert(&make_active_node(seed)).await.unwrap();
    }
    let policy_eg = ParentPolicy::EpsilonGreedy { epsilon: 0.0, score_field: ScoreField::Sharpe };
    let policy_tk = ParentPolicy::TopK { k: 3, score_field: ScoreField::Sharpe };
    let eg = select_parents(&policy_eg, &store, 3, 42).await.unwrap();
    let tk = select_parents(&policy_tk, &store, 3, 42).await.unwrap();
    let eg_hashes: Vec<_> = eg.iter().map(|n| n.bundle_hash).collect();
    let tk_hashes: Vec<_> = tk.iter().map(|n| n.bundle_hash).collect();
    assert_eq!(eg_hashes, tk_hashes, "ε=0 must behave identically to top-K");
}

#[tokio::test]
async fn epsilon_greedy_with_one_is_uniform_random() {
    let store = fresh_store().await;
    let seeds: &[&[u8]] = &[b"ur-a", b"ur-b", b"ur-c", b"ur-d"];
    let valid_hashes: Vec<ContentHash> = seeds.iter().map(|s| ContentHash::of_bytes(s)).collect();
    for seed in seeds {
        store.insert(&make_active_node(seed)).await.unwrap();
    }
    let policy = ParentPolicy::EpsilonGreedy { epsilon: 1.0, score_field: ScoreField::NetReturn };
    let result = select_parents(&policy, &store, 3, 99).await.unwrap();
    assert_eq!(result.len(), 3);
    for node in &result {
        assert!(valid_hashes.contains(&node.bundle_hash), "all results must be valid active leaves");
    }
    let unique: std::collections::HashSet<_> = result.iter().map(|n| n.bundle_hash).collect();
    assert_eq!(unique.len(), 3, "epsilon-greedy must not produce duplicates");
}

#[tokio::test]
async fn determinism_same_seed_same_selection() {
    let store = fresh_store().await;
    for seed in [b"det-a".as_ref(), b"det-b", b"det-c", b"det-d"] {
        store.insert(&make_active_node(seed)).await.unwrap();
    }
    let policy = ParentPolicy::EpsilonGreedy { epsilon: 0.5, score_field: ScoreField::ProfitFactor };
    let r1 = select_parents(&policy, &store, 3, 7).await.unwrap();
    let r2 = select_parents(&policy, &store, 3, 7).await.unwrap();
    let h1: Vec<_> = r1.iter().map(|n| n.bundle_hash).collect();
    let h2: Vec<_> = r2.iter().map(|n| n.bundle_hash).collect();
    assert_eq!(h1, h2, "same rng_seed must produce the same selection");
}
