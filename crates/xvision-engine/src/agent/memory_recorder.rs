//! V2D auto-recall + auto-write recorder.
//!
//! Sits between `execute_slot` and `xvision_memory::MemoryStore`.
//! Resolves the slot's `MemoryMode` + `agent_id` to a namespace,
//! runs a top-k recall (Patterns only, filtered by training-window
//! vs. the current scenario start) before dispatch, and writes the
//! post-dispatch decision into the same namespace as an Observation.
//!
//! Cortex F+L+T (2026-05-21):
//! - F (structural): writes are Observations; recall reads Patterns.
//! - L (rhetorical): `render_recalled_patterns` wraps each match in
//!   case-law framing before it goes into the system prompt.
//! - T (temporal): `recall` forwards `current_scenario_start` to the
//!   store; backtest dispatchers pass `Some(scenario.time_window.start)`
//!   so Patterns trained inside the scenario can't leak.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use xvision_memory::embedder::{Embedder, StaticEmbedder};
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, MemoryMatch, MemoryMode, Namespace, Tier};

#[derive(Debug)]
pub enum RecallResult {
    /// `memory_mode == Off`. No recall attempted.
    Skipped,
    /// Recall completed; zero-or-more hits.
    Hits {
        namespace: String,
        matches: Vec<MemoryMatch>,
        /// V2D provenance — the per-decision identifier this recall fed
        /// into. Mirrors the caller's `decision_id` argument verbatim so
        /// downstream consumers (observability emit, eval-review join)
        /// can answer "which memories influenced decision N." The engine
        /// convention is `(run_id, scenario_id, cycle_idx)` as the
        /// decision tuple; `cycle_idx` is the per-decision integer and
        /// is carried here as `i64`. `0` is the safe default for
        /// non-eval call sites (CLI rehearsal, unit tests) where there's
        /// no surrounding decision loop.
        decision_id: i64,
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

    /// Pattern-tier recall. `current_scenario_start` is forwarded to the
    /// store so backtest dispatchers can exclude Patterns trained inside
    /// the scenario window (`None` = live/paper, no temporal filter).
    ///
    /// `decision_id` is the per-decision identifier the recall feeds
    /// into; the V2D dispatcher passes `SlotInput.cycle_idx`. The value
    /// is echoed back in [`RecallResult::Hits::decision_id`] so the
    /// caller can surface it on the `memory_recall` observability event
    /// without re-plumbing scope. `0` is the safe default for non-eval
    /// callers (legacy `LLMSlot` pipeline, unit tests) — no per-decision
    /// loop exists to id, and the recorder is typically `None` or
    /// `mode = Off` on those paths anyway.
    pub async fn recall(
        &self,
        mode: MemoryMode,
        agent_id: &str,
        query_text: &str,
        k: usize,
        current_scenario_start: Option<DateTime<Utc>>,
        decision_id: i64,
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
        let hits = self
            .store
            .query(ns.as_str(), &q, k, current_scenario_start)
            .await?;
        Ok(RecallResult::Hits {
            namespace: ns.as_str().to_string(),
            matches: hits,
            decision_id,
        })
    }

    /// Observation-tier write. Caller must supply full provenance
    /// (`run_id`, `scenario_id`, `cycle_idx`) — the store rejects
    /// Observations without it. Returns the new item's id.
    pub async fn record(
        &self,
        mode: MemoryMode,
        agent_id: &str,
        decision_text: &str,
        run_id: String,
        scenario_id: String,
        cycle_idx: i64,
        source_window_start: DateTime<Utc>,
        source_window_end: DateTime<Utc>,
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
            tier: Tier::Observation,
            text: decision_text.to_string(),
            embedding: emb,
            created_at: chrono::Utc::now(),
            run_id: Some(run_id),
            scenario_id: Some(scenario_id),
            cycle_idx: Some(cycle_idx),
            source_window_start: Some(source_window_start),
            source_window_end: Some(source_window_end),
            training_window_end: None,
            promotion_state: None,
            attestation_id: None,
            forgotten_at: None,
        };
        self.store.upsert_observation(&item, embedder.id()).await?;
        Ok(Some(id))
    }
}

/// Truncate `text` to at most 160 chars, appending `…` when trimmed.
fn preview(text: &str) -> String {
    let mut s: String = text.chars().take(160).collect();
    if text.chars().count() > 160 {
        s.push('…');
    }
    s
}

/// Render the case-law framed `<prior_observations>` block prepended
/// to the slot's system prompt when V2D recall surfaces Pattern hits.
///
/// The tag stays `<prior_observations>` for back-compat with the Phase
/// 5 UI MemoryPanel; the *content* is the cortex L wrapper — each
/// retrieved Pattern is framed as a precedent the model should compare
/// to the present cycle, not as a fact to imitate.
pub fn render_recalled_patterns(matches: &[MemoryMatch]) -> String {
    let mut out = String::from("<prior_observations>\n");
    for m in matches {
        out.push_str(&format!(
            "A prior decision noted: \"{}\". Consider whether this \
             situation matches the present cycle.\n\n",
            preview(&m.text),
        ));
    }
    out.push_str("</prior_observations>");
    out
}
