//! Integration tests for the wizard-side blank-draft path and the
//! post-template-registry-removal `create_strategy` shape.

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use xvision_engine::{
    api::{strategy as api_strategy, Actor, ApiContext},
    authoring,
    strategies::store::{strategy_store_dir, FilesystemStore},
};

async fn fresh_api_context() -> (ApiContext, TempDir) {
    let td = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "authoring-test".into(),
        },
        td.path().to_path_buf(),
    );
    (ctx, td)
}

#[tokio::test]
async fn create_blank_strategy_produces_no_agents_and_no_placeholder_slot() {
    // agents = vec![], trader_slot = None, template = "custom"
    // (free-text label, no longer a registry key).
    let (ctx, _td) = fresh_api_context().await;
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let out = authoring::create_blank_strategy(&store, "Blank Draft".into(), Some("@op".into()))
        .await
        .expect("blank strategy must save");

    let draft = authoring::get_strategy(&store, &out.id)
        .await
        .expect("draft must load");
    assert!(draft.agents.is_empty(), "no AgentRefs on blank draft");
    assert!(
        draft.trader_slot.is_none(),
        "no placeholder trader slot on blank draft"
    );
    assert!(draft.regime_slot.is_none());
    assert_eq!(draft.manifest.template, "custom");
    assert_eq!(draft.manifest.display_name, "Blank Draft");
    assert_eq!(draft.manifest.creator, "@op");
}

#[tokio::test]
async fn create_blank_strategy_defaults_creator_to_anonymous() {
    let (ctx, _td) = fresh_api_context().await;
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let out = authoring::create_blank_strategy(&store, "x".into(), None)
        .await
        .unwrap();
    let draft = authoring::get_strategy(&store, &out.id).await.unwrap();
    assert_eq!(draft.manifest.creator, "@anonymous");
}

#[tokio::test]
async fn create_strategy_produces_a_blank_draft_post_registry_removal() {
    // Post-2026-05-21 the strategy template_registry was removed.
    // `authoring::create_strategy` no longer scaffolds from a named
    // template; it produces a blank draft identical to the wizard
    // path. The non-wizard callers (MCP, CLI, dashboard route) get
    // the same blank shape and fill in agents / slots / mechanical
    // params via follow-up calls.
    let (ctx, _td) = fresh_api_context().await;
    let out = api_strategy::create_strategy(
        &ctx,
        authoring::CreateStrategyReq {
            name: "TF1".into(),
            creator: Some("@op".into()),
        },
    )
    .await
    .expect("create_strategy must produce a blank draft");

    let strategy = api_strategy::get(&ctx, &out.id).await.expect("get");
    assert_eq!(strategy.manifest.template, "custom");
    assert!(
        strategy.trader_slot.is_none(),
        "blank draft must not carry a placeholder trader slot",
    );
    assert!(strategy.regime_slot.is_none());
    assert!(strategy.agents.is_empty());
    assert_eq!(strategy.manifest.creator, "@op");
}

#[tokio::test]
async fn legacy_create_strategy_request_with_template_field_is_rejected_at_serde() {
    // The `template` field was removed from `CreateStrategyReq`.
    // Callers that haven't migrated their JSON payloads see a
    // structured serde error on deserialize. This pins the upgrade
    // contract for downstream callers (MCP / CLI / dashboard).
    let raw = r#"{"template":"trend_follower","name":"x","creator":null}"#;
    let err = serde_json::from_str::<authoring::CreateStrategyReq>(raw)
        .expect_err("legacy template field must be rejected post-registry-removal");
    let msg = err.to_string();
    assert!(msg.contains("unknown field"), "got: {msg}");
    assert!(msg.contains("template"), "got: {msg}");
}
