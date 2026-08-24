use chrono::{TimeZone, Timelike, Utc};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::eval::filter_hook::FilterHook;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_filters::{parse_toml, ActivationMode, Filter};
use xvision_engine::strategies::DecisionMode;

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/032_filters_and_evaluations.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn build_strategy(activation_mode: ActivationMode, filter: Option<Filter>) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01FILTERHOOKSTRATEGY000000000".into(),
            display_name: "filter hook test strategy".into(),
            plain_summary: "for eval filter hook tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
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
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
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
        risk: RiskPreset::Balanced.expand(),
        activation_mode,
        filter,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn simple_close_filter() -> Filter {
    parse_toml(
        r#"
[filter]
id = "f_filter_hook_smoke"
strategy_id = "s_filter_hook_smoke"
display_name = "Close above zero"
asset_scope = ["BTC/USD"]
timeframe = "1h"
scan_cadence = "bar_close"
cooldown_bars = 0
wake_when_in_position = "always"
agent_context_template = "compact_trade_context_v1"

[[filter.conditions.all]]
lhs = "close"
op  = ">"
rhs = 0.0
"#,
    )
    .unwrap()
}

fn fire_context_filter() -> Filter {
    parse_toml(
        r#"
[filter]
id = "f_filter_hook_fire"
strategy_id = "s_filter_hook_fire"
display_name = "Close fire context"
asset_scope = ["BTC/USD"]
timeframe = "1h"

[filter.fire]
reason = "close_breakout"
priority = 0.8
tags = ["breakout"]
context = ["close", "volume_zscore_3"]

[[filter.conditions.all]]
lhs = "close"
op  = ">"
rhs = 0.0
"#,
    )
    .unwrap()
}

fn bar(close: f64) -> Ohlcv {
    Ohlcv {
        timestamp: Utc.with_ymd_and_hms(2026, 5, 22, 0, 0, 0).unwrap(),
        open: close,
        high: close + 1.0,
        low: close - 1.0,
        close,
        volume: 1_000.0,
    }
}

#[test]
fn every_bar_strategy_has_no_filter_hook() {
    let strategy = build_strategy(ActivationMode::EveryBar, None);
    let hook = FilterHook::new(&strategy).unwrap();
    assert!(hook.is_none());
}

#[test]
fn filter_gated_strategy_requires_filter() {
    let strategy = build_strategy(ActivationMode::FilterGated, None);
    let err = match FilterHook::new(&strategy) {
        Ok(_) => panic!("FilterGated without filter should fail"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("E_FILTER_GATED_WITHOUT_FILTER"));
}

#[tokio::test]
async fn hook_records_filter_event_json_and_summary() {
    let pool = migrated_pool().await;
    let filter = simple_close_filter();
    let strategy = build_strategy(ActivationMode::FilterGated, Some(filter));
    let mut hook = FilterHook::new(&strategy).unwrap().expect("filter hook");
    let bar = bar(100.0);

    let evaluation = hook.evaluate(&bar, false);
    assert!(evaluation.outcome.decision.is_trip());
    assert!(evaluation.event.triggered);
    assert_eq!(evaluation.event.conditions_passed, vec![0]);
    assert_eq!(evaluation.event.indicator_snapshot.get("close"), Some(&100.0));

    hook.record(&pool, None, "run-filter-hook", bar.timestamp, &evaluation)
        .await
        .unwrap();

    let tag: String = sqlx::query("SELECT decision_tag FROM eval_filter_evaluations WHERE run_id = ?")
        .bind("run-filter-hook")
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("decision_tag")
        .unwrap();
    assert_eq!(tag, "trip");

    let store = RunStore::new(pool);
    let events = store.read_filter_events("run-filter-hook").await.unwrap();
    assert_eq!(events, vec![evaluation.event]);

    let summaries = store.read_filter_summaries("run-filter-hook").await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].bars_scanned, 1);
    assert_eq!(summaries[0].wakeups, 1);
    assert_eq!(summaries[0].llm_calls_saved, 0);
}

#[test]
fn active_filter_builds_fire_trigger_context() {
    let filter = fire_context_filter();
    let strategy = build_strategy(ActivationMode::FilterGated, Some(filter));
    let mut hook = FilterHook::new(&strategy).unwrap().expect("filter hook");

    assert!(!hook.evaluate(&bar(100.0), false).outcome.decision.is_active());
    assert!(!hook.evaluate(&bar(101.0), false).outcome.decision.is_active());
    let active = hook.evaluate(&bar(102.0), false);
    assert!(active.outcome.decision.is_active());
    let trigger = active.trigger_context.expect("trigger context");
    assert_eq!(trigger["reason"], "close_breakout");
    assert_eq!(trigger["priority"], 0.8);
    assert_eq!(trigger["tags"], serde_json::json!(["breakout"]));
    assert_eq!(trigger["context"]["close"], 102.0);
}

#[test]
fn mechanistic_hook_matches_offline_window_indicators() {
    #[derive(Clone)]
    struct CsvBar {
        ts: chrono::DateTime<Utc>,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    }
    fn parse(line: &str) -> CsvBar {
        let f: Vec<&str> = line.split(',').collect();
        CsvBar {
            ts: f[0].parse().unwrap(),
            open: f[1].parse().unwrap(),
            high: f[2].parse().unwrap(),
            low: f[3].parse().unwrap(),
            close: f[4].parse().unwrap(),
            volume: f[5].parse().unwrap(),
        }
    }
    let raw = include_str!("../../xvision-filters/tests/fixtures/btc_5m.csv");
    let mut rows = raw.lines().skip(1).map(parse).filter(|bar| {
        bar.ts >= Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()
    });
    let filter = parse_toml(
        r#"
[filter]
id = "f_mech_golden"
strategy_id = "01FILTERHOOKSTRATEGY000000000"
display_name = "mechanistic golden"
asset_scope = ["BTC/USD"]
timeframe = "1h"
scan_cadence = "bar_close"
wake_when_in_position = "always"
[[filter.conditions.all]]
lhs = "close"
op = ">"
rhs = -1e99
[[filter.conditions.all]]
lhs = "ema_9"
op = ">"
rhs = -1e99
[[filter.conditions.all]]
lhs = "ema_21"
op = ">"
rhs = -1e99
[[filter.conditions.all]]
lhs = "ema_50"
op = ">"
rhs = -1e99
[[filter.conditions.all]]
lhs = "adx_14"
op = ">"
rhs = -1e99
[[filter.conditions.all]]
lhs = "rvol_tod_20"
op = ">"
rhs = -1e99
"#,
    )
    .unwrap();
    let mut strategy = build_strategy(ActivationMode::FilterGated, Some(filter));
    strategy.decision_mode = DecisionMode::Mechanistic;
    let mut hook = FilterHook::new(&strategy).unwrap().expect("filter hook");
    let golden = include_str!("../../xvision-filters/tests/fixtures/golden-btc-1h.csv");
    let expected: std::collections::BTreeMap<_, _> = golden
        .lines()
        .skip(1)
        .map(|line| {
            let fields: Vec<&str> = line.split(',').collect();
            (fields[0].parse::<chrono::DateTime<Utc>>().unwrap(), fields)
        })
        .collect();
    let mut gate_mismatches = Vec::new();
    for bar in rows {
        let evaluation = hook.evaluate_resampled(
            &Ohlcv {
                timestamp: bar.ts,
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
            },
            false,
        );
        let Some(evaluation) = evaluation else { continue };
        let Some(fields) = expected.get(&evaluation.event.bar_timestamp) else {
            continue;
        };
        for (idx, key) in ["ema_9", "ema_21", "ema_50", "adx_14", "rvol_tod_20"]
            .into_iter()
            .enumerate()
        {
            let expected_value: f64 = fields[idx + 6].parse().unwrap();
            let actual = *evaluation.event.indicator_snapshot.get(key).unwrap();
            assert!(
                (actual - expected_value).abs() <= 1e-4 * expected_value.abs().max(1.0),
                "{} {} engine={} offline={}",
                evaluation.event.bar_timestamp,
                key,
                actual,
                expected_value
            );
        }
        let snapshot = &evaluation.event.indicator_snapshot;
        let gate = evaluation.event.bar_timestamp.hour() >= 18
            && snapshot["ema_9"] > snapshot["ema_21"]
            && snapshot["ema_21"] > snapshot["ema_50"]
            && snapshot["close"] > snapshot["ema_21"]
            && snapshot["adx_14"] > 30.0
            && snapshot["rvol_tod_20"] > 1.3;
        let expected_gate = fields[11] == "1";
        if gate != expected_gate {
            gate_mismatches.push((evaluation.event.bar_timestamp, gate, expected_gate));
        }
        checked += 1;
    }
    assert!(
        gate_mismatches.is_empty(),
        "gate mismatches: {:?}",
        gate_mismatches
    );
    assert_eq!(checked, 145, "all offline buckets must be checked");
}
