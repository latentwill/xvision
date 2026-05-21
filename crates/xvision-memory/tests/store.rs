use chrono::TimeZone;
use xvision_memory::types::{MemoryItem, MemoryMode, Namespace, Tier};

#[test]
fn memory_mode_serde_round_trip() {
    for mode in [MemoryMode::Off, MemoryMode::Global, MemoryMode::AgentScoped] {
        let json = serde_json::to_string(&mode).unwrap();
        let back: MemoryMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }
    assert_eq!(serde_json::to_string(&MemoryMode::Off).unwrap(), "\"off\"");
    assert_eq!(serde_json::to_string(&MemoryMode::AgentScoped).unwrap(), "\"agent_scoped\"");
}

#[test]
fn namespace_for_mode_uses_agent_id() {
    assert_eq!(Namespace::for_mode(MemoryMode::Off, "01HZTEST").as_str(), None::<&str>.unwrap_or_default());
    assert_eq!(Namespace::for_mode(MemoryMode::Global, "01HZTEST").as_str(), "global");
    assert_eq!(Namespace::for_mode(MemoryMode::AgentScoped, "01HZTEST").as_str(), "agent:01HZTEST");
}

use xvision_memory::store::MemoryStore;

#[tokio::test]
async fn open_lazy_creates_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory.db");
    let store = MemoryStore::open(&path).await.unwrap();
    // Reopening must be idempotent.
    let _store2 = MemoryStore::open(&path).await.unwrap();
    drop(store);
}

fn observation(id: &str, ns: &str, text: &str, emb: Vec<f32>) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        namespace: ns.into(),
        tier: Tier::Observation,
        text: text.into(),
        embedding: emb,
        created_at: chrono::Utc::now(),
        run_id: Some("run-1".into()),
        scenario_id: Some("scenario-1".into()),
        cycle_idx: Some(0),
        training_window_end: None,
    }
}

fn pattern(id: &str, ns: &str, text: &str, emb: Vec<f32>) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        namespace: ns.into(),
        tier: Tier::Pattern,
        text: text.into(),
        embedding: emb,
        created_at: chrono::Utc::now(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        training_window_end: None,
    }
}

#[tokio::test]
async fn upsert_pattern_then_query_returns_top_k_by_cosine() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert_pattern(&pattern("a", "global", "alpha", vec![1.0, 0.0, 0.0]), "test-embedder").await.unwrap();
    store.upsert_pattern(&pattern("b", "global", "beta",  vec![0.0, 1.0, 0.0]), "test-embedder").await.unwrap();
    store.upsert_pattern(&pattern("c", "global", "gamma", vec![0.9, 0.1, 0.0]), "test-embedder").await.unwrap();
    let hits = store.query("global", &[1.0, 0.0, 0.0], 2, None).await.unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, "a");
    assert_eq!(hits[1].id, "c");
    assert!(hits[0].score > hits[1].score);
}

#[tokio::test]
async fn query_isolates_by_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert_pattern(&pattern("a", "agent:A", "alpha", vec![1.0, 0.0]), "test").await.unwrap();
    store.upsert_pattern(&pattern("b", "agent:B", "beta",  vec![1.0, 0.0]), "test").await.unwrap();
    let hits_a = store.query("agent:A", &[1.0, 0.0], 5, None).await.unwrap();
    let hits_b = store.query("agent:B", &[1.0, 0.0], 5, None).await.unwrap();
    let hits_g = store.query("global",  &[1.0, 0.0], 5, None).await.unwrap();
    assert_eq!(hits_a.len(), 1);
    assert_eq!(hits_a[0].id, "a");
    assert_eq!(hits_b.len(), 1);
    assert_eq!(hits_b[0].id, "b");
    assert_eq!(hits_g.len(), 0);
}

#[tokio::test]
async fn forget_namespace_clears_only_that_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert_pattern(&pattern("a", "agent:A", "alpha", vec![1.0, 0.0]), "test").await.unwrap();
    store.upsert_pattern(&pattern("b", "global",  "beta",  vec![1.0, 0.0]), "test").await.unwrap();
    store.forget("agent:A").await.unwrap();
    let hits_a = store.query("agent:A", &[1.0, 0.0], 5, None).await.unwrap();
    let hits_g = store.query("global",  &[1.0, 0.0], 5, None).await.unwrap();
    assert!(hits_a.is_empty());
    assert_eq!(hits_g.len(), 1);
}

// --- Tier invariant enforcement -----------------------------------------

#[tokio::test]
async fn upsert_observation_requires_provenance() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut item = observation("o1", "global", "obs", vec![1.0, 0.0]);
    item.run_id = None;
    assert!(store.upsert_observation(&item, "test").await.is_err());

    let mut item = observation("o2", "global", "obs", vec![1.0, 0.0]);
    item.scenario_id = None;
    assert!(store.upsert_observation(&item, "test").await.is_err());

    let mut item = observation("o3", "global", "obs", vec![1.0, 0.0]);
    item.cycle_idx = None;
    assert!(store.upsert_observation(&item, "test").await.is_err());
}

#[tokio::test]
async fn upsert_observation_rejects_training_window_end() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut item = observation("o1", "global", "obs", vec![1.0, 0.0]);
    item.training_window_end = Some(chrono::Utc.with_ymd_and_hms(2024, 9, 1, 0, 0, 0).unwrap());
    assert!(store.upsert_observation(&item, "test").await.is_err());
}

#[tokio::test]
async fn upsert_observation_rejects_pattern_tier() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut item = observation("o1", "global", "obs", vec![1.0, 0.0]);
    item.tier = Tier::Pattern;
    assert!(store.upsert_observation(&item, "test").await.is_err());
}

#[tokio::test]
async fn upsert_pattern_rejects_provenance() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut item = pattern("p1", "global", "pat", vec![1.0, 0.0]);
    item.run_id = Some("run-x".into());
    assert!(store.upsert_pattern(&item, "test").await.is_err());

    let mut item = pattern("p2", "global", "pat", vec![1.0, 0.0]);
    item.scenario_id = Some("sc-x".into());
    assert!(store.upsert_pattern(&item, "test").await.is_err());

    let mut item = pattern("p3", "global", "pat", vec![1.0, 0.0]);
    item.cycle_idx = Some(3);
    assert!(store.upsert_pattern(&item, "test").await.is_err());
}

#[tokio::test]
async fn upsert_pattern_rejects_observation_tier() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut item = pattern("p1", "global", "pat", vec![1.0, 0.0]);
    item.tier = Tier::Observation;
    assert!(store.upsert_pattern(&item, "test").await.is_err());
}

// --- Recall filters tier + temporal window ------------------------------

#[tokio::test]
async fn query_only_returns_patterns_never_observations() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_observation(
            &observation("o1", "global", "OBS_TEXT", vec![1.0, 0.0]),
            "test",
        )
        .await
        .unwrap();
    store
        .upsert_pattern(
            &pattern("p1", "global", "PAT_TEXT", vec![1.0, 0.0]),
            "test",
        )
        .await
        .unwrap();
    let hits = store.query("global", &[1.0, 0.0], 5, None).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "p1");
    assert_eq!(hits[0].text, "PAT_TEXT");
}

#[tokio::test]
async fn query_excludes_patterns_inside_scenario_window() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut p = pattern("p1", "global", "PAT_TEXT", vec![1.0, 0.0]);
    p.training_window_end = Some(chrono::Utc.with_ymd_and_hms(2024, 9, 1, 0, 0, 0).unwrap());
    store.upsert_pattern(&p, "test").await.unwrap();

    let before = chrono::Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    let hits = store.query("global", &[1.0, 0.0], 5, Some(before)).await.unwrap();
    assert!(hits.is_empty(), "training window 2024-09-01 must be filtered for scenario starting 2024-08-01");

    let after = chrono::Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap();
    let hits = store.query("global", &[1.0, 0.0], 5, Some(after)).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "p1");
}

#[tokio::test]
async fn query_includes_null_training_window_patterns() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_pattern(
            &pattern("p1", "global", "OPERATOR_WISDOM", vec![1.0, 0.0]),
            "test",
        )
        .await
        .unwrap();
    let any = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let hits = store.query("global", &[1.0, 0.0], 5, Some(any)).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "p1");
}

#[tokio::test]
async fn query_with_no_scenario_start_skips_temporal_filter() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut p = pattern("p1", "global", "PAT_TEXT", vec![1.0, 0.0]);
    p.training_window_end = Some(chrono::Utc.with_ymd_and_hms(2024, 9, 1, 0, 0, 0).unwrap());
    store.upsert_pattern(&p, "test").await.unwrap();

    let hits = store.query("global", &[1.0, 0.0], 5, None).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "p1");
}

#[tokio::test]
async fn demote_pattern_removes_only_the_named_pattern() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert_pattern(&pattern("p1", "global", "one", vec![1.0, 0.0]), "test").await.unwrap();
    store.upsert_pattern(&pattern("p2", "global", "two", vec![0.0, 1.0]), "test").await.unwrap();

    let removed = store.demote_pattern("p1").await.unwrap();
    assert_eq!(removed, 1);

    let hits = store.query("global", &[1.0, 1.0], 5, None).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "p2");
}

#[tokio::test]
async fn demote_pattern_refuses_to_delete_observations() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_observation(
            &observation("o1", "global", "obs", vec![1.0, 0.0]),
            "test",
        )
        .await
        .unwrap();
    let removed = store.demote_pattern("o1").await.unwrap();
    assert_eq!(removed, 0, "demote_pattern must only delete tier='pattern' rows");
}

use xvision_memory::embedder::{Embedder, StaticEmbedder};

#[tokio::test]
async fn static_embedder_returns_configured_vector() {
    let embedder = StaticEmbedder::new("test-embedder", vec![0.5, 0.5, 0.0]);
    let v = embedder.embed("anything").await.unwrap();
    assert_eq!(v, vec![0.5, 0.5, 0.0]);
    assert_eq!(embedder.id(), "test-embedder");
}
