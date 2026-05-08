use xianvec_engine::templates::registry;
use xianvec_engine::tokens::estimate_pipeline_tokens;

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
