use xvision_engine::baselines::ma_crossover::ma_crossover_template;
use xvision_engine::bundle::validate::validate_bundle;

#[test]
fn ma_crossover_produces_valid_bundle() {
    let tpl = ma_crossover_template();
    let draft = tpl.new_draft(
        "01H8N7ZBASE".into(),
        "btc-ma-cross".into(),
        "@xvision_official".into(),
    );
    validate_bundle(&draft).expect("baseline must validate");
    assert!(draft.trader_slot.is_some());
    let trader = draft.trader_slot.unwrap();
    assert!(trader.prompt.to_lowercase().contains("crossover"));
}
