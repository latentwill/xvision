//! `xvn intern …` — run Stage 1 in isolation.
//!
//! - `brief`   — call the Intern backend, print `InternBriefing` JSON.
//! - `preview` — render the prompt without calling the backend.
//!
//! Provider strings: `anthropic` | `openai-compat`.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use xianvec_core::market::MarketSnapshot;
use xianvec_intern::backend::{AnthropicIntern, InternBackend, OpenAICompatIntern};
use xianvec_intern::prompt::{build_intern_prompt, PromptOpts};

#[derive(Args, Debug)]
pub struct InternCmd {
    #[command(subcommand)]
    action: InternAction,
}

#[derive(Subcommand, Debug)]
enum InternAction {
    /// Render the intern prompt for a snapshot without calling any backend.
    Preview {
        #[arg(long)]
        snapshot: PathBuf,
    },
    /// Call the intern backend and print the `InternBriefing` as JSON.
    Brief {
        #[arg(long)]
        snapshot: PathBuf,
        /// `anthropic` | `openai-compat`.
        #[arg(long, default_value = "anthropic")]
        intern: String,
        #[arg(long, default_value = "claude-haiku-4-5-20251001")]
        model: String,
    },
}

pub async fn run(cmd: InternCmd) -> anyhow::Result<()> {
    match cmd.action {
        InternAction::Preview { snapshot } => preview(snapshot).await,
        InternAction::Brief {
            snapshot,
            intern,
            model,
        } => brief(snapshot, intern, model).await,
    }
}

async fn preview(snapshot: PathBuf) -> anyhow::Result<()> {
    let snap: MarketSnapshot = serde_json::from_slice(&std::fs::read(&snapshot)?)?;
    let prompt = build_intern_prompt(&snap, &[], &PromptOpts::default());
    println!("{prompt}");
    Ok(())
}

async fn brief(snapshot: PathBuf, intern_provider: String, model: String) -> anyhow::Result<()> {
    let snap: MarketSnapshot = serde_json::from_slice(&std::fs::read(&snapshot)?)?;
    let prompt = build_intern_prompt(&snap, &[], &PromptOpts::default());

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
    println!("{}", serde_json::to_string_pretty(&briefing)?);
    Ok(())
}
