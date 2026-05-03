//! Phase 3 Task 3.3 smoke — Trader-only, real Qwen3 GGUF, fixture briefing.
//!
//! Acceptance:
//!   - Engine loads from the configured GGUF + tokenizer
//!   - First-pass response parses into a valid `TraderDecision`
//!   - Timing printed; no assertions on tokens/sec (varies by hardware)
//!
//! Env knobs:
//!   XVN_GGUF        path to Qwen3 Q4 GGUF (default: models/qwen3-32b-q4-gguf/...)
//!   XVN_TOKENIZER   path to tokenizer.json (default: sibling MLX dir)
//!   XVN_MAX_TOKENS  generation cap (default 384)
//!   XVN_RETRY       1 to allow one corrective retry; 0 to fail loudly

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use xianvec_core::trading::{AssetSymbol, EvidenceTag, InternBriefing, PortfolioState, Regime};
use xianvec_inference::engine::Qwen3Engine;
use xianvec_trader::{run_trader, TraderParams};

const DEFAULT_GGUF: &str = "models/qwen3-32b-q4-gguf/Qwen_Qwen3-32B-Q4_K_M.gguf";
const DEFAULT_TOKENIZER: &str = "models/qwen3-32b-mlx-4bit/tokenizer.json";

fn env_path(var: &str, default: &str) -> PathBuf {
    std::env::var(var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

fn fixture_briefing() -> InternBriefing {
    InternBriefing {
        setup_id: Uuid::new_v4(),
        asset: AssetSymbol::Btc,
        bull_case: "Funding rate compressed near zero while spot OI declines — leverage \
                    flushing without spot follow-through. Smart-money inflow bucket trending \
                    up over the last 8 sessions, suggesting accumulation under cover of chop."
            .into(),
        bear_case: "Realized 30d vol expanding into prior squeeze level; long-skew funding \
                    rebuilding from bottom. Donchian-20 lower has been tagged twice in 24h \
                    without rejection — bears retain initiative on flush."
            .into(),
        flat_case: "Range-bound between SMA20 and SMA50 with declining ATR. RSI in mid-band; \
                    no clean trigger either side. Wait for directional break with volume."
            .into(),
        evidence_long: vec![
            EvidenceTag::Onchain("smart_money_inflow".into()),
            EvidenceTag::Technical("funding_compressed".into()),
        ],
        evidence_short: vec![
            EvidenceTag::Technical("vol_expansion".into()),
            EvidenceTag::Onchain("long_skew_rebuild".into()),
        ],
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

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let gguf = env_path("XVN_GGUF", DEFAULT_GGUF);
    let tokenizer = env_path("XVN_TOKENIZER", DEFAULT_TOKENIZER);

    println!("loading GGUF: {gguf:?}");
    println!("tokenizer:    {tokenizer:?}");

    let device = Qwen3Engine::pick_device()?;
    let load_start = Instant::now();
    let mut engine = Qwen3Engine::load(&gguf, &tokenizer, device).context("engine load")?;
    println!("model loaded in {:.2}s", load_start.elapsed().as_secs_f32());

    let params = TraderParams {
        max_tokens: std::env::var("XVN_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(384),
        retry_on_parse_fail: std::env::var("XVN_RETRY").map(|s| s != "0").unwrap_or(true),
        ..TraderParams::default()
    };

    let briefing = fixture_briefing();
    let portfolio = fixture_portfolio();

    println!("\n--- briefing setup_id ---");
    println!("{}", briefing.setup_id);

    let run_start = Instant::now();
    let decision = run_trader(&mut engine, &briefing, &portfolio, &params).context("run_trader")?;
    let dt_ms = run_start.elapsed().as_millis();

    println!("\n--- trader decision ---");
    println!("{}", serde_json::to_string_pretty(&decision)?);
    println!("--- /trader decision ---");
    println!("\nrun_trader took {dt_ms} ms");

    Ok(())
}
