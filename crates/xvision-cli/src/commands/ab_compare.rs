//! `xvn ab-compare` — N-arm backtest A/B runner.
//!
//! Each `trader_arm` may carry inline `intern=<provider>/<model>` and
//! `trader=<provider>/<model>` overrides; otherwise the global CLI flags
//! supply the defaults via the `ProviderRegistry`. Two arms that resolve
//! to the same `(provider, model)` share a backend `Arc` so they reuse one
//! HTTP client.
//!
//! Bars input has two paths:
//! 1. `--bars <path>` — legacy JSON file (`Vec<MarketBar>`).
//! 2. `--from --to [--granularity]` — cache-backed: routes through
//!    `engine::eval::bars::load_bars`, which reads/writes the SQLite
//!    `bars_cache` table and falls back to Alpaca on miss. Pre-warm via
//!    `xvn bars fetch` if you want to skip the upstream call.
//!
//! Cycles (`MarketSnapshot`) are always JSON for now — building snapshots
//! from raw bars is downstream work (the harness has no helper yet).

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use chrono::NaiveDate;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_core::market::MarketSnapshot;
use xvision_core::slot::SlotRef;
use xvision_core::trading::{AssetSymbol, PortfolioState};
use xvision_data::alpaca::BarGranularity;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::bars::{compute_cache_key, load_bars, BarCacheArgs};
use xvision_eval::ab_compare::{auto_suffix_arm_names, default_arms, parse_arm_spec, run_ab_compare};
use xvision_eval::backtest::MarketBar;
use xvision_eval::baselines::PortfolioProvider;
use xvision_eval::harness::BacktestRunConfig;
use xvision_eval::provider_registry::ProviderRegistry;
use xvision_trader::TraderParams;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    cycles: PathBuf,
    bars: Option<PathBuf>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    granularity: String,
    arms: String,
    output: PathBuf,
    initial_nav_usd: f64,
    fee_bps: u32,
    step_hours: u32,
    horizon_hours: u32,
    asset: String,
    intern_provider: String,
    intern_model: String,
    trader_base_url: String,
    trader_model: String,
    trader_api_key_env: String,
) -> anyhow::Result<()> {
    let asset_sym = AssetSymbol::from_str(&asset)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let snapshots: Vec<MarketSnapshot> = serde_json::from_slice(&std::fs::read(&cycles)?)?;
    let bars_vec: Vec<MarketBar> = load_bars_input(
        bars.as_ref(),
        from,
        to,
        &granularity,
        asset_sym,
    )
    .await?;

    let mut arm_specs: Vec<_> = if arms.trim().is_empty() {
        default_arms()
    } else {
        arms.split(',')
            .map(|s| parse_arm_spec(s.trim()))
            .collect::<anyhow::Result<Vec<_>>>()?
    };
    auto_suffix_arm_names(&mut arm_specs);

    // Build the provider registry from `config/default.toml`'s `[[providers]]`
    // rows (Plan #7 Phase 1) plus a synthesized fallback row for the CLI's
    // `--trader-base-url` / `--trader-api-key-env` if no existing provider
    // matches that triple. The CLI's `--intern --intern-model` selects which
    // existing provider's `(base_url, api_key_env)` is used for the Intern
    // default — matching by `ProviderKind`.
    let workspace_root = std::env::current_dir()?;
    let runtime_cfg = xvision_core::config::load_runtime(&workspace_root.join("config/default.toml"))?;
    let mut rows = runtime_cfg.providers;

    let cli_trader_kind = ProviderKind::OpenaiCompat;
    let cli_trader_provider_name = rows
        .iter()
        .find(|p| p.matches_triple(cli_trader_kind, &trader_base_url, &trader_api_key_env))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| {
            let synth_name = "_cli_default_trader".to_string();
            rows.push(ProviderEntry {
                name: synth_name.clone(),
                kind: cli_trader_kind,
                base_url: trader_base_url.clone(),
                api_key_env: trader_api_key_env.clone(),
                enabled_models: Vec::new(),
            });
            synth_name
        });

    let cli_intern_kind: ProviderKind = match intern_provider.as_str() {
        "anthropic" => ProviderKind::Anthropic,
        "openai-compat" => ProviderKind::OpenaiCompat,
        other => anyhow::bail!("unknown intern provider: {other}"),
    };
    let cli_intern_provider_name = rows
        .iter()
        .find(|p| p.kind == cli_intern_kind)
        .map(|p| p.name.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no provider row matches CLI --intern={intern_provider}; \
                 register one under [[providers]] in config/default.toml"
            )
        })?;

    let registry = Arc::new(ProviderRegistry::new(
        rows,
        SlotRef::new(cli_intern_provider_name, intern_model),
        SlotRef::new(cli_trader_provider_name, trader_model),
    ));

    let risk = xvision_harness::load_risk_layer(&workspace_root)?;

    let cfg = BacktestRunConfig {
        initial_nav_usd,
        fee_bps,
        slippage_atr_frac: 0.0,
        instrument: asset_sym,
        step_hours,
        horizon_hours,
        n_bootstrap_resamples: 1000,
        block_size: None,
    };

    let init_nav = initial_nav_usd;
    let portfolio_provider: PortfolioProvider = Arc::new(move || PortfolioState {
        equity_usd: init_nav,
        realized_pnl_today_usd: 0.0,
        day_index: 0,
        open_positions: Default::default(),
        as_of: chrono::Utc::now(),
    });

    println!(
        "running {} arm(s) over {} cycle(s) / {} bar(s)…",
        arm_specs.len(),
        snapshots.len(),
        bars_vec.len()
    );
    let result = run_ab_compare(
        snapshots,
        bars_vec,
        arm_specs,
        cfg,
        registry,
        TraderParams::default(),
        portfolio_provider,
        &risk,
    )
    .await?;

    std::fs::write(&output, serde_json::to_vec_pretty(&result)?)?;
    println!("wrote {} arm result(s) → {}", result.arms.len(), output.display());
    Ok(())
}

/// Resolve bars input from either `--from`/`--to` (cache-backed) or
/// `--bars <path>` (JSON file). Exactly one path must be specified;
/// the other flags must be omitted. Returns the eval-shaped
/// `Vec<MarketBar>` regardless of source.
async fn load_bars_input(
    bars_path: Option<&PathBuf>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    granularity: &str,
    asset: AssetSymbol,
) -> anyhow::Result<Vec<MarketBar>> {
    let cache_window = matches!((from, to), (Some(_), Some(_)));
    let file_window = bars_path.is_some();

    match (cache_window, file_window) {
        (true, true) => anyhow::bail!(
            "--bars is mutually exclusive with --from/--to; pick one source"
        ),
        (false, false) => anyhow::bail!(
            "must supply either --bars <path> OR --from <date> + --to <date>"
        ),
        (true, false) => {
            // Cache-backed path. asset already validated by caller.
            let g = match granularity {
                "1h" => BarGranularity::Hour1,
                "1d" => BarGranularity::Day1,
                other => anyhow::bail!("granularity '{other}' not in v1 set {{1h,1d}}"),
            };
            // Safe: cache_window match arm guarantees both Some.
            let from = from.unwrap();
            let to = to.unwrap();
            let start = from
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow::anyhow!("invalid --from date"))?
                .and_utc();
            let end = to
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow::anyhow!("invalid --to date"))?
                .and_utc();
            if end <= start {
                anyhow::bail!("--to must be strictly after --from");
            }
            let asset_pair = asset.as_alpaca_pair();
            let data_source_tag = "alpaca-historical-v1";
            let cache_key = compute_cache_key(&asset_pair, g, start, end, data_source_tag);
            let ctx = open_api_ctx().await?;
            let cache_args = BarCacheArgs {
                cache_key,
                asset_pair,
                granularity: g,
                start,
                end,
                data_source_tag: data_source_tag.into(),
            };
            let upstream = load_bars(&ctx, &cache_args)
                .await
                .map_err(|e| anyhow::anyhow!("load bars: {e}"))?;
            // Convert `xvision_data::alpaca::MarketBar` → eval-shaped
            // `MarketBar`. Field set is identical; the two types live in
            // separate crates to keep `xvision-eval` independent of the
            // Alpaca data crate's transitive deps.
            Ok(upstream
                .into_iter()
                .map(|b| MarketBar {
                    timestamp: b.timestamp,
                    open: b.open,
                    high: b.high,
                    low: b.low,
                    close: b.close,
                    volume: b.volume,
                })
                .collect())
        }
        (false, true) => {
            // Legacy JSON-file path. Safe unwrap: file_window match arm
            // guarantees Some.
            let path = bars_path.unwrap();
            let raw = std::fs::read(path)
                .map_err(|e| anyhow::anyhow!("read bars {}: {e}", path.display()))?;
            serde_json::from_slice(&raw)
                .map_err(|e| anyhow::anyhow!("parse bars {}: {e}", path.display()))
        }
    }
}

/// Resolve `$XVN_HOME` (or `~/.xvn`) and open an `ApiContext` bound to
/// it. Mirrors `commands::bars::open_ctx` — duplicating the ~6 lines of
/// resolution avoids exposing `open_ctx` as a CLI-wide helper before
/// there's a third caller to warrant the API surface.
async fn open_api_ctx() -> anyhow::Result<ApiContext> {
    let xvn_home = if let Ok(p) = std::env::var("XVN_HOME") {
        PathBuf::from(p)
    } else {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("HOME not set; set XVN_HOME explicitly"))?
            .join(".xvn")
    };
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext at {}: {e}", xvn_home.display()))
}
