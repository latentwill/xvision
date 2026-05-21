//! V2D auto-recall + auto-write recorder.
//!
//! Sits between `execute_slot` and `xvision_memory::MemoryStore`.
//! Resolves the slot's `MemoryMode` + `agent_id` to a namespace,
//! runs a top-k recall before dispatch, and writes the post-dispatch
//! decision into the same namespace. The provider client used for
//! embeddings is injected — see Decision 3 in the V2D intake.

use std::sync::Arc;

use xvision_memory::embedder::{Embedder, StaticEmbedder};
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, MemoryMatch, MemoryMode, Namespace};

#[derive(Debug)]
pub enum RecallResult {
    /// `memory_mode == Off`. No recall attempted.
    Skipped,
    /// Recall completed; zero-or-more hits.
    Hits {
        namespace: String,
        matches: Vec<MemoryMatch>,
    },
    /// Mode was non-off but no embedder is available for the slot's
    /// provider. Dispatcher emits a `memory_disabled_no_embedder`
    /// event and proceeds without prepending the prior-observations
    /// block.
    NoEmbedder { namespace: String },
}

pub struct MemoryRecorder {
    store: Arc<MemoryStore>,
    embedder: Option<Arc<dyn Embedder>>,
}

impl MemoryRecorder {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self {
            store,
            embedder: None,
        }
    }

    pub fn with_embedder(store: Arc<MemoryStore>, embedder: Arc<dyn Embedder>) -> Self {
        Self {
            store,
            embedder: Some(embedder),
        }
    }

    pub fn with_static_embedder(store: Arc<MemoryStore>, id: &str, vector: Vec<f32>) -> Self {
        Self {
            store,
            embedder: Some(Arc::new(StaticEmbedder::new(id, vector))),
        }
    }

    pub async fn recall(
        &self,
        mode: MemoryMode,
        agent_id: &str,
        query_text: &str,
        k: usize,
    ) -> anyhow::Result<RecallResult> {
        let ns = Namespace::for_mode(mode, agent_id);
        if !ns.is_active() {
            return Ok(RecallResult::Skipped);
        }
        let Some(embedder) = &self.embedder else {
            return Ok(RecallResult::NoEmbedder {
                namespace: ns.as_str().to_string(),
            });
        };
        let q = embedder.embed(query_text).await?;
        let hits = self.store.query(ns.as_str(), &q, k).await?;
        Ok(RecallResult::Hits {
            namespace: ns.as_str().to_string(),
            matches: hits,
        })
    }

    pub async fn record(
        &self,
        mode: MemoryMode,
        agent_id: &str,
        decision_text: &str,
        source_run_id: Option<String>,
        source_cycle_id: Option<String>,
    ) -> anyhow::Result<Option<String>> {
        let ns = Namespace::for_mode(mode, agent_id);
        if !ns.is_active() {
            return Ok(None);
        }
        let Some(embedder) = &self.embedder else {
            return Ok(None);
        };
        let emb = embedder.embed(decision_text).await?;
        let id = ulid::Ulid::new().to_string();
        let item = MemoryItem {
            id: id.clone(),
            namespace: ns.as_str().to_string(),
            text: decision_text.to_string(),
            embedding: emb,
            created_at: chrono::Utc::now(),
            source_run_id,
            source_cycle_id,
        };
        self.store.upsert(&item, embedder.id()).await?;
        Ok(Some(id))
    }
}
