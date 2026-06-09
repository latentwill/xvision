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
use xvision_observability::Redactor;

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

    /// Explicit-namespace Pattern recall — the shared cortex primitive
    /// for subsurface call sites (autooptimizer Judge in P2; the
    /// Mutator/seed paths in P3/P4) that address a custom namespace
    /// rather than the slot-derived `global` / `agent:{id}` namespaces
    /// `recall` produces via [`Namespace::for_mode`].
    ///
    /// Surfaces **Patterns only** — that is the store's `query`
    /// contract; a freshly written Observation is not recalled here.
    /// `current_scenario_start` is forwarded verbatim so eval callers
    /// keep temporal safety (`Some(scenario.time_window.start)`); `None`
    /// = live/no temporal filter. No embedder → `NoEmbedder` (best-effort
    /// callers degrade to the plain prompt).
    pub async fn recall_in_namespace(
        &self,
        namespace: &str,
        query_text: &str,
        k: usize,
        current_scenario_start: Option<DateTime<Utc>>,
    ) -> anyhow::Result<RecallResult> {
        let Some(embedder) = &self.embedder else {
            return Ok(RecallResult::NoEmbedder {
                namespace: namespace.to_string(),
            });
        };
        let q = embedder.embed(query_text).await?;
        let hits = self.store.query(namespace, &q, k, current_scenario_start).await?;
        Ok(RecallResult::Hits {
            namespace: namespace.to_string(),
            matches: hits,
            // No surrounding per-decision loop on these subsurface call
            // sites; `0` is the conventional non-eval default (mirrors
            // `recall`).
            decision_id: 0,
        })
    }

    /// Explicit-namespace Observation write — the write-back half of
    /// the cortex primitive. Caller supplies full provenance; the source
    /// window doubles as the temporal anchor the distillation pass reads
    /// when computing a Pattern's `training_window_end`. No embedder →
    /// `Ok(None)` (best-effort no-op). Returns the new item id.
    pub async fn record_observation_in_namespace(
        &self,
        namespace: &str,
        text: &str,
        run_id: String,
        scenario_id: String,
        cycle_idx: i64,
        source_window_start: DateTime<Utc>,
        source_window_end: DateTime<Utc>,
    ) -> anyhow::Result<Option<String>> {
        let Some(embedder) = &self.embedder else {
            return Ok(None);
        };
        // Cross-cutting invariant: every memory write routes through the
        // observability redactor so a secret-shaped token (API key, JWT,
        // private key, mnemonic) pasted into an agent/chat surface is never
        // embedded or persisted. This is the single chokepoint for the
        // custom-namespace surfaces (judge/mutator/chat); redacting here is
        // idempotent if a caller already redacted (e.g. the chat rail).
        let redacted = Redactor::new().redact(text).text;
        let emb = embedder.embed(&redacted).await?;
        let id = ulid::Ulid::new().to_string();
        let item = MemoryItem {
            id: id.clone(),
            namespace: namespace.to_string(),
            tier: Tier::Observation,
            text: redacted,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    async fn store() -> Arc<MemoryStore> {
        Arc::new(
            MemoryStore::open_in_memory()
                .await
                .expect("in-memory store opens"),
        )
    }

    fn seed_pattern(ns: &str, text: &str) -> MemoryItem {
        MemoryItem {
            id: ulid::Ulid::new().to_string(),
            namespace: ns.to_string(),
            tier: Tier::Pattern,
            text: text.to_string(),
            // The recorder's StaticEmbedder returns a fixed vector for
            // every input, so any non-empty embedding here matches the
            // query vector (cosine == 1.0) and surfaces deterministically.
            embedding: vec![0.1, 0.2, 0.3],
            created_at: Utc::now(),
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
            source_window_start: None,
            source_window_end: None,
            training_window_end: None,
            promotion_state: Some("active".to_string()),
            attestation_id: None,
            forgotten_at: None,
        }
    }

    #[tokio::test]
    async fn recall_in_namespace_surfaces_seeded_pattern() {
        let store = store().await;
        let pat = seed_pattern("n", "raising leverage past 3x degraded holdout");
        store
            .upsert_pattern(&pat, "static-test")
            .await
            .expect("seed pattern");

        let rec = MemoryRecorder::with_static_embedder(store, "static-test", vec![0.1, 0.2, 0.3]);
        let res = rec
            .recall_in_namespace("n", "leverage", 3, None)
            .await
            .expect("recall ok");
        match res {
            RecallResult::Hits {
                namespace, matches, ..
            } => {
                assert_eq!(namespace, "n");
                assert_eq!(matches.len(), 1, "expected the seeded pattern");
                assert!(matches[0].text.contains("raising leverage"));
            }
            other => panic!("expected Hits, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn record_observation_in_namespace_writes_an_observation() {
        let store = store().await;
        let rec = MemoryRecorder::with_static_embedder(store.clone(), "static-test", vec![0.4, 0.5, 0.6]);

        let win = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let id = rec
            .record_observation_in_namespace(
                "n",
                "obs text",
                "run-1".to_string(),
                "autooptimizer".to_string(),
                0,
                win,
                win,
            )
            .await
            .expect("record ok");
        assert!(id.is_some(), "embedder present → id returned");
        assert_eq!(store.count_live_observations("n").await.expect("count ok"), 1);
    }

    #[tokio::test]
    async fn record_observation_redacts_secret_shaped_tokens() {
        // Cross-cutting closeout: the shared write primitive must redact
        // before persisting so a pasted secret never lands in memory.
        let store = store().await;
        let rec = MemoryRecorder::with_static_embedder(store.clone(), "static-test", vec![0.4, 0.5, 0.6]);
        let win = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let secret = "sk-ant-aaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        rec.record_observation_in_namespace(
            "n",
            &format!("user pasted a key {secret} into chat"),
            "run-1".to_string(),
            "chat".to_string(),
            0,
            win,
            win,
        )
        .await
        .expect("record ok");

        let texts = store.list_live_observation_texts("n", 10).await.expect("list ok");
        assert_eq!(texts.len(), 1);
        assert!(
            !texts[0].contains(secret),
            "raw secret must not be persisted, got: {}",
            texts[0]
        );
    }

    #[tokio::test]
    async fn no_embedder_degrades_silently() {
        let store = store().await;
        let rec = MemoryRecorder::new(store);

        match rec
            .recall_in_namespace("n", "q", 3, None)
            .await
            .expect("recall ok")
        {
            RecallResult::NoEmbedder { namespace } => assert_eq!(namespace, "n"),
            other => panic!("expected NoEmbedder, got {other:?}"),
        }

        let win = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let id = rec
            .record_observation_in_namespace(
                "n",
                "obs",
                "run-1".to_string(),
                "autooptimizer".to_string(),
                0,
                win,
                win,
            )
            .await
            .expect("record ok");
        assert!(id.is_none(), "no embedder → no write");
    }
}
