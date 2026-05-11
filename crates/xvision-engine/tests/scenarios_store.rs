//! Scenario CRUD store helper tests.
//!
//! Covers: insert + read roundtrip, immutability trigger (migration 006),
//! and archive flow.

use chrono::{TimeZone, Utc};
use xvision_engine::api::{Actor, ApiContext};
use xvision_data::alpaca::BarGranularity;
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, AssetRef, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel,
    LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode,
    Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
};
use xvision_engine::eval::scenario_store as store;

fn make_test_scenario(id: &str) -> Scenario {
    Scenario {
        id: id.into(),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: format!("test {}", id),
        description: "".into(),
        tags: vec!["regression".into()],
        notes: None,
        asset_class: AssetClass::Crypto,
        asset: vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "ETH".into(),
            venue_symbol: "ETH/USD".into(),
        }],
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2024, 2, 10, 0, 0, 0).unwrap(),
        },
        granularity: BarGranularity::Hour1,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: None,
            adjustment: AdjustmentMode::Raw,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 10,
                taker_bps: 25,
            },
            slippage: SlippageModel::Linear { bps: 5 },
            latency: LatencyModel {
                decision_to_fill_ms: 500,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
        },
        replay_mode: ReplayMode::Continuous,
        bar_cache_policy: BarCachePolicy {
            cache_key: format!("k_{}", id),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        created_at: Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap(),
        created_by: "test".into(),
        archived_at: None,
    }
}

async fn test_ctx() -> ApiContext {
    let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn insert_and_read_scenario() {
    let ctx = test_ctx().await;
    let s = make_test_scenario("sc_1");
    store::insert_scenario(&ctx, &s).await.unwrap();
    let back = store::get_scenario(&ctx, "sc_1").await.unwrap().unwrap();
    assert_eq!(back.id, "sc_1");
    assert_eq!(back.display_name, s.display_name);
    assert_eq!(back.tags, s.tags);
}

#[tokio::test]
async fn immutable_update_rejected() {
    let ctx = test_ctx().await;
    let s = make_test_scenario("sc_immut");
    store::insert_scenario(&ctx, &s).await.unwrap();
    let err = sqlx::query("UPDATE scenarios SET display_name = 'hacked' WHERE id = ?")
        .bind(&s.id)
        .execute(&ctx.db)
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("immutable"),
        "expected immutability trigger: {msg}"
    );
}

#[tokio::test]
async fn archive_succeeds() {
    let ctx = test_ctx().await;
    let s = make_test_scenario("sc_archive");
    store::insert_scenario(&ctx, &s).await.unwrap();
    store::archive_scenario(&ctx, "sc_archive").await.unwrap();
    let back = store::get_scenario(&ctx, "sc_archive")
        .await
        .unwrap()
        .unwrap();
    assert!(back.archived_at.is_some());
}

#[tokio::test]
async fn list_scenarios_filters_by_source_and_tags() {
    let ctx = test_ctx().await;
    let mut a = make_test_scenario("sc_a");
    a.tags = vec!["regression".into(), "crypto".into()];
    let mut b = make_test_scenario("sc_b");
    b.source = ScenarioSource::Canonical;
    b.tags = vec!["crypto".into()];
    store::insert_scenario(&ctx, &a).await.unwrap();
    store::insert_scenario(&ctx, &b).await.unwrap();

    let all = store::list_scenarios(&ctx, &store::ListScenariosFilter::default())
        .await
        .unwrap();
    assert_eq!(all.len(), 2);

    let user_only = store::list_scenarios(
        &ctx,
        &store::ListScenariosFilter {
            source: Some(ScenarioSource::User),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(user_only.len(), 1);
    assert_eq!(user_only[0].id, "sc_a");

    let tagged = store::list_scenarios(
        &ctx,
        &store::ListScenariosFilter {
            tags: vec!["regression".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].id, "sc_a");
}

#[tokio::test]
async fn list_scenarios_hides_archived_by_default() {
    let ctx = test_ctx().await;
    let s = make_test_scenario("sc_arch_list");
    store::insert_scenario(&ctx, &s).await.unwrap();
    store::archive_scenario(&ctx, "sc_arch_list").await.unwrap();

    let visible = store::list_scenarios(&ctx, &store::ListScenariosFilter::default())
        .await
        .unwrap();
    assert!(visible.iter().all(|s| s.id != "sc_arch_list"));

    let all = store::list_scenarios(
        &ctx,
        &store::ListScenariosFilter {
            include_archived: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(all.iter().any(|s| s.id == "sc_arch_list"));
}

#[tokio::test]
async fn list_children_returns_descendants() {
    let ctx = test_ctx().await;
    let parent = make_test_scenario("sc_parent");
    store::insert_scenario(&ctx, &parent).await.unwrap();

    let mut child1 = make_test_scenario("sc_child1");
    child1.parent_scenario_id = Some("sc_parent".into());
    let mut child2 = make_test_scenario("sc_child2");
    child2.parent_scenario_id = Some("sc_parent".into());
    store::insert_scenario(&ctx, &child1).await.unwrap();
    store::insert_scenario(&ctx, &child2).await.unwrap();

    let children = store::list_children(&ctx, "sc_parent").await.unwrap();
    assert_eq!(children.len(), 2);
    let ids: Vec<&str> = children.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"sc_child1"));
    assert!(ids.contains(&"sc_child2"));
}

#[tokio::test]
async fn delete_blocked_when_runs_reference_scenario() {
    let ctx = test_ctx().await;
    let s = make_test_scenario("sc_with_run");
    store::insert_scenario(&ctx, &s).await.unwrap();

    // Insert a fake eval_runs row referencing this scenario.
    sqlx::query(
        "INSERT INTO eval_runs (id, strategy_bundle_hash, scenario_id, mode, status, started_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("run_1")
    .bind("hash")
    .bind("sc_with_run")
    .bind("backtest")
    .bind("queued")
    .bind("2026-05-11T00:00:00Z")
    .execute(&ctx.db)
    .await
    .unwrap();

    let err = store::delete_scenario(&ctx, "sc_with_run")
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("Archive instead") || msg.contains("reference"),
        "expected validation error, got: {msg}"
    );
}

#[tokio::test]
async fn delete_succeeds_when_no_runs_reference() {
    let ctx = test_ctx().await;
    let s = make_test_scenario("sc_del");
    store::insert_scenario(&ctx, &s).await.unwrap();
    store::delete_scenario(&ctx, "sc_del").await.unwrap();
    let back = store::get_scenario(&ctx, "sc_del").await.unwrap();
    assert!(back.is_none());
}
