use xvision_memory::types::{MemoryMode, Namespace};

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

use xvision_memory::types::MemoryItem;

fn make_item(id: &str, ns: &str, text: &str, emb: Vec<f32>) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        namespace: ns.into(),
        text: text.into(),
        embedding: emb,
        created_at: chrono::Utc::now(),
        source_run_id: None,
        source_cycle_id: None,
    }
}

#[tokio::test]
async fn upsert_then_query_returns_top_k_by_cosine() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert(&make_item("a", "global", "alpha", vec![1.0, 0.0, 0.0]), "test-embedder").await.unwrap();
    store.upsert(&make_item("b", "global", "beta",  vec![0.0, 1.0, 0.0]), "test-embedder").await.unwrap();
    store.upsert(&make_item("c", "global", "gamma", vec![0.9, 0.1, 0.0]), "test-embedder").await.unwrap();
    let hits = store.query("global", &[1.0, 0.0, 0.0], 2).await.unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, "a");
    assert_eq!(hits[1].id, "c");
    assert!(hits[0].score > hits[1].score);
}

#[tokio::test]
async fn query_isolates_by_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert(&make_item("a", "agent:A", "alpha", vec![1.0, 0.0]), "test").await.unwrap();
    store.upsert(&make_item("b", "agent:B", "beta",  vec![1.0, 0.0]), "test").await.unwrap();
    let hits_a = store.query("agent:A", &[1.0, 0.0], 5).await.unwrap();
    let hits_b = store.query("agent:B", &[1.0, 0.0], 5).await.unwrap();
    let hits_g = store.query("global",  &[1.0, 0.0], 5).await.unwrap();
    assert_eq!(hits_a.len(), 1);
    assert_eq!(hits_a[0].id, "a");
    assert_eq!(hits_b.len(), 1);
    assert_eq!(hits_b[0].id, "b");
    assert_eq!(hits_g.len(), 0);
}

#[tokio::test]
async fn forget_namespace_clears_only_that_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert(&make_item("a", "agent:A", "alpha", vec![1.0, 0.0]), "test").await.unwrap();
    store.upsert(&make_item("b", "global",  "beta",  vec![1.0, 0.0]), "test").await.unwrap();
    store.forget("agent:A").await.unwrap();
    let hits_a = store.query("agent:A", &[1.0, 0.0], 5).await.unwrap();
    let hits_g = store.query("global",  &[1.0, 0.0], 5).await.unwrap();
    assert!(hits_a.is_empty());
    assert_eq!(hits_g.len(), 1);
}
