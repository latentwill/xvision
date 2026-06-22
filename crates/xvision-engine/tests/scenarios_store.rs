//! Scenario CRUD store helper tests.
//!
//! Covers: insert + read roundtrip, immutability trigger (migration 006),
//! and archive flow.

use chrono::{TimeZone, Utc};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, Capital, DataSource, Fees, FillModel,
    LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario,
    ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings, DEFAULT_WARMUP_BARS,
};
use xvision_engine::eval::scenario_store as store;
use xvision_engine::safety::VenueLabel;

struct TestCtx {
    ctx: ApiContext,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for TestCtx {
    type Target = ApiContext;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

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
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2024, 2, 10, 0, 0, 0).unwrap(),
        },
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
            overrides: Vec::new(),
            borrow_bps_per_day: 5.0,
        },
        capital: Capital::default(),
        replay_mode: ReplayMode::Continuous,
        bar_cache_policy: BarCachePolicy {
            cache_key: format!("k_{}", id),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: DEFAULT_WARMUP_BARS,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap(),
        created_by: "test".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

async fn test_ctx() -> TestCtx {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();
    TestCtx { ctx, _dir: dir }
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
    assert!(msg.contains("immutable"), "expected immutability trigger: {msg}");
}

#[tokio::test]
async fn archive_succeeds() {
    let ctx = test_ctx().await;
    let s = make_test_scenario("sc_archive");
    store::insert_scenario(&ctx, &s).await.unwrap();
    store::archive_scenario(&ctx, "sc_archive").await.unwrap();
    let back = store::get_scenario(&ctx, "sc_archive").await.unwrap().unwrap();
    assert!(back.archived_at.is_some());
}

#[tokio::test]
async fn list_scenarios_filters_by_source_and_tags() {
    let ctx = test_ctx().await;
    let unique_tag = "regression-filter-20260521";
    let mut a = make_test_scenario("sc_a");
    a.tags = vec![unique_tag.into(), "crypto".into()];
    let mut b = make_test_scenario("sc_b");
    b.source = ScenarioSource::Canonical;
    b.tags = vec!["crypto".into()];
    store::insert_scenario(&ctx, &a).await.unwrap();
    store::insert_scenario(&ctx, &b).await.unwrap();

    let all = store::list_scenarios(&ctx, &store::ListScenariosFilter::default())
        .await
        .unwrap();
    let all_ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
    assert!(all_ids.contains(&"sc_a"));
    assert!(all_ids.contains(&"sc_b"));

    let user_only = store::list_scenarios(
        &ctx,
        &store::ListScenariosFilter {
            source: Some(ScenarioSource::User),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let user_ids: Vec<&str> = user_only.iter().map(|s| s.id.as_str()).collect();
    assert!(user_ids.contains(&"sc_a"));
    assert!(!user_ids.contains(&"sc_b"));

    let tagged = store::list_scenarios(
        &ctx,
        &store::ListScenariosFilter {
            tags: vec![unique_tag.into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].id, "sc_a");
}

#[tokio::test]
async fn list_scenarios_can_exclude_optimizer_tagged_rows() {
    let ctx = test_ctx().await;
    let mut regular = make_test_scenario("sc_regular");
    regular.tags = vec!["operator".into()];
    let mut optimizer = make_test_scenario("sc_optimizer");
    optimizer.source = ScenarioSource::Generated;
    optimizer.tags = vec!["source:autooptimizer".into()];
    store::insert_scenario(&ctx, &regular).await.unwrap();
    store::insert_scenario(&ctx, &optimizer).await.unwrap();

    let visible = store::list_scenarios(
        &ctx,
        &store::ListScenariosFilter {
            exclude_tags: vec!["source:autooptimizer".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let ids: Vec<&str> = visible.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"sc_regular"));
    assert!(!ids.contains(&"sc_optimizer"));

    let optimizer_folder = store::list_scenarios(
        &ctx,
        &store::ListScenariosFilter {
            source: Some(ScenarioSource::Generated),
            tags: vec!["source:autooptimizer".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(optimizer_folder.len(), 1);
    assert_eq!(optimizer_folder[0].id, "sc_optimizer");
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
        "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at) VALUES (?, ?, ?, ?, ?, ?)",
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

    let err = store::delete_scenario(&ctx, "sc_with_run").await.unwrap_err();
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
