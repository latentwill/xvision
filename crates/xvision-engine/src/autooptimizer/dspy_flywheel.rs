use std::sync::Arc;

use chrono::Utc;
use sqlx::SqlitePool;
use ulid::Ulid;
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, Tier};

use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::dspy_bridge::DspyBridge;
use crate::autooptimizer::judge::Finding;
use crate::autooptimizer::pattern_snapshot::{PatternSnapshot, PatternSnapshotStore};

const EMBEDDER_ID: &str = "autooptimizer-static-v1";

fn static_embedding() -> Vec<f32> {
    vec![1.0f32]
}

fn finding_to_observation(finding: &Finding, namespace: &str, cycle_id: &str) -> MemoryItem {
    let now = Utc::now();
    MemoryItem {
        id: Ulid::new().to_string(),
        namespace: namespace.to_string(),
        tier: Tier::Observation,
        text: format!("[{}] {}", finding.code, finding.summary),
        embedding: static_embedding(),
        created_at: now,
        run_id: Some(cycle_id.to_string()),
        scenario_id: Some("autooptimizer".to_string()),
        cycle_idx: Some(0),
        source_window_start: Some(now),
        source_window_end: Some(now),
        training_window_end: None,
        promotion_state: None,
        attestation_id: None,
        forgotten_at: None,
    }
}

pub async fn write_cycle_findings(
    store: &MemoryStore,
    namespace: &str,
    findings: &[Finding],
    cycle_id: &str,
) -> anyhow::Result<()> {
    assert!(findings.len() <= 256, "findings count exceeds bound");
    for finding in findings {
        let item = finding_to_observation(finding, namespace, cycle_id);
        store.upsert_observation(&item, EMBEDDER_ID).await?;
    }
    Ok(())
}

pub async fn query_dsr_prefix(store: &MemoryStore, namespace: &str) -> anyhow::Result<Option<String>> {
    let matches = store.query(namespace, &static_embedding(), 1, None).await?;
    Ok(matches.into_iter().next().map(|m| m.text))
}

async fn persist_compiled_pattern(
    store: &MemoryStore,
    namespace: &str,
    instruction: &str,
) -> anyhow::Result<()> {
    if instruction.is_empty() {
        return Ok(());
    }
    let now = Utc::now();
    let item = MemoryItem {
        id: Ulid::new().to_string(),
        namespace: namespace.to_string(),
        tier: Tier::Pattern,
        text: instruction.to_string(),
        embedding: static_embedding(),
        created_at: now,
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        source_window_start: None,
        source_window_end: None,
        training_window_end: Some(now),
        promotion_state: Some("active".to_string()),
        attestation_id: None,
        forgotten_at: None,
    };
    store.upsert_pattern(&item, EMBEDDER_ID).await
}

/// Returns the snapshot id when a compile ran and a pattern was persisted;
/// `None` when the observation count was below the threshold.
async fn maybe_trigger_compile(
    mem_store: &MemoryStore,
    xvn_pool: &SqlitePool,
    bridge: &dyn DspyBridge,
    namespace: &str,
    threshold: usize,
) -> anyhow::Result<Option<String>> {
    let count = mem_store.count_live_observations(namespace).await?;
    if count < threshold as u64 {
        return Ok(None);
    }
    let all_observations = mem_store.list_live_observations(namespace, threshold).await?;

    // Phase 6: split 80/20 for held-out validation (Chen 2026, held-out KL-endpoint).
    // If there are ≥ 5 observations, hold out ~20% for validation.
    let (train, held_out) = if all_observations.len() >= 5 {
        let split_at = (all_observations.len() * 4 / 5).max(1);
        let (t, h) = all_observations.split_at(split_at);
        (t.to_vec(), h.to_vec())
    } else {
        (all_observations, vec![])
    };

    let result = bridge.compile(namespace, &train).await?;
    persist_compiled_pattern(mem_store, namespace, &result.instruction).await?;

    // Held-out validation: if the winning instruction scores significantly worse
    // on held-out observations, flag it as overfit.
    if !held_out.is_empty() && !result.demos.is_empty() {
        // Use the stored scores from compile (demos) as a proxy for held-out check.
        // GEPA's demos carry per-observation scores; the held-out check re-uses
        // the train scores for now. Full held-out re-scoring requires a separate
        // GEPA score() call which we defer to Phase 4.
        let train_mean: f64 =
            result.demos.iter().filter_map(|d| d.score).sum::<f64>() / result.demos.len().max(1) as f64;
        tracing::info!(
            namespace,
            train_n = train.len(),
            held_out_n = held_out.len(),
            train_mean_score = train_mean,
            "DSPy compile held-out split: {}/{} train/held-out observations",
            train.len(),
            held_out.len(),
        );
    }

    if result.instruction.is_empty() {
        return Ok(None);
    }

    // Find the prior snapshot for this namespace to link the DAG parent_id.
    let snap_store = PatternSnapshotStore::new(xvn_pool.clone());
    let parent_id = snap_store
        .latest_for_namespace(namespace)
        .await
        .ok()
        .flatten()
        .map(|s| s.id);

    let snapshot = PatternSnapshot::new(
        namespace,
        &result.instruction,
        result.demos,
        "delta_sharpe",
        &result.optimizer_name,
        result.provenance,
        result.rng_seed,
        parent_id,
    );
    let snapshot_id = snapshot.id.clone();
    snap_store.insert(&snapshot).await?;
    Ok(Some(snapshot_id))
}

/// Carrier for DSPy flywheel state threaded through the optimizer cycle.
pub struct DspyContext {
    pub store: Arc<MemoryStore>,
    pub bridge: Arc<dyn DspyBridge>,
    pub namespace: String,
    pub pool: SqlitePool,
}

/// Called after judge findings are emitted for an active child node.
/// Writes findings as Observations and triggers a DSPy compile when the
/// cohort threshold is reached. Skipped entirely when `dspy_enabled=false`.
///
/// Returns the snapshot id (ULID) if a compile ran this call; returns `None`
/// if dspy is disabled, the context is absent, or the observation count is
/// still below the threshold.
pub async fn handle_cycle_dspy(
    config: &AutoOptimizerConfig,
    ctx: Option<&DspyContext>,
    findings: &[Finding],
    cycle_id: &str,
) -> anyhow::Result<Option<String>> {
    if !config.dspy_enabled {
        return Ok(None);
    }
    let Some(ctx) = ctx else {
        return Ok(None);
    };
    write_cycle_findings(&ctx.store, &ctx.namespace, findings, cycle_id).await?;
    let snapshot_id = maybe_trigger_compile(
        &ctx.store,
        &ctx.pool,
        ctx.bridge.as_ref(),
        &ctx.namespace,
        config.dspy_pattern_cohort_threshold,
    )
    .await?;
    Ok(snapshot_id)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::autooptimizer::dspy_bridge::CompileResult;
    use crate::autooptimizer::judge::FindingSeverity;
    use crate::autooptimizer::pattern_snapshot::Provenance;

    struct RecordingBridge {
        called: Arc<Mutex<bool>>,
    }

    #[async_trait]
    impl DspyBridge for RecordingBridge {
        async fn compile(&self, _ns: &str, obs: &[(String, String)]) -> anyhow::Result<CompileResult> {
            *self.called.lock().expect("mutex poisoned") = true;
            Ok(CompileResult {
                instruction: "compiled instruction from recording bridge".to_string(),
                provenance: Provenance::new("test", "model"),
                demos: obs
                    .iter()
                    .map(
                        |(id, text)| crate::autooptimizer::pattern_snapshot::SnapshotDemo {
                            observation_id: id.clone(),
                            text: text.clone(),
                            score: None,
                        },
                    )
                    .collect(),
                optimizer_name: "recording".to_string(),
                rng_seed: 0,
            })
        }
    }

    fn make_finding(code: &str) -> Finding {
        Finding {
            code: code.to_string(),
            severity: FindingSeverity::Info,
            summary: format!("summary for {code}"),
            detail: None,
        }
    }

    fn make_config(enabled: bool, threshold: usize) -> AutoOptimizerConfig {
        AutoOptimizerConfig {
            dspy_enabled: enabled,
            dspy_pattern_cohort_threshold: threshold,
            ..AutoOptimizerConfig::default()
        }
    }

    async fn fresh_xvn_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE autooptimizer_pattern_snapshots (
                id TEXT PRIMARY KEY,
                namespace TEXT NOT NULL,
                instruction TEXT NOT NULL,
                demos_json TEXT NOT NULL,
                signature_hash TEXT NOT NULL,
                metric_name TEXT NOT NULL,
                optimizer_name TEXT NOT NULL,
                optimizer_version TEXT NOT NULL,
                provenance_json TEXT NOT NULL,
                rng_seed INTEGER NOT NULL DEFAULT 0,
                parent_id TEXT,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn dspy_disabled_skips_compilation() {
        let store = Arc::new(MemoryStore::open_in_memory().await.unwrap());
        let called = Arc::new(Mutex::new(false));
        let bridge = Arc::new(RecordingBridge {
            called: Arc::clone(&called),
        });
        let pool = fresh_xvn_pool().await;
        let ctx = DspyContext {
            store: Arc::clone(&store),
            bridge,
            namespace: "test:disabled".to_string(),
            pool,
        };
        let config = make_config(false, 1);
        let findings = vec![make_finding("c1"), make_finding("c2")];

        handle_cycle_dspy(&config, Some(&ctx), &findings, "cycle-1")
            .await
            .unwrap();

        assert!(
            !*called.lock().unwrap(),
            "bridge must not be called when dspy_enabled=false"
        );
        let count = store.count_live_observations("test:disabled").await.unwrap();
        assert_eq!(count, 0, "no observations must be written when disabled");
    }

    #[tokio::test]
    async fn dspy_enabled_triggers_compile_on_threshold() {
        let store = Arc::new(MemoryStore::open_in_memory().await.unwrap());
        let called = Arc::new(Mutex::new(false));
        let bridge = Arc::new(RecordingBridge {
            called: Arc::clone(&called),
        });
        let pool = fresh_xvn_pool().await;
        let ctx = DspyContext {
            store: Arc::clone(&store),
            bridge,
            namespace: "test:enabled".to_string(),
            pool: pool.clone(),
        };
        let threshold = 3;
        let config = make_config(true, threshold);

        // write threshold-1 findings first: bridge must not fire yet
        let initial_findings: Vec<_> = (0..threshold - 1)
            .map(|i| make_finding(&format!("pre-{i}")))
            .collect();
        handle_cycle_dspy(&config, Some(&ctx), &initial_findings, "cycle-1")
            .await
            .unwrap();
        assert!(!*called.lock().unwrap(), "bridge must not fire before threshold");

        // one more finding pushes us to threshold: bridge must fire
        let last_finding = vec![make_finding("final")];
        let snap_id = handle_cycle_dspy(&config, Some(&ctx), &last_finding, "cycle-2")
            .await
            .unwrap();
        assert!(*called.lock().unwrap(), "bridge must be called at threshold");
        assert!(snap_id.is_some(), "compile must return a snapshot id");

        // compiled instruction was persisted as a Pattern
        let prefix = query_dsr_prefix(&store, "test:enabled").await.unwrap();
        assert!(prefix.is_some(), "DSR prefix must be queryable after compilation");
        assert!(
            prefix.unwrap().contains("compiled instruction"),
            "prefix must contain the bridge-returned instruction"
        );

        // snapshot must be in xvn.db
        let snap_store = PatternSnapshotStore::new(pool);
        let snap = snap_store.latest_for_namespace("test:enabled").await.unwrap();
        assert!(snap.is_some(), "snapshot must be persisted to xvn.db");
        assert!(
            snap.unwrap().instruction.contains("compiled instruction"),
            "persisted snapshot instruction must match bridge output"
        );
    }
}
