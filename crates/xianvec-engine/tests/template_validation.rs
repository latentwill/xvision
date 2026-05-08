use xianvec_engine::bundle::validate::validate_bundle;
use xianvec_engine::templates::registry;

#[test]
fn unknown_template_returns_none() {
    assert!(registry::get("does_not_exist").is_none());
}

#[test]
fn registry_has_mean_reversion() {
    let names = registry::list_template_names();
    assert!(names.contains(&"mean_reversion".to_string()));
}

#[test]
fn mean_reversion_draft_validates() {
    let tpl = registry::get("mean_reversion").expect("template exists");
    let draft = tpl.new_draft("01H8N7ZTEST".into(), "test-eth-mr".into(), "@test".into());
    validate_bundle(&draft).expect("draft must validate");
    assert_eq!(draft.manifest.template, "mean_reversion");
    assert_eq!(draft.manifest.display_name, "test-eth-mr");
    assert!(draft.trader_slot.is_some());
}
