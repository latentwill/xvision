use xianvec_engine::templates::registry;

#[test]
fn unknown_template_returns_none() {
    assert!(registry::get("does_not_exist").is_none());
}

#[test]
fn list_template_names_returns_a_vec() {
    let _names: Vec<String> = registry::list_template_names();
    // empty until Task 9 — Task 9 adds the assertion that mean_reversion is present.
}
