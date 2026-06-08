use std::sync::Arc;

use chrono::Utc;
use ulid::Ulid;
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, Tier};

use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::dspy_bridge::DspyBridge;
use crate::autooptimizer::judge::Finding;

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

/// Returns `true` when a compile actually ran and a pattern was persisted;
/// `false` when the observation count was below the threshold.
async fn maybe_trigger_compile(
    store: &MemoryStore,
    bridge: &dyn DspyBridge,
    namespace: &str,
    threshold: usize,
) -> anyhow::Result<bool> {
    let count = store.count_live_observations(namespace).await?;
    if count < threshold as u64 {
        return Ok(false);
    }
    let texts = store.list_live_observation_texts(namespace, threshold).await?;
    let instruction = bridge.compile(namespace, &texts).await?;
    persist_compiled_pattern(store, namespace, &instruction).await?;
    Ok(true)
}

/// Carrier for DSPy flywheel state threaded through the optimizer cycle.
pub struct DspyContext {
    pub store: Arc<MemoryStore>,
    pub bridge: Arc<dyn DspyBridge>,
    pub namespace: String,
}

/// Called after judge findings are emitted for an active child node.
/// Writes findings as Observations and triggers a DSPy compile when the
/// cohort threshold is reached. Skipped entirely when `dspy_enabled=false`.
///
/// Returns the pattern_id string (the ULID of the persisted pattern) if a
/// compile ran this call; returns `None` if dspy is disabled, the context is
/// absent, or the observation count is still below the threshold.
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
    let compiled = maybe_trigger_compile(
        &ctx.store,
        ctx.bridge.as_ref(),
        &ctx.namespace,
        config.dspy_pattern_cohort_threshold,
    )
    .await?;
    if compiled {
        // Return a synthetic pattern_id (ULID) so the caller can emit FlywheelCompiled.
        Ok(Some(ulid::Ulid::new().to_string()))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::*;
    use crate::autooptimizer::judge::FindingSeverity;

    struct RecordingBridge {
        called: Arc<Mutex<bool>>,
    }

    #[async_trait]
    impl DspyBridge for RecordingBridge {
        async fn compile(&self, _ns: &str, _texts: &[String]) -> anyhow::Result<String> {
            *self.called.lock().expect("mutex poisoned") = true;
            Ok("compiled instruction from recording bridge".to_string())
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

    #[tokio::test]
    async fn dspy_disabled_skips_compilation() {
        let store = Arc::new(MemoryStore::open_in_memory().await.unwrap());
        let called = Arc::new(Mutex::new(false));
        let bridge = Arc::new(RecordingBridge {
            called: Arc::clone(&called),
        });
        let ctx = DspyContext {
            store: Arc::clone(&store),
            bridge,
            namespace: "test:disabled".to_string(),
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
        let ctx = DspyContext {
            store: Arc::clone(&store),
            bridge,
            namespace: "test:enabled".to_string(),
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
        handle_cycle_dspy(&config, Some(&ctx), &last_finding, "cycle-2")
            .await
            .unwrap();
        assert!(*called.lock().unwrap(), "bridge must be called at threshold");

        // compiled instruction was persisted as a Pattern
        let prefix = query_dsr_prefix(&store, "test:enabled").await.unwrap();
        assert!(prefix.is_some(), "DSR prefix must be queryable after compilation");
        assert!(
            prefix.unwrap().contains("compiled instruction"),
            "prefix must contain the bridge-returned instruction"
        );
    }
}
