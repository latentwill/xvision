//! Phase 3 Task 3.3 smoke — Stage 1 (Intern) → Stage 2 (Trader), end-to-end,
//! vectors disabled. Exercises the cache-pair contract by running the Trader
//! against the SAME briefing it would see in a paired-arms backtest.
//!
//! Modes:
//!   - With `ANTHROPIC_API_KEY` set, calls Claude as the Intern backend.
//!   - Otherwise prints a warning and uses a hand-rolled fixture briefing
//!     (still exercises the Trader fully against the live GGUF).
//!
//! Env knobs:
//!   ANTHROPIC_API_KEY     enable real Intern call
//!   XVN_INTERN_MODEL      default: claude-haiku-4-5
//!   XVN_GGUF              path to Qwen3 Q4 GGUF
//!   XVN_TOKENIZER         path to tokenizer.json
//!   XVN_MAX_TOKENS        Trader generation cap (default 384)

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use xianvec_core::market::{IndicatorPanel, MarketSnapshot, Ohlcv, OnchainPanel, SkillRef};
use xianvec_core::trading::{AssetSymbol, EvidenceTag, InternBriefing, PortfolioState, Regime};
use xianvec_inference::engine::Qwen3Engine;
use xianvec_intern::{
    backend::{AnthropicIntern, InternBackend},
    build_intern_prompt, PromptOpts,
};
use xianvec_trader::{run_trader, TraderParams};

const DEFAULT_GGUF: &str = "models/qwen3-32b-q4-gguf/Qwen_Qwen3-32B-Q4_K_M.gguf";
const DEFAULT_TOKENIZER: &str = "models/qwen3-32b-mlx-4bit/tokenizer.json";

fn env_path(var: &str, default: &str) -> PathBuf {
    std::env::var(var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

fn fixture_snapshot(setup_id: Uuid) -> MarketSnapshot {
    let now = Utc::now();
    MarketSnapshot {
        setup_id,
        asset: AssetSymbol::Btc,
        timestamp: now,
        price: 70_123.45,
        volume_24h: Some(28_000_000_000.0),
        recent_bars: (0..6)
            .map(|i| Ohlcv {
                timestamp: now - chrono::Duration::hours(6 - i),
                open: 70_000.0 + i as f64 * 80.0,
                high: 70_400.0 + i as f64 * 80.0,
                low: 69_800.0 + i as f64 * 80.0,
                close: 70_100.0 + i as f64 * 80.0,
                volume: 1_000_000.0,
            })
            .collect(),
        indicators: IndicatorPanel {
            rsi_14: Some(54.3),
            sma_20: Some(69_500.0),
            sma_50: Some(69_000.0),
            atr_14: Some(420.0),
            bb_upper: Some(71_500.0),
            bb_middle: Some(70_000.0),
            bb_lower: Some(68_500.0),
            ..Default::default()
        },
        onchain: OnchainPanel {
            funding_rate_8h: Some(0.000_05),
            open_interest_usd: Some(9_200_000_000.0),
            long_short_ratio: Some(1.05),
            realized_volatility_30d: Some(0.45),
            ..Default::default()
        },
        regime: Regime::Chop,
        horizon_hours: 24,
    }
}

fn fixture_skills() -> Vec<SkillRef> {
    vec![SkillRef {
        catalog: "byreal".into(),
        name: "perp-risk-shapes".into(),
        summary: "Drawdown, funding skew, liquidation cascades.".into(),
    }]
}

fn fallback_briefing(setup_id: Uuid) -> InternBriefing {
    InternBriefing {
        setup_id,
        asset: AssetSymbol::Btc,
        bull_case: "Funding compressed near zero with declining OI — leverage flushing without \
                    spot follow-through. Smart-money inflow trending up over 8 sessions, \
                    suggesting accumulation under cover of chop."
            .into(),
        bear_case: "Realized 30d vol expanding into prior squeeze level; long-skew funding \
                    rebuilding from bottom. Donchian-20 lower has been tagged twice in 24h \
                    without rejection."
            .into(),
        flat_case: "Range-bound between SMA20 and SMA50 with declining ATR. RSI in mid-band; \
                    no clean trigger either side. Wait for directional break with volume."
            .into(),
        evidence_long: vec![EvidenceTag::Onchain("smart_money_inflow".into())],
        evidence_short: vec![EvidenceTag::Technical("vol_expansion".into())],
        evidence_flat: vec![EvidenceTag::Technical("range_bound".into())],
        regime: Regime::Chop,
        signal_quality: 0.55,
        horizon_hours: 24,
        created_at: Utc::now(),
    }
}

fn fixture_portfolio() -> PortfolioState {
    PortfolioState {
        equity_usd: 100_000.0,
        realized_pnl_today_usd: 0.0,
        day_index: 0,
        open_positions: BTreeMap::new(),
        as_of: Utc::now(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let setup_id = Uuid::new_v4();
    println!("setup_id: {setup_id}");

    let briefing = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        println!("\n[stage 1] Intern via Anthropic");
        let snapshot = fixture_snapshot(setup_id);
        let skills = fixture_skills();
        let prompt = build_intern_prompt(&snapshot, &skills, &PromptOpts::default());

        let model = std::env::var("XVN_INTERN_MODEL").unwrap_or_else(|_| "claude-haiku-4-5".to_string());
        let intern = AnthropicIntern::from_env("https://api.anthropic.com", model, "ANTHROPIC_API_KEY")
            .context("constructing AnthropicIntern")?;

        let intern_start = Instant::now();
        let briefing = intern
            .brief(
                &prompt,
                setup_id,
                snapshot.asset,
                snapshot.regime,
                snapshot.horizon_hours,
            )
            .await
            .context("Intern brief() call")?;
        println!(
            "intern produced briefing in {:.2}s",
            intern_start.elapsed().as_secs_f32()
        );
        briefing
    } else {
        eprintln!(
            "[warn] ANTHROPIC_API_KEY not set — using fallback fixture briefing instead. \
             Trader path is still exercised against the live GGUF."
        );
        fallback_briefing(setup_id)
    };

    println!("\n--- briefing ---");
    println!("{}", serde_json::to_string_pretty(&briefing)?);
    println!("--- /briefing ---");

    println!("\n[stage 2] Trader via local Qwen3 GGUF");
    let gguf = env_path("XVN_GGUF", DEFAULT_GGUF);
    let tokenizer = env_path("XVN_TOKENIZER", DEFAULT_TOKENIZER);
    let device = Qwen3Engine::pick_device()?;
    let load_start = Instant::now();
    let mut engine = Qwen3Engine::load(&gguf, &tokenizer, device).context("engine load")?;
    println!("model loaded in {:.2}s", load_start.elapsed().as_secs_f32());

    let params = TraderParams {
        max_tokens: std::env::var("XVN_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(384),
        ..TraderParams::default()
    };

    let portfolio = fixture_portfolio();
    let run_start = Instant::now();
    let decision = run_trader(&mut engine, &briefing, &portfolio, &params).context("run_trader")?;
    let dt_ms = run_start.elapsed().as_millis();

    println!("\n--- trader decision ---");
    println!("{}", serde_json::to_string_pretty(&decision)?);
    println!("--- /trader decision ---");
    println!("\nstage 2 took {dt_ms} ms");

    Ok(())
}
