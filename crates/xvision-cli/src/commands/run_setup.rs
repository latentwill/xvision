//! `xvn run-setup` — run a single setup through Intern → Risk slice.
//!
//! v1 covers Stage 1 (Intern HTTP call) and Stage 3 (Risk layer over a
//! placeholder decision). Stage 2 (the real Trader call) is now a vanilla
//! HTTP backend (`xvision_trader::OpenAiCompatBackend`) — wire it in here
//! when the demo narrative requires it.

use std::path::PathBuf;

use xvision_core::market::MarketSnapshot;
use xvision_core::trading::{Action, AssetSymbol, Direction, PortfolioState, TraderDecision};
use xvision_intern::backend::{AnthropicIntern, InternBackend, OpenAICompatIntern};
use xvision_intern::prompt::{build_intern_prompt, PromptOpts};

pub async fn run(snapshot_path: PathBuf, intern_provider: String, model: String) -> anyhow::Result<()> {
    let bytes = std::fs::read(&snapshot_path)?;
    let snap: MarketSnapshot = serde_json::from_slice(&bytes)?;

    println!("=== Stage 1: Intern ({intern_provider} / {model}) ===");
    let prompt = build_intern_prompt(&snap, &[], &PromptOpts::default());
    println!("(prompt {} chars)", prompt.len());

    let intern: Box<dyn InternBackend> = match intern_provider.as_str() {
        "anthropic" => Box::new(AnthropicIntern::from_env(
            "https://api.anthropic.com",
            &model,
            "ANTHROPIC_API_KEY",
        )?),
        "openai-compat" => Box::new(OpenAICompatIntern::from_env(
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            &model,
            "OPENAI_API_KEY",
        )?),
        other => anyhow::bail!("unknown intern provider: {other}"),
    };

    let briefing = intern
        .brief(
            &prompt,
            snap.cycle_id,
            snap.asset,
            snap.regime,
            snap.horizon_hours,
        )
        .await?;
    println!("bull_case: {}", briefing.bull_case);
    println!("bear_case: {}", briefing.bear_case);
    println!("flat_case: {}", briefing.flat_case);
    println!("signal_quality: {:.3}", briefing.signal_quality);

    println!();
    println!("=== Stage 2: Trader (skipped — wire xvision_trader::OpenAiCompatBackend here when needed) ===");

    println!();
    println!("=== Stage 3: Risk ===");
    let workspace_root = std::env::current_dir()?;
    let risk = xvision_harness::load_risk_layer(&workspace_root)?;
    // v1 uses a placeholder decision so the risk layer can exercise its rules
    // without running the Trader. v1.1 plugs the real Trader call between
    // Stage 1 and Stage 3.
    let portfolio = PortfolioState {
        equity_usd: 100_000.0,
        realized_pnl_today_usd: 0.0,
        day_index: 0,
        open_positions: Default::default(),
        as_of: chrono::Utc::now(),
    };
    let placeholder = TraderDecision {
        cycle_id: snap.cycle_id,
        action: Action::Buy,
        size_bps: 500,
        direction: Direction::Long,
        stop_loss_pct: 2.0,
        take_profit_pct: 5.0,
        trader_summary: "run-setup placeholder decision (Trader path not invoked here).".into(),
        asset: None,
    };
    let risk_outcome = xvision_harness::apply_risk(placeholder, &portfolio, AssetSymbol::Btc, &risk);
    println!("risk verdict: {risk_outcome:?}");
    Ok(())
}
