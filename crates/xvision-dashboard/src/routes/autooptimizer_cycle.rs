//! POST /api/autooptimizer/evening-cycle — launch an evening optimizer run.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use xvision_core::config::ProviderKind;
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, OpenaiCompatDispatch};
use xvision_engine::autooptimizer::{
    blob_store::BlobStore,
    config::AutoOptimizerConfig,
    content_hash::ContentHash,
    cycle::{run_evening_cycle, CycleConfig},
    eval_adapter::StubPaperTester,
    gate::GateVerdict,
    judge::Judge,
    lineage::{LineageNode, LineageStatus, LineageStore},
    mutator::Mutator,
    parent_policy::ParentPolicy,
    scenario_synthesis::synthesize_baseline_untouched_scenario,
};
use xvision_engine::eval::run::MetricsSummary;
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, BarGranularity, CalendarRef, Capital, DataSource, Fees,
    FillModel, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode,
    Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings, DEFAULT_WARMUP_BARS,
};
use xvision_engine::safety::VenueLabel;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::Strategy;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct StartCycleBody {
    pub strategy_id: Option<String>,
    pub mutator_model: Option<String>,
    pub judge_model: Option<String>,
}

#[derive(Serialize)]
pub struct StartCycleResponse {
    pub started: bool,
    pub message: String,
}

pub async fn start_evening_cycle(
    State(state): State<AppState>,
    Json(body): Json<StartCycleBody>,
) -> Result<(StatusCode, Json<StartCycleResponse>), DashboardError> {
    let cfg = load_optimizer_config()?;
    let mutator_model = body.mutator_model.unwrap_or_else(|| cfg.mutator.model.clone());
    let judge_model = body.judge_model.unwrap_or_else(|| cfg.mutator.model.clone());
    let dispatch = build_autooptimizer_dispatch(&cfg.mutator.provider, &state.xvn_home).await?;
    let day_scenario = build_day_scenario(&cfg)?;
    let baseline_scenario =
        synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)?;
    let (mutator, judge) = build_mutator_and_judge(&cfg, mutator_model, judge_model, dispatch);
    let pool = state.pool.clone();
    let lineage_store = LineageStore::new(pool.clone());
    let strategy_blob_store = BlobStore::new(state.xvn_home.join("lineage").join("blobs"));
    let strategy_id = body
        .strategy_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DashboardError::Validation {
            field: "strategy_id".into(),
            msg: "strategy_id is required for dashboard evening-cycle launches".into(),
        })?;
    let (bundle_hash, strategy) =
        load_strategy_parent(strategy_id, &state.xvn_home, &lineage_store, &strategy_blob_store).await?;
    let mut parent_strategies = HashMap::new();
    parent_strategies.insert(bundle_hash.to_hex(), strategy);
    let explicit_parent_hashes = vec![bundle_hash];
    let cycle_config = build_cycle_config(
        &cfg,
        &judge,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes,
    );
    let tx = state.autooptimizer_tx.clone();
    let obs_blob_store =
        xvision_observability::BlobStore::new(state.xvn_home.join("lineage").join("obs-blobs"));
    tokio::spawn(async move {
        let paper_tester = Arc::new(stub_paper_tester());
        let result = run_evening_cycle(
            &pool,
            &obs_blob_store,
            &cfg,
            &cycle_config,
            &ParentPolicy::RoundRobin,
            &mutator,
            &judge,
            paper_tester.as_ref(),
            move |ev| {
                let _ = tx.send(ev);
            },
            None,
            None,
        )
        .await;
        if let Err(e) = result {
            tracing::warn!(error = %e, "evening cycle failed");
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(StartCycleResponse {
            started: true,
            message: "Evening run started. Watch the Live tab for progress.".into(),
        }),
    ))
}

fn load_optimizer_config() -> Result<AutoOptimizerConfig, DashboardError> {
    let cfg = match AutoOptimizerConfig::default_path() {
        Ok(path) if path.exists() => AutoOptimizerConfig::load(&path)?,
        _ => AutoOptimizerConfig::default(),
    };
    Ok(cfg)
}

async fn build_autooptimizer_dispatch(
    provider: &str,
    xvn_home: &std::path::Path,
) -> Result<Arc<dyn LlmDispatch + Send + Sync>, DashboardError> {
    let config_path = if let Ok(p) = std::env::var("XVN_CONFIG_PATH") {
        if !p.is_empty() {
            std::path::PathBuf::from(p)
        } else {
            xvn_home.join("config").join("default.toml")
        }
    } else {
        xvn_home.join("config").join("default.toml")
    };
    let provider_name = provider.to_owned();
    let rt = tokio::task::spawn_blocking(move || xvision_core::config::load_runtime(&config_path))
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("load runtime config: {e}")))?;
    let entry = rt
        .providers
        .iter()
        .find(|p| p.name == provider_name)
        .ok_or_else(|| DashboardError::Validation {
            field: "provider".into(),
            msg: format!("autooptimizer provider '{provider_name}' not configured in Settings → Providers"),
        })?;
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).map_err(|_| DashboardError::Validation {
            field: "provider".into(),
            msg: format!(
                "env var '{}' unset for provider '{provider_name}'",
                entry.api_key_env
            ),
        })?
    };
    Ok(match entry.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicDispatch::new(api_key)),
        ProviderKind::OpenaiCompat | ProviderKind::Ollama | ProviderKind::LlamaCpp => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::LocalCandle => {
            return Err(DashboardError::Validation {
                field: "provider".into(),
                msg: "local-candle is not supported for the autooptimizer".into(),
            })
        }
    })
}

fn build_mutator_and_judge(
    cfg: &AutoOptimizerConfig,
    mutator_model: String,
    judge_model: String,
    dispatch: Arc<dyn LlmDispatch + Send + Sync>,
) -> (Mutator, Judge) {
    let mutator = Mutator {
        provider: cfg.mutator.provider.clone(),
        model: mutator_model,
        dispatch: Arc::clone(&dispatch),
        max_retries: cfg.mutator.max_retries,
    };
    let judge = Judge {
        dispatch,
        provider: cfg.mutator.provider.clone(),
        model: judge_model,
    };
    (mutator, judge)
}

fn build_cycle_config(
    cfg: &AutoOptimizerConfig,
    judge: &Judge,
    day_scenario: Scenario,
    baseline_scenario: Scenario,
    parent_strategies: HashMap<String, Strategy>,
    explicit_parent_hashes: Vec<ContentHash>,
) -> CycleConfig {
    CycleConfig {
        num_parents: if explicit_parent_hashes.is_empty() {
            2
        } else {
            explicit_parent_hashes.len()
        },
        mutations_per_parent: 1,
        sabotage_seed: 42,
        judge_provider: cfg.mutator.provider.clone(),
        judge_model: judge.model.clone(),
        prompt_version: "v1".into(),
        sustained_no_pass_cycles: 0,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes,
    }
}

async fn load_strategy_parent(
    strategy_id: &str,
    xvn_home: &std::path::Path,
    lineage: &LineageStore,
    blobs: &BlobStore,
) -> Result<(ContentHash, Strategy), DashboardError> {
    let store = FilesystemStore::new(strategy_store_dir(xvn_home));
    store
        .path_for(strategy_id)
        .map_err(|e| DashboardError::Validation {
            field: "strategy_id".into(),
            msg: format!("invalid strategy id '{strategy_id}': {e}"),
        })?;
    let strategy = store.load(strategy_id).await.map_err(|e| {
        if e.to_string().contains("reading ") {
            DashboardError::NotFound(format!("strategy '{strategy_id}' not found"))
        } else {
            DashboardError::Internal(anyhow::anyhow!("load strategy '{strategy_id}': {e}"))
        }
    })?;
    let strategy_json = serde_json::to_value(&strategy)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("serialize strategy '{strategy_id}': {e}")))?;
    let bundle_hash = blobs
        .put_json(&strategy_json)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("write strategy blob '{strategy_id}': {e}")))?;

    match lineage
        .get(&bundle_hash)
        .await
        .map_err(DashboardError::Internal)?
    {
        Some(node) if node.status != LineageStatus::Active => {
            return Err(DashboardError::Validation {
                field: "strategy_id".into(),
                msg: format!(
                    "strategy '{strategy_id}' resolves to lineage parent {} but that parent is not active",
                    bundle_hash.to_hex()
                ),
            });
        }
        Some(_) => {}
        None => {
            let root_node = LineageNode {
                bundle_hash,
                parent_hash: None,
                gate_verdict: GateVerdict::Pass,
                status: LineageStatus::Active,
                cycle_id: None,
                created_at: Utc::now(),
                diversity_score: None,
            };
            lineage
                .insert(&root_node)
                .await
                .map_err(DashboardError::Internal)?;
        }
    }

    Ok((bundle_hash, strategy))
}

fn build_day_scenario(cfg: &AutoOptimizerConfig) -> Result<Scenario, DashboardError> {
    let start = Utc.from_utc_datetime(&cfg.day_window.start.and_hms_opt(0, 0, 0).expect("valid midnight"));
    let end = Utc.from_utc_datetime(&cfg.day_window.end.and_hms_opt(0, 0, 0).expect("valid midnight"));
    Ok(Scenario {
        id: format!("ec-day-{}", Ulid::new()),
        parent_scenario_id: None,
        source: ScenarioSource::Generated,
        display_name: "Evening cycle day window".into(),
        description: format!(
            "Synthesized day window {} – {}",
            cfg.day_window.start, cfg.day_window.end
        ),
        tags: vec![],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow { start, end },
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
            slippage: SlippageModel::None,
            latency: LatencyModel {
                decision_to_fill_ms: 250,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: vec![],
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Continuous,
        capital: Capital::default(),
        bar_cache_policy: BarCachePolicy {
            cache_key: format!("ec-day-{}-{}", cfg.day_window.start, cfg.day_window.end),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: DEFAULT_WARMUP_BARS,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc::now(),
        created_by: "xvn-dashboard".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    })
}

fn stub_paper_tester() -> StubPaperTester {
    StubPaperTester {
        metrics: MetricsSummary {
            sharpe: 0.9,
            total_return_pct: 5.0,
            max_drawdown_pct: 3.0,
            win_rate: 0.55,
            n_trades: 10,
            n_decisions: 20,
            inference_cost_quote_total: None,
            net_return_pct: None,
            baselines: None,
        },
    }
}
