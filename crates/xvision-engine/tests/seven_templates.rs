use xvision_engine::strategies::validate::validate_strategy;
use xvision_engine::templates::registry;

const EXPECTED_TEMPLATES: &[&str] = &[
    "trend_follower",
    "breakout",
    "mean_reversion",
    "momentum",
    "range_trade",
    "scalping",
    "news_trader",
    "custom",
    "ma_crossover_baseline",
];

#[test]
fn all_v1_templates_are_registered() {
    let names = registry::list_template_names();
    for name in EXPECTED_TEMPLATES {
        assert!(
            names.contains(&name.to_string()),
            "missing template: {name} (registered: {names:?})"
        );
    }
}

#[test]
fn each_template_produces_a_valid_draft() {
    for name in EXPECTED_TEMPLATES {
        let tpl = registry::get(name).unwrap_or_else(|| panic!("template {name} missing from registry"));
        // ULID-shaped 26-char id (validate_strategy doesn't enforce format,
        // just shape; this matches the assertion in the plan).
        let id: String = format!("01H8N7Z{name}").chars().take(26).collect();
        let draft = tpl.new_draft(id, format!("test-{name}"), "@test".into());
        validate_strategy(&draft).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn templates_have_unique_names() {
    let names = registry::list_template_names();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(names.len(), sorted.len(), "duplicate template names: {names:?}");
}
