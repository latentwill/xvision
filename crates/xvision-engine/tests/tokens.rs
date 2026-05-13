use xvision_engine::templates::registry;
use xvision_engine::tokens::{estimate_pipeline_tokens, estimate_pipeline_tokens_from_slots};

#[test]
fn estimator_returns_positive_token_counts_for_real_bundle() {
    let tpl = registry::get("mean_reversion").unwrap();
    let b = tpl.new_draft("01H8N7ZTKN".into(), "tkn-test".into(), "@t".into());
    let est = estimate_pipeline_tokens(&b, /*decision_points=*/ 100);
    assert!(est.total > 0);
    assert!(est.input > 0);
    assert!(est.output > 0);
    // input dominates output for typical strategy runs (long prompts, short JSON outs).
    assert!(est.input > est.output);
}

#[test]
fn estimator_scales_with_decision_points() {
    let tpl = registry::get("mean_reversion").unwrap();
    let b = tpl.new_draft("01H8N7ZSCALE".into(), "scale-test".into(), "@t".into());
    let est_small = estimate_pipeline_tokens(&b, 10);
    let est_big = estimate_pipeline_tokens(&b, 1000);
    assert!(est_big.total > est_small.total * 50); // ~100x more decisions ≈ 100x more tokens
}

#[test]
fn slot_iterator_estimate_matches_legacy_slot_estimate() {
    let tpl = registry::get("mean_reversion").unwrap();
    let b = tpl.new_draft("01H8N7ZITER".into(), "iter-test".into(), "@t".into());
    let est_legacy = estimate_pipeline_tokens(&b, 25);
    let est_from_slots = estimate_pipeline_tokens_from_slots(
        [&b.regime_slot, &b.intern_slot, &b.trader_slot]
            .into_iter()
            .flatten(),
        25,
    );
    assert_eq!(est_from_slots, est_legacy);
}
