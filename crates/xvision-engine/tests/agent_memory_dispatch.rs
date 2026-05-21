//! V2D dispatcher wiring — integration tests for memory recall/write.

use xvision_engine::agent::memory_recorder::{MemoryRecorder, RecallResult};
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, MemoryMode};

#[tokio::test]
async fn recall_returns_empty_when_mode_is_off() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let recorder = MemoryRecorder::new(std::sync::Arc::new(store));
    let r = recorder
        .recall(MemoryMode::Off, "agent-1", "any query text", 5)
        .await
        .unwrap();
    assert!(matches!(r, RecallResult::Skipped));
}

#[tokio::test]
async fn recall_returns_top_k_for_agent_scoped() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    // Pre-seed two items in the agent-scoped namespace.
    for (id, text) in [("m1", "first note"), ("m2", "second note")] {
        store
            .upsert(
                &MemoryItem {
                    id: id.into(),
                    namespace: "agent:agent-1".into(),
                    text: text.into(),
                    embedding: vec![1.0, 0.0],
                    created_at: chrono::Utc::now(),
                    source_run_id: None,
                    source_cycle_id: None,
                },
                "test-embedder",
            )
            .await
            .unwrap();
    }
    let recorder = MemoryRecorder::with_static_embedder(
        std::sync::Arc::new(store),
        "test-embedder",
        vec![1.0, 0.0],
    );
    let r = recorder
        .recall(MemoryMode::AgentScoped, "agent-1", "query", 5)
        .await
        .unwrap();
    match r {
        RecallResult::Hits { matches, namespace } => {
            assert_eq!(namespace, "agent:agent-1");
            assert_eq!(matches.len(), 2);
        }
        other => panic!("expected Hits, got {other:?}"),
    }
}

#[tokio::test]
async fn record_writes_into_correct_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = std::sync::Arc::new(store);
    let recorder = MemoryRecorder::with_static_embedder(
        std::sync::Arc::clone(&store_arc),
        "test-embedder",
        vec![0.0, 1.0],
    );
    recorder
        .record(
            MemoryMode::AgentScoped,
            "agent-1",
            "decision text",
            None,
            None,
        )
        .await
        .unwrap();
    let hits = store_arc
        .query("agent:agent-1", &[0.0, 1.0], 5)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].text, "decision text");
}
