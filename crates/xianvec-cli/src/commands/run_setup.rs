//! `xvn run-setup` — run a single setup through Intern → Risk slice.
//!
//! v1 covers Stage 1 (Intern HTTP call) and Stage 3 (Risk layer over a
//! placeholder decision). Stage 2 (the real Trader call) is now a vanilla
//! HTTP backend (`xianvec_trader::OpenAiCompatBackend`) — wire it in here
//! when the demo narrative requires it.

use std::path::PathBuf;

use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::{
    Action, AssetSymbol, Direction, PortfolioState, TraderDecision,
};
use xianvec_intern::backend::{AcpxIntern, AnthropicIntern, InternBackend, OpenAICompatIntern};
use xianvec_intern::prompt::{build_intern_prompt, PromptOpts};

pub async fn run(snapshot_path: PathBuf, intern_provider: String, model: String) -> anyhow::Result<()> {
    let bytes = std::fs::read(&snapshot_path)?;
    let snap: MarketSnapshot = serde_json::from_slice(&bytes)?;

    println!("=== Stage 1: Intern ({intern_provider} / {model}) ===");
    let prompt = build_intern_prompt(&snap, &[], &PromptOpts::default());
    println!("(prompt {} chars)", prompt.len());

    // ACPX provider can specify the underlying agent inline as `acpx:<agent>`
    // (e.g. `acpx:codex`, `acpx:claude`) OR by setting `XVN_INTERN_ACPX_AGENT`.
    // The `model` arg is treated as documentation when the provider is acpx —
    // the actual model is whatever the harness has configured.
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
        p if p == "acpx" || p.starts_with("acpx:") => {
            let agent = p
                .strip_prefix("acpx:")
                .map(str::to_string)
                .or_else(|| std::env::var("XVN_INTERN_ACPX_AGENT").ok())
                .ok_or_else(|| anyhow::anyhow!(
                    "acpx provider requires an agent: pass `acpx:<agent>` or set XVN_INTERN_ACPX_AGENT"
                ))?;
            Box::new(AcpxIntern::from_env(agent)?)
        }
        other => anyhow::bail!("unknown intern provider: {other}"),
    };

    let briefing = intern
        .brief(
            &prompt,
            snap.setup_id,
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
    println!("=== Stage 2: Trader (skipped — wire xianvec_trader::OpenAiCompatBackend here when needed) ===");

    println!();
    println!("=== Stage 3: Risk ===");
    let workspace_root = std::env::current_dir()?;
    let risk = xianvec_harness::load_risk_layer(&workspace_root)?;
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
        setup_id: snap.setup_id,
        action: Action::Buy,
        size_bps: 500,
        direction: Direction::Long,
        stop_loss_pct: 2.0,
        take_profit_pct: 5.0,
        trader_summary: "run-setup placeholder decision (Trader path not invoked here).".into(),
    };
    let risk_outcome = xianvec_harness::apply_risk(placeholder, &portfolio, AssetSymbol::Btc, &risk);
    println!("risk verdict: {risk_outcome:?}");
    Ok(())
}
