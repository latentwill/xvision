//! Deterministic foundation tests for `xvision-dspy`.
//!
//! Everything here is **no-network**: the only model used is the `DummyLM`-backed
//! [`DeterministicTestModel`]. There is no live provider call anywhere.

use std::collections::BTreeMap;

use dspy_rs::{Chat, Example, Message};

use xvision_dspy::adapter::{DeterministicTestModel, OptimizerModel};
use xvision_dspy::capability::Capability;
use xvision_dspy::error::OptimizerError;
use xvision_dspy::signatures::{
    self, validate_confidence, validate_size_fraction, TraderAction,
};
use xvision_dspy::snapshot::{signature_hash, OptimizationSnapshot, SnapshotDemo};

/// Build a minimal DSRs Example for seeding DummyLM calls.
fn seed_example() -> Example {
    let mut data = std::collections::HashMap::new();
    data.insert(
        "briefing".to_string(),
        serde_json::Value::String("SPY up 0.3%, low vol regime".to_string()),
    );
    Example::new(data, vec!["briefing".to_string()], vec![])
}

#[tokio::test]
async fn deterministic_model_smoke_no_network() {
    // The DummyLM-backed model returns its scripted output verbatim, every time.
    let scripted = "action: buy\nsize_fraction: 0.25\nrationale: trend intact";
    let model = DeterministicTestModel::new(scripted).await;

    let chat = Chat::new(vec![Message::user("Decide the trade.")]);

    let first = model
        .complete(seed_example(), chat.clone())
        .await
        .expect("deterministic completion must succeed");
    let second = model
        .complete(seed_example(), chat)
        .await
        .expect("deterministic completion must succeed");

    // Determinism: identical input → identical output.
    assert_eq!(first.text, scripted);
    assert_eq!(second.text, scripted);

    // Provenance identity is the dummy provider; cost is free (0).
    assert_eq!(first.provenance.provider, "dummy");
    assert_eq!(first.provenance.model, "dummy");
    assert_eq!(first.provenance.cost_micros_usd, 0);

    // Provenance accumulates across calls (token totals are monotonic).
    let prov = model.provenance();
    assert!(prov.total_tokens() >= first.provenance.total_tokens());
}

#[tokio::test]
async fn signature_compile_and_optimize_boundary_smoke() {
    // "compile/optimize smoke": construct the trader signature, run a scripted
    // model completion against it, and parse/validate the output back through the
    // signature boundary — no optimizer search, no network.
    let sig = signatures::signature_for(Capability::Trader)
        .expect("trader signature must exist");

    assert!(!sig.instruction().is_empty());
    assert!(sig.input_fields().get("briefing").is_some());
    assert!(sig.output_fields().get("action").is_some());

    // A scripted model "decision" that the validate boundary accepts.
    let model = DeterministicTestModel::new("buy").await;
    let completion = model
        .complete(
            seed_example(),
            Chat::new(vec![Message::user(sig.instruction())]),
        )
        .await
        .expect("completion must succeed");

    let action = TraderAction::parse(&completion.text).expect("buy must parse");
    assert_eq!(action, TraderAction::Buy);

    // Range validators.
    assert!(validate_size_fraction(0.25).is_ok());
    assert!(validate_size_fraction(1.5).is_err());
    assert!(validate_confidence(0.9, "filter_signal").is_ok());
    assert!(validate_confidence(-0.1, "filter_signal").is_err());

    // Bad action string is a typed validate error, not a panic.
    let err = TraderAction::parse("yolo").unwrap_err();
    assert!(matches!(err, OptimizerError::Signature { phase: "validate", .. }));
}

#[test]
fn signature_hash_is_deterministic_and_distinguishing() {
    let trader = signatures::signature_for(Capability::Trader).unwrap();
    let filter = signatures::signature_for(Capability::Filter).unwrap();

    let trader_hash_a = signature_hash(trader.as_ref());
    // Re-derive a fresh trader signature: same shape ⇒ same hash.
    let trader_again = signatures::signature_for(Capability::Trader).unwrap();
    let trader_hash_b = signature_hash(trader_again.as_ref());

    assert_eq!(trader_hash_a, trader_hash_b, "stable across instances");
    assert_eq!(trader_hash_a.len(), 64, "sha-256 hex is 64 chars");

    let filter_hash = signature_hash(filter.as_ref());
    assert_ne!(
        trader_hash_a, filter_hash,
        "different signatures must hash differently"
    );
}

#[test]
fn missing_capability_returns_typed_error_with_remediation() {
    for cap in [
        Capability::DecisionGrader,
        Capability::Intern,
        Capability::ChatAuthoring,
    ] {
        assert!(!cap.has_optimizer());
        // `BoxedSignature` (the Ok type) is not Debug, so match instead of
        // `expect_err`.
        let err = match signatures::signature_for(cap) {
            Ok(_) => panic!("expected unsupported capability {}", cap.as_key()),
            Err(e) => e,
        };
        match err {
            OptimizerError::MissingCapabilityOptimizer {
                capability,
                remediation,
            } => {
                assert_eq!(capability, cap.as_key());
                assert!(
                    !remediation.is_empty(),
                    "remediation text must be present for {capability}"
                );
                assert!(remediation.contains("stub"));
            }
            other => panic!("expected MissingCapabilityOptimizer, got {other:?}"),
        }
    }

    // Implemented capabilities do NOT error.
    assert!(Capability::Trader.has_optimizer());
    assert!(Capability::Filter.has_optimizer());
    assert!(signatures::signature_for(Capability::Trader).is_ok());
    assert!(signatures::signature_for(Capability::Filter).is_ok());
}

#[test]
fn snapshot_round_trips_through_json() {
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "briefing".to_string(),
        serde_json::Value::String("regime: trending".to_string()),
    );
    let mut outputs = BTreeMap::new();
    outputs.insert(
        "action".to_string(),
        serde_json::Value::String("buy".to_string()),
    );

    let trader = signatures::signature_for(Capability::Trader).unwrap();

    let snap = OptimizationSnapshot {
        id: "snap-child-01".to_string(),
        instruction: "Be a disciplined trader.".to_string(),
        demos: vec![SnapshotDemo { inputs, outputs }],
        signature_hash: signature_hash(trader.as_ref()),
        metric_name: "delta_sharpe".to_string(),
        corpus_query: "scenario:spy-2024-q1".to_string(),
        rng_seed: 42,
        optimizer_name: "copro".to_string(),
        optimizer_version: "dspy-rs=0.7.3".to_string(),
        parent_id: Some("snap-root-00".to_string()),
        child_ids: vec![],
    };

    let json = snap.to_json().expect("serialize");
    let back = OptimizationSnapshot::from_json(&json).expect("deserialize");
    assert_eq!(snap, back, "snapshot must round-trip losslessly");

    // Serialization is deterministic (same value ⇒ same bytes).
    let json2 = back.to_json().expect("serialize again");
    assert_eq!(json, json2);

    // Lineage is captured.
    assert_eq!(back.parent_id.as_deref(), Some("snap-root-00"));
    assert_eq!(back.rng_seed, 42);
}
