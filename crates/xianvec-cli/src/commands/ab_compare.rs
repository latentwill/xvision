//! `xvn ab-compare` — N-arm backtest A/B runner. Phase 9.1 + 9.2.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::{AssetSymbol, PortfolioState};
use xianvec_eval::ab_compare::{parse_arm_spec, run_ab_compare};
use xianvec_eval::backtest::MarketBar;
use xianvec_eval::baselines::PortfolioProvider;
use xianvec_eval::harness::BacktestRunConfig;
use xianvec_inference::engine::Qwen3Engine;
use xianvec_intern::backend::{AnthropicIntern, InternBackend, OpenAICompatIntern};
use xianvec_trader::TraderParams;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    setups: PathBuf,
    bars: PathBuf,
    arms: String,
    output: PathBuf,
    initial_nav_usd: f64,
    fee_bps: u32,
    step_hours: u32,
    horizon_hours: u32,
    asset: String,
    model: PathBuf,
    tokenizer: PathBuf,
    intern_provider: String,
    intern_model: String,
) -> anyhow::Result<()> {
    let snapshots: Vec<MarketSnapshot> = serde_json::from_slice(&std::fs::read(&setups)?)?;
    let bars_vec: Vec<MarketBar> = serde_json::from_slice(&std::fs::read(&bars)?)?;
    let arm_specs: Vec<_> = arms
        .split(',')
        .map(|s| parse_arm_spec(s.trim()))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let asset_sym = match asset.as_str() {
        "BTC" => AssetSymbol::Btc,
        "ETH" => AssetSymbol::Eth,
        "SOL" => AssetSymbol::Sol,
        other => anyhow::bail!("unknown asset: {other}"),
    };

    println!("loading Qwen3 weights from {}…", model.display());
    let device = Qwen3Engine::pick_device()?;
    let engine = Qwen3Engine::load(&model, &tokenizer, device)?;
    let engine = Arc::new(Mutex::new(engine));

    let intern: Arc<dyn InternBackend> = match intern_provider.as_str() {
        "anthropic" => Arc::new(AnthropicIntern::from_env(
            "https://api.anthropic.com",
            &intern_model,
            "ANTHROPIC_API_KEY",
        )?),
        "openai-compat" => Arc::new(OpenAICompatIntern::from_env(
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            &intern_model,
            "OPENAI_API_KEY",
        )?),
        other => anyhow::bail!("unknown intern provider: {other}"),
    };

    let workspace_root = std::env::current_dir()?;
    let risk = xianvec_harness::load_risk_layer(&workspace_root)?;

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
        "running {} arm(s) over {} setup(s) / {} bar(s)…",
        arm_specs.len(),
        snapshots.len(),
        bars_vec.len()
    );
    let result = run_ab_compare(
        snapshots,
        bars_vec,
        arm_specs,
        cfg,
        intern,
        intern_provider,
        intern_model,
        engine,
        TraderParams::default(),
        portfolio_provider,
        &risk,
    )
    .await?;

    std::fs::write(&output, serde_json::to_vec_pretty(&result)?)?;
    println!(
        "wrote {} arm result(s) → {}",
        result.arms.len(),
        output.display()
    );
    Ok(())
}
