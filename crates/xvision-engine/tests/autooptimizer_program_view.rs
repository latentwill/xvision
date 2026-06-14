use xvision_engine::{
    autooptimizer::program_view::{from_markdown, round_trip_invariant_ok, to_markdown},
    strategies::{
        manifest::PublicManifest, risk::RiskPreset, ActivationMode, AgentRef, PipelineDef, Strategy,
    },
};

fn fixture_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01HZTEST0001".into(),
            display_name: "Test Strategy".into(),
            plain_summary: "A test strategy".into(),
            creator: "@test".into(),
            template: "custom".into(),
            regime_fit: vec![],
            asset_universe: vec![],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: "01HZAGENT01".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
        }],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

#[test]
fn to_markdown_produces_all_section_headers() {
    let md = to_markdown(&fixture_strategy());
    assert!(!md.is_empty());
    assert!(md.contains("# Strategy "), "missing H1: {md}");
    assert!(md.contains("## Manifest"), "missing Manifest: {md}");
    assert!(md.contains("## Agents"), "missing Agents: {md}");
    assert!(md.contains("## Risk config"), "missing Risk config: {md}");
}

#[test]
fn round_trip_preserves_fixture_strategy_bit_for_bit() {
    round_trip_invariant_ok(&fixture_strategy()).expect("round-trip must be lossless");
}

#[test]
fn from_markdown_rejects_missing_required_section() {
    let md = to_markdown(&fixture_strategy());
    let cut = md
        .find("\n## Risk config")
        .expect("Risk config must be present in to_markdown output");
    let stripped = &md[..cut];
    let result = from_markdown(stripped, &fixture_strategy());
    assert!(result.is_err(), "expected error for missing section");
    assert!(
        result.unwrap_err().to_string().contains("Risk config"),
        "error message must name the missing section",
    );
}

#[test]
fn editing_manifest_section_reflects_in_from_markdown() {
    let md = to_markdown(&fixture_strategy());
    let edited = md.replace(
        "\"asset_universe\": []",
        "\"asset_universe\": [\"ETH/USD\",\"BTC/USD\"]",
    );
    assert_ne!(md, edited, "replacement must have changed the document");
    let parsed = from_markdown(&edited, &fixture_strategy()).expect("edited markdown must parse");
    assert_eq!(
        parsed.manifest.asset_universe,
        vec!["ETH/USD".to_string(), "BTC/USD".to_string()],
    );
}
