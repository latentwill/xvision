//! P3 — mutator cross-run/cross-framework memory.
//!
//! The mutator records each gated candidate's outcome as an Observation in
//! `autooptimizer:mutations` and recalls prior outcomes (Patterns) to inform
//! its prompt — alongside (NOT replacing) the F32 hard avoid-set + seed-directed
//! exploration. These tests cover the compact outcome descriptor, the advisory
//! prior-outcomes prompt section, and the namespace recall wiring.

use std::sync::Arc;

use xvision_engine::agent::memory_recorder::MemoryRecorder;
use xvision_engine::agent::memory_recorder::RecallResult;
use xvision_engine::autooptimizer::mutator::{
    describe_mutation_outcome, MutationDiff, MutationGateContext, MutationKind, ParamChange, ProseEdit,
    ToolDiff, MUTATIONS_NS,
};

use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, Tier};

fn param_diff() -> MutationDiff {
    MutationDiff {
        kind: MutationKind::Param,
        prose: Vec::new(),
        params: vec![ParamChange {
            key: "risk.stop_loss_atr_multiple".into(),
            before: serde_json::json!(2.0),
            after: serde_json::json!(3.5),
        }],
        tools: ToolDiff {
            added: Vec::new(),
            removed: Vec::new(),
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: "widen stop".into(),
    }
}

#[test]
fn describe_mutation_outcome_param_change_is_compact() {
    let desc = describe_mutation_outcome(&param_diff(), -0.40, "rejected", None);
    // Compact one-liner including the key, the from→to, the ΔSharpe, status.
    assert!(
        desc.contains("risk.stop_loss_atr_multiple"),
        "should name the param key: {desc}"
    );
    assert!(desc.contains("3.5"), "should include the new value: {desc}");
    assert!(
        desc.contains("ΔSharpe"),
        "should include the ΔSharpe marker: {desc}"
    );
    assert!(
        desc.contains("rejected"),
        "should include the status label: {desc}"
    );
    assert_eq!(desc.lines().count(), 1, "must be a single line: {desc}");
}

#[test]
fn describe_mutation_outcome_prose_and_tools() {
    let prose = MutationDiff {
        kind: MutationKind::Prose,
        prose: vec![ProseEdit {
            agent_role: "trader".into(),
            before: "a".into(),
            after: "b".into(),
        }],
        params: Vec::new(),
        tools: ToolDiff {
            added: Vec::new(),
            removed: Vec::new(),
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: "x".into(),
    };
    let d = describe_mutation_outcome(&prose, 0.12, "active", None);
    assert!(d.contains("ΔSharpe"), "prose summary has ΔSharpe: {d}");
    assert!(d.contains("active"), "prose summary has status: {d}");
    assert_eq!(d.lines().count(), 1);

    let tools = MutationDiff {
        kind: MutationKind::Tool,
        prose: Vec::new(),
        params: Vec::new(),
        tools: ToolDiff {
            added: vec!["macd".into()],
            removed: vec!["rsi".into()],
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: "x".into(),
    };
    let t = describe_mutation_outcome(&tools, -0.05, "rejected", None);
    assert!(t.contains("ΔSharpe"), "tool summary has ΔSharpe: {t}");
    assert_eq!(t.lines().count(), 1);
}

#[test]
fn describe_mutation_outcome_preserves_gate_failure_reason() {
    let desc = describe_mutation_outcome(
        &param_diff(),
        2.83,
        "rejected",
        Some(MutationGateContext {
            objective_label: "sharpe",
            delta_day: Some(2.83),
            delta_holdout: Some(-1.12),
            drawdown_ratio: Some(1.82),
            parent_n_trades: Some(26),
            child_n_trades: Some(18),
            min_trade_retention_ratio: Some(0.5),
            parent_realized_return_ratio: None,
            child_realized_return_ratio: Some(0.2),
            min_realized_return_ratio: Some(0.25),
            reason: Some("baseline-untouched-score worsened\nand max drawdown deteriorated"),
        }),
    );

    assert!(desc.contains("Δholdout -1.1200"), "{desc}");
    assert!(desc.contains("drawdown 1.82×"), "{desc}");
    assert!(
        desc.contains("reason: baseline-untouched-score worsened and max drawdown deteriorated"),
        "{desc}"
    );
    assert_eq!(desc.lines().count(), 1, "must be a single line: {desc}");
}

#[tokio::test]
async fn recall_in_mutations_namespace_surfaces_seeded_pattern() {
    let store = Arc::new(
        MemoryStore::open_in_memory()
            .await
            .expect("in-memory store opens"),
    );
    let pat = MemoryItem {
        id: ulid::Ulid::new().to_string(),
        namespace: MUTATIONS_NS.to_string(),
        tier: Tier::Pattern,
        text: "param risk.max_leverage 1.0→3.0 ⇒ ΔSharpe -0.40 (rejected)".to_string(),
        embedding: vec![0.1, 0.2, 0.3],
        created_at: chrono::Utc::now(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        source_window_start: None,
        source_window_end: None,
        training_window_end: None,
        promotion_state: Some("active".to_string()),
        attestation_id: None,
        forgotten_at: None,
    };
    store
        .upsert_pattern(&pat, "static-test")
        .await
        .expect("seed pattern");

    let rec = MemoryRecorder::with_static_embedder(store, "static-test", vec![0.1, 0.2, 0.3]);
    let res = rec
        .recall_in_namespace(MUTATIONS_NS, "q", 5, None)
        .await
        .expect("recall ok");
    match res {
        RecallResult::Hits {
            namespace, matches, ..
        } => {
            assert_eq!(namespace, MUTATIONS_NS);
            assert_eq!(matches.len(), 1, "expected the seeded pattern");
            assert!(matches[0].text.contains("max_leverage"));
        }
        other => panic!("expected Hits, got {other:?}"),
    }
}
