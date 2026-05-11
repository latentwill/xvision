//! `xvn ab-compare` — N-arm backtest A/B runner.
//!
//! Each `trader_arm` may carry inline `intern=<provider>/<model>` and
//! `trader=<provider>/<model>` overrides; otherwise the global CLI flags
//! supply the defaults via the `ProviderRegistry`. Two arms that resolve
//! to the same `(provider, model)` share a backend `Arc` so they reuse one
//! HTTP client.

use std::path::PathBuf;
use std::sync::Arc;

use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_core::market::MarketSnapshot;
use xvision_core::slot::SlotRef;
use xvision_core::trading::{AssetSymbol, PortfolioState};
use xvision_eval::ab_compare::{auto_suffix_arm_names, default_arms, parse_arm_spec, run_ab_compare};
use xvision_eval::backtest::MarketBar;
use xvision_eval::baselines::PortfolioProvider;
use xvision_eval::harness::BacktestRunConfig;
use xvision_eval::provider_registry::ProviderRegistry;
use xvision_trader::TraderParams;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    cycles: PathBuf,
    bars: PathBuf,
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
    let snapshots: Vec<MarketSnapshot> = serde_json::from_slice(&std::fs::read(&cycles)?)?;
    let bars_vec: Vec<MarketBar> = serde_json::from_slice(&std::fs::read(&bars)?)?;
    let mut arm_specs: Vec<_> = if arms.trim().is_empty() {
        default_arms()
    } else {
        arms.split(',')
            .map(|s| parse_arm_spec(s.trim()))
            .collect::<anyhow::Result<Vec<_>>>()?
    };
    auto_suffix_arm_names(&mut arm_specs);

    let asset_sym = match asset.as_str() {
        "BTC" => AssetSymbol::Btc,
        "ETH" => AssetSymbol::Eth,
        "SOL" => AssetSymbol::Sol,
        other => anyhow::bail!("unknown asset: {other}"),
    };

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
