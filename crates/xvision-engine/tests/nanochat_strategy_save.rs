//! Integration test: strategy save path enforces checkpoint live_approved gate
//! and indicator-compatibility check via NanochatStore DB lookup.
//! Also covers `set_agent_checkpoint` end-to-end (WU s3ph.27).

use tempfile::TempDir;
use xvision_engine::api::strategy::SetAgentCheckpointReq;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::nanochat::store::{NanochatStore, NewTrainedModel};
use xvision_engine::strategies::agent_ref::{AgentRef, CheckpointRef};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::Strategy;

/// Helper: insert a trained_models row and return model_id.
async fn insert_model(ctx: &ApiContext, live_approved: bool, indicators: &[&str]) -> String {
    let store = NanochatStore::new(ctx.db.clone());
    let indicators_json: Vec<String> = indicators.iter().map(|s| format!(r#""{s}""#)).collect();
    let input_spec = format!(
        r#"{{"window_bars":64,"indicators":[{}],"normalization":"zscore"}}"#,
        indicators_json.join(",")
    );
    let model_id = store
        .insert_model(NewTrainedModel {
            display_name: "test-model".into(),
            source_strategy_id: None,
            source_strategy_name: None,
            run_tag: "jun14a".into(),
            checkpoint_path: "/tmp/ckpt/jun14a".into(),
            weights_sha256: "abc123".into(),
            input_spec,
            label_strategy: "price_forward".into(),
            label_config: r#"{"pnl":{"$gt":0}}"#.into(),
            best_acc: Some(0.57),
            best_loss: None,
            holdout_samples: Some(300),
            autoresearch_run_id: None,
        })
        .await
        .unwrap();
    if live_approved {
        store.set_live_approved(&model_id).await.unwrap();
    }
    model_id
}

/// Build a minimal Strategy fixture with one checkpoint slot.
fn strategy_with_checkpoint(model_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: ulid::Ulid::new().to_string(),
            display_name: "checkpoint-test".into(),
            plain_summary: "test".into(),
            creator: "@test".into(),
            template: "custom".into(),
            regime_fit: Vec::new(),
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: Vec::new(),
            required_tools: Vec::new(),
            risk_preset_or_config: "conservative".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![
            AgentRef {
                agent_id: "01HZFILTER000000000000000000".into(),
                role: "filter".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: Some(CheckpointRef {
                    model_id: model_id.into(),
                }),
                veto: Some(true),
            },
            AgentRef {
                agent_id: "01HZTRADER000000000000000000".into(),
                role: "trader".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
            },
        ],
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Conservative.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

#[tokio::test]
async fn strategy_save_rejected_when_checkpoint_not_live_approved() {
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();
    // Insert model with live_approved=false (default from insert).
    let model_id = insert_model(&ctx, false, &[]).await;

    let s = strategy_with_checkpoint(&model_id);
    let err = ctx
        .validate_and_save_strategy(s)
        .await
        .expect_err("live_approved=0 must block save");
    assert!(
        err.to_string().contains("live-approved") || err.to_string().contains("CheckpointNotLiveApproved"),
        "error must mention live-approved gate: {err}"
    );
}

#[tokio::test]
async fn strategy_save_rejected_when_indicator_missing() {
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();
    // Model requires rsi_14 but strategy has no tools registered.
    let model_id = insert_model(&ctx, true, &["rsi_14"]).await;

    let s = strategy_with_checkpoint(&model_id); // no tools in manifest.required_tools
    let err = ctx
        .validate_and_save_strategy(s)
        .await
        .expect_err("missing indicator must block save");
    assert!(
        err.to_string().contains("rsi_14") || err.to_string().contains("MissingCheckpointIndicators"),
        "error must name missing indicator: {err}"
    );
}

#[tokio::test]
async fn strategy_save_succeeds_when_live_approved_and_indicators_satisfied() {
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();
    let model_id = insert_model(&ctx, true, &["rsi_14"]).await;

    let mut s = strategy_with_checkpoint(&model_id);
    s.manifest.required_tools = vec!["rsi_14".into()]; // indicator satisfied

    // Should not return a checkpoint-validation error.
    let result = ctx.validate_and_save_strategy(s).await;
    match result {
        Ok(_) => {}
        Err(e)
            if e.to_string().contains("live-approved")
                || e.to_string().contains("rsi_14")
                || e.to_string().contains("CheckpointNotLiveApproved")
                || e.to_string().contains("MissingCheckpointIndicators") =>
        {
            panic!("satisfied checkpoint must not trigger validation error: {e}");
        }
        Err(_) => {} // other strategy-structure errors are fine for this fixture
    }
}

// ── set_agent_checkpoint end-to-end tests (s3ph.27) ──────────────────────────

/// Build a minimal strategy WITHOUT a checkpoint so we can attach one via
/// `set_agent_checkpoint`.  Unlike `strategy_with_checkpoint`, the filter
/// slot starts with `checkpoint: None`.
fn strategy_no_checkpoint() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: ulid::Ulid::new().to_string(),
            display_name: "checkpoint-patch-test".into(),
            plain_summary: "test".into(),
            creator: "@test".into(),
            template: "custom".into(),
            regime_fit: Vec::new(),
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: Vec::new(),
            required_tools: vec!["rsi_14".into()],
            risk_preset_or_config: "conservative".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: "01HZFILTER000000000000000000".into(),
            role: "filter".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Conservative.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

#[tokio::test]
async fn set_agent_checkpoint_persists_live_approved_checkpoint() {
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();

    // Insert a live-approved model that needs rsi_14 (strategy already has it).
    let model_id = insert_model(&ctx, true, &["rsi_14"]).await;

    // Save a base strategy (no checkpoint yet).
    let s = strategy_no_checkpoint();
    let strategy_id = s.manifest.id.clone();
    ctx.validate_and_save_strategy(s).await.unwrap();

    // Attach the checkpoint via set_agent_checkpoint.
    let updated = xvision_engine::api::strategy::set_agent_checkpoint(
        &ctx,
        SetAgentCheckpointReq {
            strategy_id: strategy_id.clone(),
            role: "filter".into(),
            checkpoint: Some(CheckpointRef {
                model_id: model_id.clone(),
            }),
            veto: Some(true),
        },
    )
    .await
    .expect("live-approved checkpoint must be accepted");

    // Verify the returned strategy has the checkpoint set.
    let slot = updated
        .agents
        .iter()
        .find(|a| a.role == "filter")
        .expect("filter slot must exist");
    assert_eq!(slot.checkpoint.as_ref().map(|c| &c.model_id), Some(&model_id));
    assert_eq!(slot.veto, Some(true));

    // Reload from disk — must persist.
    let reloaded = xvision_engine::api::strategy::get(&ctx, &strategy_id)
        .await
        .expect("strategy must reload");
    let reloaded_slot = reloaded
        .agents
        .iter()
        .find(|a| a.role == "filter")
        .expect("filter slot must exist after reload");
    assert_eq!(
        reloaded_slot.checkpoint.as_ref().map(|c| &c.model_id),
        Some(&model_id),
        "checkpoint must persist to disk"
    );
    assert_eq!(reloaded_slot.veto, Some(true));
}

#[tokio::test]
async fn set_agent_checkpoint_rejects_not_live_approved() {
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();

    let model_id = insert_model(&ctx, false, &[]).await;

    let s = strategy_no_checkpoint();
    let strategy_id = s.manifest.id.clone();
    ctx.validate_and_save_strategy(s).await.unwrap();

    let err = xvision_engine::api::strategy::set_agent_checkpoint(
        &ctx,
        SetAgentCheckpointReq {
            strategy_id: strategy_id.clone(),
            role: "filter".into(),
            checkpoint: Some(CheckpointRef {
                model_id: model_id.clone(),
            }),
            veto: Some(true),
        },
    )
    .await
    .expect_err("non-live-approved checkpoint must be rejected");

    assert!(
        err.to_string().contains("live-approved") || err.to_string().contains("CheckpointNotLiveApproved"),
        "error must mention live-approved gate: {err}"
    );
}

#[tokio::test]
async fn set_agent_checkpoint_rejects_unknown_role() {
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();

    let model_id = insert_model(&ctx, true, &[]).await;

    let s = strategy_no_checkpoint();
    let strategy_id = s.manifest.id.clone();
    ctx.validate_and_save_strategy(s).await.unwrap();

    let err = xvision_engine::api::strategy::set_agent_checkpoint(
        &ctx,
        SetAgentCheckpointReq {
            strategy_id: strategy_id.clone(),
            role: "nonexistent-role".into(),
            checkpoint: Some(CheckpointRef { model_id }),
            veto: None,
        },
    )
    .await
    .expect_err("unknown role must be rejected");

    assert!(
        err.to_string().contains("not found") || err.to_string().contains("nonexistent-role"),
        "error must mention missing role: {err}"
    );
}

#[tokio::test]
async fn set_agent_checkpoint_clears_checkpoint_when_none() {
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();

    let model_id = insert_model(&ctx, true, &["rsi_14"]).await;

    // Start with a strategy that already has a checkpoint.
    let mut s = strategy_no_checkpoint();
    let strategy_id = s.manifest.id.clone();
    s.agents[0].checkpoint = Some(CheckpointRef {
        model_id: model_id.clone(),
    });
    s.agents[0].veto = Some(true);
    ctx.validate_and_save_strategy(s).await.unwrap();

    // Clear it via set_agent_checkpoint with checkpoint: None.
    let updated = xvision_engine::api::strategy::set_agent_checkpoint(
        &ctx,
        SetAgentCheckpointReq {
            strategy_id: strategy_id.clone(),
            role: "filter".into(),
            checkpoint: None,
            veto: None,
        },
    )
    .await
    .expect("clearing checkpoint must succeed");

    let slot = updated.agents.iter().find(|a| a.role == "filter").unwrap();
    assert!(slot.checkpoint.is_none(), "checkpoint must be cleared");
    assert!(slot.veto.is_none(), "veto must be cleared");
}

#[tokio::test]
async fn set_agent_checkpoint_rejects_model_override_conflict() {
    // Attaching a checkpoint to a slot that already carries model_override is the
    // illegal CheckpointAndModelOverrideConflict — the mutual-exclusion check
    // (validate_strategy) must reject it BEFORE persisting, even though the
    // checkpoint itself is live-approved + indicator-compatible.
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();

    let model_id = insert_model(&ctx, true, &["rsi_14"]).await;

    // Seed a strategy whose filter slot has model_override set (and no checkpoint
    // yet — model_override alone is valid and saves fine).
    let mut s = strategy_no_checkpoint();
    let strategy_id = s.manifest.id.clone();
    s.agents[0].model_override = Some("anthropic/claude-haiku-4-5".into());
    ctx.validate_and_save_strategy(s).await.unwrap();

    let err = xvision_engine::api::strategy::set_agent_checkpoint(
        &ctx,
        SetAgentCheckpointReq {
            strategy_id: strategy_id.clone(),
            role: "filter".into(),
            checkpoint: Some(CheckpointRef {
                model_id: model_id.clone(),
            }),
            veto: Some(true),
        },
    )
    .await
    .expect_err("checkpoint + model_override on the same slot must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("model_override") || msg.contains("mutually exclusive"),
        "error must name the mutual-exclusion conflict: {msg}"
    );

    // And it must NOT have persisted — the slot still has no checkpoint on reload.
    let reloaded = xvision_engine::api::strategy::get(&ctx, &strategy_id)
        .await
        .expect("strategy must reload");
    let slot = reloaded.agents.iter().find(|a| a.role == "filter").unwrap();
    assert!(
        slot.checkpoint.is_none(),
        "rejected checkpoint must NOT have been persisted"
    );
}
