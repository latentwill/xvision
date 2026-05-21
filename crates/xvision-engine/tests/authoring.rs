//! Integration tests for the wizard-side blank-draft path added by the
//! `templates-elimination` contract (2026-05-21) and a regression test
//! for the existing template-named `api_strategy::create_strategy` path
//! the wizard no longer consumes but other callers (MCP, CLI, dashboard
//! routes) still do.

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
    // Wizard-side blank-draft path: agents = vec![], trader_slot = None,
    // template = "custom", mechanical_params = {}.
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
    assert!(draft.intern_slot.is_none());
    assert_eq!(draft.manifest.template, "custom");
    assert_eq!(draft.manifest.display_name, "Blank Draft");
    assert_eq!(draft.manifest.creator, "@op");
    assert!(draft.mechanical_params.as_object().is_some_and(|m| m.is_empty()));
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
async fn create_strategy_with_named_template_still_seeds_a_real_strategy() {
    // Save-gate regression: the wizard no longer dispatches on template,
    // but direct callers (MCP, CLI, dashboard `/api/strategies` route)
    // still construct via the named-template path. The existing
    // template-backed `authoring::create_strategy` must still produce a
    // ready-to-save strategy with expected trend_follower content, not an
    // arbitrary non-empty placeholder.
    let (ctx, _td) = fresh_api_context().await;
    let out = api_strategy::create_strategy(
        &ctx,
        authoring::CreateStrategyReq {
            template: "trend_follower".into(),
            name: "TF1".into(),
            creator: Some("@op".into()),
        },
    )
    .await
    .expect("named-template path must still seed a real strategy");

    let strategy = api_strategy::get(&ctx, &out.id).await.expect("get");
    assert_eq!(strategy.manifest.template, "trend_follower");
    let trader = strategy
        .trader_slot
        .as_ref()
        .expect("template seeds a trader slot");
    assert!(
        trader.prompt.contains("EMA(12) > EMA(26) > EMA(50)"),
        "template-seeded trader prompt must include trend_follower EMA logic; got: {}",
        trader.prompt
    );
    assert_eq!(
        trader.allowed_tools,
        vec!["ohlcv".to_string(), "indicator_panel".to_string()]
    );
    assert_eq!(trader.model_requirement, "anthropic.claude-sonnet-4.6");
    assert_eq!(strategy.mechanical_params["ema_fast"], 12);
    assert_eq!(strategy.mechanical_params["ema_mid"], 26);
    assert_eq!(strategy.mechanical_params["ema_slow"], 50);
}

#[tokio::test]
async fn create_strategy_with_unknown_template_surfaces_engine_error_verbatim() {
    // Defensive: failing template lookup must surface the engine error
    // verbatim. The wizard relies on this path returning Err so its
    // `?` propagation prevents chaining `create_strategy_agent` against
    // a phantom id.
    let (ctx, _td) = fresh_api_context().await;
    let err = api_strategy::create_strategy(
        &ctx,
        authoring::CreateStrategyReq {
            template: "no_such_template".into(),
            name: "x".into(),
            creator: None,
        },
    )
    .await
    .expect_err("unknown template must error");
    let msg = err.to_string();
    assert!(
        msg.contains("no_such_template"),
        "error must name the failing template, got: {msg}"
    );
}
