//! Integration tests for `xvn eval validate`.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use ulid::Ulid;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, PipelineKind, Strategy};

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn seed_strategy_with_missing_agent(home: &Path) -> String {
    let strategy_id = Ulid::new().to_string();
    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.clone(),
            display_name: "eval validate dangling agent".into(),
            plain_summary: "strategy with dangling trader AgentRef".into(),
            creator: "@eval-validate-test".into(),
            template: "custom".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
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
            agent_id: "01EVALMISSINGAGENT00000000".into(),
            role: "trader".into(),
            activates: Some(xvision_engine::agents::Capability::Trader),
            prompt_override: None,
            model_override: None,
        }],
        pipeline: PipelineDef {
            kind: PipelineKind::Single,
            edges: vec![],
        },
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
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let _ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "eval-validate-test".into(),
            },
        )
        .await
        .unwrap();
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.save(&strategy).await.unwrap();
    });
    strategy_id
}

#[test]
fn eval_validate_json_rejects_dangling_agent_ref() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_strategy_with_missing_agent(dir.path());

    let out = xvn(
        &[
            "eval",
            "validate",
            "--strategy",
            &strategy_id,
            "--scenario",
            "crypto-bull-q1-2025",
            "--json",
        ],
        dir.path(),
    );

    assert_ne!(
        out.status.code(),
        Some(0),
        "eval validate must fail before launch when strategy has dangling AgentRef; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout must be JSON on --json failure; stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        )
    });
    assert_eq!(body["ok"], false, "body: {body}");
    let errors = body["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| {
            let s = e.as_str().unwrap_or("");
            s.contains("01EVALMISSINGAGENT00000000") || s.contains("not launchable")
        }),
        "errors should mention dangling agent or launchability: {errors:?}",
    );
}
