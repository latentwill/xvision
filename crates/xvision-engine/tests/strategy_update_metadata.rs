//! Engine-level integration coverage for `StrategyStore::update_metadata`
//! (track: `strategy-edit-top-level-fields`).
//!
//! Two invariants this file pins down:
//!
//! 1. The patch surface mutates only the in-scope fields
//!    (display_name / plain_summary / asset_universe) and leaves
//!    everything else — including the strategy id, template, creator,
//!    agents, pipeline, and risk — untouched.
//!
//! 2. **Cycle-id-stable round-trip**: editing a strategy that has
//!    completed eval runs against it must keep the `agent_id ==
//!    strategy_id` ULID stable, so the orphan-runs scenario from QA
//!    operator round 4 item 2 (delete-and-recreate losing run history)
//!    is fully avoided.

use sqlx::SqlitePool;
use xvision_engine::api::{strategy as api_strategy, Actor, ApiContext};
use xvision_engine::authoring::CreateStrategyReq;
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::store::{
    strategy_store_dir, FilesystemStore, MetadataPatchError, StrategyMetadataPatch, StrategyStore,
};

async fn open_ctx() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "metadata-patch-test".into(),
        },
    )
    .await
    .expect("open ApiContext");
    (ctx, dir)
}

async fn seed_strategy(ctx: &ApiContext) -> String {
    // Post-2026-05-21 template-registry removal: `create_strategy`
    // produces a blank draft. Subsequent edits flesh it out; the
    // patch surface tested below only mutates the in-scope manifest
    // fields, so a blank starter is sufficient.
    let out = api_strategy::create_strategy(
        ctx,
        CreateStrategyReq {
            name: "Pre-Edit Title".into(),
            creator: Some("@op".into()),
        },
    )
    .await
    .expect("create strategy");
    // Seed an asset_universe so the metadata patch tests have something
    // to compare against on the "preserve out-of-scope fields" check.
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let mut strategy = store.load(&out.id).await.unwrap();
    strategy.manifest.asset_universe = vec!["BTC/USD".into()];
    strategy.manifest.plain_summary = "seed summary".into();
    store.save(&strategy).await.unwrap();
    out.id
}

fn store_for(ctx: &ApiContext) -> FilesystemStore {
    FilesystemStore::new(strategy_store_dir(&ctx.xvn_home))
}

async fn seed_completed_run(pool: &SqlitePool, agent_id: &str) -> String {
    let store = RunStore::new(pool.clone());
    let mut run = Run::new_queued(
        agent_id.to_string(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    run.status = RunStatus::Completed;
    let run_id = run.id.clone();
    store.create(&run).await.expect("seed run");
    store
        .update_status(&run_id, RunStatus::Completed, None)
        .await
        .expect("transition to terminal");
    run_id
}

#[tokio::test]
async fn update_metadata_applies_in_scope_fields_and_preserves_others() {
    let (ctx, _d) = open_ctx().await;
    let id = seed_strategy(&ctx).await;
    let store = store_for(&ctx);

    let before = store.load(&id).await.unwrap();
    let original_template = before.manifest.template.clone();
    let original_creator = before.manifest.creator.clone();
    let original_risk_basis = before.manifest.risk_preset_or_config.clone();
    let original_risk = before.risk.clone();
    let original_agents = before.agents.clone();
    let original_pipeline = before.pipeline.clone();

    let patched = store
        .update_metadata(
            &id,
            StrategyMetadataPatch {
                display_name: Some("After-Edit Title".into()),
                plain_summary: Some("Updated summary".into()),
                asset_universe: Some(vec!["BTC/USD".into(), "eth/usd".into()]),
                decision_cadence_minutes: None,
                color: None,
            },
        )
        .await
        .unwrap();

    // In-scope fields move.
    assert_eq!(patched.manifest.display_name, "After-Edit Title");
    assert_eq!(patched.manifest.plain_summary, "Updated summary");
    assert_eq!(
        patched.manifest.asset_universe,
        vec!["BTC/USD".to_string(), "ETH/USD".to_string()]
    );
    // Strategy id is stable.
    assert_eq!(patched.manifest.id, id);

    // Every out-of-scope field is byte-for-byte identical.
    assert_eq!(patched.manifest.template, original_template);
    assert_eq!(patched.manifest.creator, original_creator);
    assert_eq!(patched.manifest.risk_preset_or_config, original_risk_basis);
    assert_eq!(patched.risk, original_risk);
    assert_eq!(patched.agents, original_agents);
    assert_eq!(patched.pipeline, original_pipeline);
}

#[tokio::test]
async fn update_metadata_validation_failure_does_not_partially_mutate_disk() {
    let (ctx, _d) = open_ctx().await;
    let id = seed_strategy(&ctx).await;
    let store = store_for(&ctx);

    let before = store.load(&id).await.unwrap();

    let err = store
        .update_metadata(
            &id,
            StrategyMetadataPatch {
                // First field is valid; second triggers
                // EmptyPlainSummary; together they should leave the
                // strategy on disk unchanged.
                display_name: Some("Would-Apply".into()),
                plain_summary: Some("".into()),
                asset_universe: None,
                decision_cadence_minutes: None,
                color: None,
            },
        )
        .await
        .expect_err("blank plain_summary must be rejected");
    let typed: Option<&MetadataPatchError> = err.downcast_ref();
    assert_eq!(typed, Some(&MetadataPatchError::EmptyPlainSummary));

    let after = store.load(&id).await.unwrap();
    assert_eq!(after.manifest.display_name, before.manifest.display_name);
    assert_eq!(after.manifest.plain_summary, before.manifest.plain_summary);
    assert_eq!(after.manifest.asset_universe, before.manifest.asset_universe);
}

#[tokio::test]
async fn update_metadata_keeps_strategy_id_stable_across_completed_run_history() {
    // QA operator round 4 item 2 specifically called out: a typo in
    // the create wizard forced a delete-and-recreate, which orphans
    // every eval run that referenced the original strategy_id.
    //
    // This test pins the desired behaviour: an in-place metadata edit
    // keeps the agent_id ULID stable, so the seeded completed run
    // continues to resolve to the (now edited) strategy.
    let (ctx, _d) = open_ctx().await;
    let id = seed_strategy(&ctx).await;
    let store = store_for(&ctx);

    let run_id = seed_completed_run(&ctx.db, &id).await;

    // Patch every in-scope field.
    let patched = store
        .update_metadata(
            &id,
            StrategyMetadataPatch {
                display_name: Some("Renamed After Run".into()),
                plain_summary: Some("Eval-run history must survive this edit.".into()),
                asset_universe: Some(vec!["BTC/USD".into()]),
                decision_cadence_minutes: None,
                color: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(patched.manifest.id, id, "strategy id must not move");

    // Eval-run linkage is by `agent_id` (string ULID, identical to
    // strategy_id pre-mint). Confirm the seeded run still resolves
    // and still points at the (now edited) strategy.
    let run_store = RunStore::new(ctx.db.clone());
    let reloaded_run = run_store
        .get(&run_id)
        .await
        .expect("seeded run still queryable post-edit");
    assert_eq!(reloaded_run.agent_id, id);
    assert_eq!(reloaded_run.status, RunStatus::Completed);

    // And the strategy on disk has the new title.
    let reloaded_strategy = store.load(&id).await.unwrap();
    assert_eq!(reloaded_strategy.manifest.display_name, "Renamed After Run");
}
