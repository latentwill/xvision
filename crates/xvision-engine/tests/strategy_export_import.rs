//! Engine-side tests for the self-contained marketplace strategy export +
//! import flow.
//!
//! Track: `mp-purchase-flow` — a published marketplace strategy must carry
//! the *full* `Agent` definitions it references, not just the
//! `AgentRef { agent_id, role }` POINTERS. Today the published bundle is the
//! bare `Strategy` JSON, so a buyer's `import_strategy` saves a strategy whose
//! referenced agents do not exist in the buyer's DB — the strategy can't run.
//!
//! These tests exercise `api::strategy::export_strategy` (publish side, bundles
//! the referenced agents) and `api::strategy::import_strategy` (import side,
//! materializes the bundled agents with fresh ULIDs and remaps the AgentRefs)
//! directly, independent of the dashboard / chain plumbing.

mod common;

use common::open_api_context;
use xvision_engine::agents::{AgentSlot, InputsPolicy};
use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
use xvision_engine::api::strategy::{self as api_strategy, StrategyExport};
use xvision_engine::api::ApiContext;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};

/// Create a real library Agent with a single slot and return its id.
async fn seed_agent(ctx: &ApiContext, name: &str) -> String {
    let agent = agents_api::create(
        ctx,
        CreateAgentRequest {
            name: name.into(),
            description: "seed agent for export/import tests".into(),
            tags: vec!["export-test".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "openrouter".into(),
                model: "deepseek/deepseek-chat".into(),
                system_prompt: "You are a disciplined crypto trader. Use the supplied OHLCV \
                                history, indicator panel, and scenario metadata to choose an \
                                action with explicit position sizing and invalidation. Avoid \
                                placeholders; ground every claim in active data."
                    .into(),
                skill_ids: vec![],
                max_tokens: Some(1024),
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: Default::default(),
                noop_skip: None,
                allowed_tools: Vec::new(),
                delta_briefing: None,
            }],
            scope_strategy_id: None,
        },
    )
    .await
    .expect("create seed agent");
    agent.agent_id
}

fn seed_strategy(id: &str, agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.into(),
            display_name: "seed-export-source".into(),
            plain_summary: "Source strategy for export/import tests.".into(),
            creator: "@export-test".into(),
            template: "custom".into(),
            regime_fit: vec![],
            asset_universe: vec!["ETH/USD".into()],
            decision_cadence_minutes: 240,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: Some(chrono::Utc::now()),
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: agent_id.into(),
            role: "trader".into(),
            activates: None,
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
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

async fn persist_strategy(ctx: &ApiContext, strategy: &Strategy) {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    store.save(strategy).await.expect("persist seed strategy");
}

/// `export_strategy` must bundle the FULL `Agent` definition for every
/// `AgentRef` the strategy carries, not just the pointer.
#[tokio::test]
async fn export_strategy_bundles_referenced_agents() {
    let (ctx, _d) = open_api_context().await;
    let agent_id = seed_agent(&ctx, "exporter-trader").await;
    let source_id = "01HZSTRATEGYEXPORTBUNDLE01";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let export: StrategyExport = api_strategy::export_strategy(&ctx, source_id)
        .await
        .expect("export the strategy");

    assert_eq!(export.strategy.manifest.id, source_id);
    assert_eq!(export.agents.len(), 1, "must bundle the one referenced agent");
    let bundled = &export.agents[0];
    assert_eq!(bundled.agent_id, agent_id);
    assert_eq!(bundled.slots.len(), 1);
    assert_eq!(bundled.slots[0].model, "deepseek/deepseek-chat");
    assert_eq!(bundled.slots[0].provider, "openrouter");
}

/// Importing a `StrategyExport` envelope must MATERIALIZE the bundled agents
/// with FRESH ULIDs and remap the strategy's AgentRefs to the new ids — the
/// buyer's DB has the referenced agents and the strategy is runnable.
#[tokio::test]
async fn import_envelope_materializes_agents_and_remaps_refs() {
    let (ctx, _d) = open_api_context().await;

    // Build an envelope by hand referencing a synthetic source agent id "A".
    // (Simulates a bundle that arrived from a seller — the agent does NOT
    // exist in this buyer's DB yet.)
    let source_agent_id = "01HZSOURCEAGENTAAAAAAAAAA1";
    let source_strategy_id = "01HZSOURCESTRATEGYAAAAAAA1";
    let strategy = seed_strategy(source_strategy_id, source_agent_id);
    // The bundled Agent carries the source id "A" in its own agent_id field.
    let bundled_agent = xvision_engine::agents::Agent {
        agent_id: source_agent_id.into(),
        name: "bundled-trader".into(),
        description: "bundled from a marketplace seller".into(),
        tags: vec!["bundled".into()],
        slots: vec![AgentSlot {
            name: "main".into(),
            provider: "openrouter".into(),
            model: "deepseek/deepseek-chat".into(),
            system_prompt: "You are a disciplined crypto trader. Ground every claim in \
                            active data; no placeholders. Provide explicit sizing and \
                            invalidation for every action."
                .into(),
            skill_ids: vec![],
            max_tokens: Some(1024),
            max_wall_ms: None,
            temperature: None,
            prompt_version: String::new(),
            inputs_policy: InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: Default::default(),
            noop_skip: None,
            allowed_tools: Vec::new(),
            delta_briefing: None,
        }],
        archived: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        scope_strategy_id: None,
    };
    let envelope = StrategyExport {
        strategy,
        agents: vec![bundled_agent],
    };
    let manifest = serde_json::to_value(&envelope).expect("serialize envelope");

    let imported = api_strategy::import_strategy(&ctx, manifest)
        .await
        .expect("import the envelope");

    // 1. The saved strategy minted a NEW strategy ULID.
    assert_ne!(
        imported.manifest.id, source_strategy_id,
        "must mint a NEW strategy ULID"
    );
    assert!(
        imported.manifest.published_at.is_none(),
        "published_at cleared on import"
    );

    // 2. The strategy's AgentRef was REMAPPED to a fresh agent id (not "A").
    assert_eq!(imported.agents.len(), 1);
    let new_agent_id = &imported.agents[0].agent_id;
    assert_ne!(
        new_agent_id, source_agent_id,
        "AgentRef must be remapped to a freshly-minted local agent id"
    );
    assert_eq!(imported.agents[0].role, "trader", "role preserved");

    // 3. A NEW agent exists in the buyer's AgentStore under the remapped id and
    //    is loadable, with the bundled slot definition.
    let materialized = agents_api::get(&ctx, new_agent_id)
        .await
        .expect("remapped agent is loadable");
    assert_eq!(materialized.slots.len(), 1);
    assert_eq!(materialized.slots[0].model, "deepseek/deepseek-chat");
    assert_eq!(materialized.slots[0].provider, "openrouter");

    // 4. The source agent id "A" does NOT leak into the buyer's store.
    assert!(
        agents_api::get(&ctx, source_agent_id).await.is_err(),
        "the seller's source agent id must NOT be reused locally"
    );
}

/// Backward compat: importing a BARE `Strategy` (legacy / open-tier xvn:// with
/// no envelope, no bundled agents) must still save the strategy with a fresh
/// ULID and materialize zero agents — the pre-envelope behavior.
#[tokio::test]
async fn import_bare_strategy_is_backward_compatible() {
    let (ctx, _d) = open_api_context().await;
    let source_strategy_id = "01HZBARESTRATEGYLEGACYAAA1";
    let strategy = seed_strategy(source_strategy_id, "01HZSOMEPOINTERAGENTIDAAA1");
    let manifest = serde_json::to_value(&strategy).expect("serialize bare strategy");

    let imported = api_strategy::import_strategy(&ctx, manifest)
        .await
        .expect("import a bare strategy");

    assert_ne!(
        imported.manifest.id, source_strategy_id,
        "must mint a NEW strategy ULID"
    );
    assert!(imported.manifest.published_at.is_none());

    // The strategy round-trips through the same store.
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let reread = store
        .load(&imported.manifest.id)
        .await
        .expect("load saved strategy");
    assert_eq!(reread.manifest.display_name, "seed-export-source");

    // No envelope agents → the AgentRef pointer is left untouched (it is the
    // seller's pointer, but there is nothing to materialize from a bare
    // Strategy). The legacy behavior saved the Strategy verbatim except id +
    // published_at, so the pointer survives unchanged.
    assert_eq!(imported.agents.len(), 1);
    assert_eq!(imported.agents[0].agent_id, "01HZSOMEPOINTERAGENTIDAAA1");
}

/// Round-trip: `export_strategy` → `import_strategy` yields a runnable strategy
/// whose materialized agent slots match the source agent's slots.
#[tokio::test]
async fn export_then_import_round_trips_to_runnable_strategy() {
    let (ctx, _d) = open_api_context().await;
    let agent_id = seed_agent(&ctx, "roundtrip-trader").await;
    let source_id = "01HZSTRATEGYROUNDTRIP00001";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    // Publish side: bundle.
    let export = api_strategy::export_strategy(&ctx, source_id)
        .await
        .expect("export");
    let source_slot = export.agents[0].slots[0].clone();
    let manifest = serde_json::to_value(&export).expect("serialize export");

    // Import side: materialize.
    let imported = api_strategy::import_strategy(&ctx, manifest)
        .await
        .expect("import");

    // The imported strategy references a freshly-minted local agent whose slots
    // match the exported source agent's slots.
    let new_agent_id = &imported.agents[0].agent_id;
    assert_ne!(new_agent_id, &agent_id, "agent id remapped");
    let materialized = agents_api::get(&ctx, new_agent_id).await.expect("loadable");
    assert_eq!(materialized.slots.len(), 1);
    assert_eq!(materialized.slots[0].model, source_slot.model);
    assert_eq!(materialized.slots[0].provider, source_slot.provider);
    assert_eq!(materialized.slots[0].system_prompt, source_slot.system_prompt);
}
