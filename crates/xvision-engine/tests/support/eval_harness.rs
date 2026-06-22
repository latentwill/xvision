use std::sync::Arc;

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::{RiskConfig, RiskPreset};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;

pub async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        include_str!("../../migrations/001_api_audit.sql"),
        include_str!("../../migrations/002_eval.sql"),
        include_str!("../../migrations/013_cli_jobs.sql"),
        include_str!("../../migrations/014_eval_agent_id.sql"),
        include_str!("../../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../../migrations/027_run_bars_manifest.sql"),
        include_str!("../../migrations/032_filters_and_evaluations.sql"),
        include_str!("../../migrations/016_eval_reviews.sql"),
        include_str!("../../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../../migrations/038_eval_runs_live_config.sql"),
        include_str!("../../migrations/015_eval_decisions_reasoning.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    sqlx::query(include_str!("../../migrations/018_agent_run_observability.sql"))
        .execute(&pool)
        .await
        .unwrap();
    RunStore::new(pool)
}

pub fn minimal_strategy(agent_id: &str) -> Strategy {
    strategy_with_manifest(
        agent_id,
        "guardrails test strategy",
        "F-7 guardrail coverage",
        &["BTC/USD"],
        RiskPreset::Balanced.expand(),
        1_440,
    )
}

pub fn strategy_with(agent_id: &str, assets: &[&str], preset: RiskPreset, cadence_minutes: u32) -> Strategy {
    strategy_with_risk(agent_id, assets, preset.expand(), cadence_minutes)
}

pub fn strategy_with_risk(
    agent_id: &str,
    assets: &[&str],
    risk: RiskConfig,
    cadence_minutes: u32,
) -> Strategy {
    strategy_with_manifest(
        agent_id,
        "exit-enforcement test strategy",
        "U2 protective-exit coverage",
        assets,
        risk,
        cadence_minutes,
    )
}

fn strategy_with_manifest(
    agent_id: &str,
    display_name: &str,
    plain_summary: &str,
    assets: &[&str],
    risk: RiskConfig,
    cadence_minutes: u32,
) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: display_name.into(),
            plain_summary: plain_summary.into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: assets.iter().map(|s| (*s).into()).collect(),
            decision_cadence_minutes: cadence_minutes,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk,
        hypothesis: None,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

pub fn trader_resp(action: &str) -> LlmResponse {
    let body = format!(r#"{{"action":"{action}","conviction":0.7,"justification":"test {action}"}}"#);
    LlmResponse {
        content: vec![ContentBlock::Text { text: body }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

pub fn sequenced_dispatch(actions: &[&str]) -> Arc<dyn LlmDispatch> {
    let resps: Vec<LlmResponse> = actions.iter().map(|a| trader_resp(a)).collect();
    Arc::new(MockDispatch::sequence(resps))
}

pub async fn count_notes_with_prefix(store: &RunStore, run_id: &str, prefix: &str) -> i64 {
    let pool = store_pool(store);
    let pattern = format!("{prefix}%");
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM supervisor_notes WHERE run_id = ? AND content LIKE ?")
        .bind(run_id)
        .bind(pattern)
        .fetch_one(&pool)
        .await
        .unwrap()
}

pub async fn fetch_note_contents(store: &RunStore, run_id: &str) -> Vec<(String, String, String)> {
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT role, severity, content FROM supervisor_notes WHERE run_id = ? ORDER BY created_at ASC",
    )
    .bind(run_id)
    .fetch_all(&store_pool(store))
    .await
    .unwrap()
}

fn store_pool(store: &RunStore) -> SqlitePool {
    store.pool_for_test()
}
