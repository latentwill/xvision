//! Phase 0.2 smoke test — load Qwen3-32B Q4 GGUF + tokenizer, generate 32
//! tokens, print timings. Pass criteria (implementation-plan.md §0.2):
//!   - Load + first-token completes in finite time
//!   - Output is coherent (visual check; we just print)
//!   - Memory footprint visible via `top` during the run
//!
//! Acceptance from the plan calls for <10s and <12 GB on a 27B model — at 32B
//! Q4 on M4 Max we expect ~20–40 GB resident and a few seconds for the first
//! token. The numbers are reported, not asserted.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use xianvec_inference::engine::{GenerateOpts, Qwen3Engine};

const DEFAULT_GGUF: &str = "models/qwen3-32b-q4-gguf/Qwen_Qwen3-32B-Q4_K_M.gguf";
const DEFAULT_TOKENIZER: &str = "models/qwen3-32b-mlx-4bit/tokenizer.json";

fn env_path(var: &str, default: &str) -> PathBuf {
    std::env::var(var).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(default))
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let gguf = env_path("XVN_GGUF", DEFAULT_GGUF);
    let tokenizer = env_path("XVN_TOKENIZER", DEFAULT_TOKENIZER);

    println!("loading {gguf:?}");
    println!("tokenizer {tokenizer:?}");

    let device = Qwen3Engine::pick_device()?;
    let load_start = Instant::now();
    let mut engine = Qwen3Engine::load(&gguf, &tokenizer, device)?;
    println!("model loaded in {:.2}s", load_start.elapsed().as_secs_f32());

    let prompt = std::env::var("XVN_PROMPT").unwrap_or_else(|_| {
        "You are a market analyst. The trading signal is".to_string()
    });
    println!("\nprompt: {prompt}");

    let opts = GenerateOpts {
        max_tokens: std::env::var("XVN_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(48),
        temperature: 0.0,
        ..Default::default()
    };
    let result = engine.generate(&prompt, &opts)?;

    println!("\n--- output ---");
    println!("{}", result.text);
    println!("--- /output ---");
    println!(
        "\nprompt_tokens={}  completion_tokens={}  prompt_dt_ms={}  completion_dt_ms={}  tps={:.2}",
        result.prompt_tokens,
        result.completion_tokens,
        result.prompt_dt_ms,
        result.completion_dt_ms,
        result.completion_tokens as f64 * 1000.0 / result.completion_dt_ms.max(1) as f64,
    );

    Ok(())
}
