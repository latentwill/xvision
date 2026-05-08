//! `xvn strategy ...` — strategy authoring subcommands.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, Subcommand};
use ulid::Ulid;
use xianvec_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch};
use xianvec_engine::agent::pipeline::{run_pipeline, PipelineInputs};
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_engine::bundle::validate::validate_bundle;
use xianvec_engine::templates::registry;
use xianvec_engine::tokens::estimate_pipeline_tokens;
use xianvec_engine::tools::ToolRegistry;

#[derive(Args, Debug)]
pub struct StrategyCmd {
    #[command(subcommand)]
    action: StrategyAction,
}

#[derive(Subcommand, Debug)]
enum StrategyAction {
    /// Create a new strategy draft from a template.
    New {
        #[arg(long)]
        template: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        creator: Option<String>,
    },
    /// Validate a saved strategy bundle by id.
    Validate { id: String },
    /// List all saved strategy ids.
    Ls,
    /// Show a saved strategy bundle as JSON.
    Show { id: String },
    /// List available strategy templates.
    Templates,
    /// Run a saved strategy inline against a fixture (decision_points iterations).
    Run {
        /// Strategy id (ULID) returned from `xvn strategy new`.
        id: String,
        /// Fixture parquet name under data/probes/ (without .parquet).
        #[arg(long)]
        fixture: String,
        /// How many decision points to simulate (>=1).
        #[arg(long, default_value_t = 1)]
        decisions: u32,
        /// Use the deterministic mock LLM dispatch (no API calls).
        #[arg(long, default_value_t = false)]
        mock: bool,
    },
}

pub async fn run(cmd: StrategyCmd) -> anyhow::Result<()> {
    match cmd.action {
        StrategyAction::New { template, name, creator } => new(&template, &name, creator).await,
        StrategyAction::Validate { id } => validate(&id).await,
        StrategyAction::Ls => ls().await,
        StrategyAction::Show { id } => show(&id).await,
        StrategyAction::Templates => templates().await,
        StrategyAction::Run { id, fixture, decisions, mock } =>
            run_inline(&id, &fixture, decisions, mock).await,
    }
}

fn home() -> PathBuf {
    if let Ok(p) = env::var("XVN_HOME") {
        return PathBuf::from(p);
    }
    let h = dirs::home_dir().expect("$HOME");
    h.join(".xvn")
}

fn store() -> FilesystemStore {
    FilesystemStore::new(home().join("strategies"))
}

async fn new(template: &str, name: &str, creator: Option<String>) -> anyhow::Result<()> {
    let tpl = registry::get(template)
        .ok_or_else(|| anyhow::anyhow!("unknown template '{template}' — try `xvn strategy templates`"))?;
    let id = Ulid::new().to_string();
    let creator = creator
        .or_else(|| env::var("XVN_CREATOR").ok())
        .unwrap_or_else(|| "@anonymous".to_string());
    let draft = tpl.new_draft(id.clone(), name.to_string(), creator);
    validate_bundle(&draft)?;
    store().save(&draft).await?;
    println!("{id}");
    Ok(())
}

async fn validate(id: &str) -> anyhow::Result<()> {
    let bundle = store().load(id).await?;
    validate_bundle(&bundle)?;
    println!("ok");
    Ok(())
}

async fn ls() -> anyhow::Result<()> {
    let ids = store().list().await?;
    for id in ids {
        println!("{id}");
    }
    Ok(())
}

async fn show(id: &str) -> anyhow::Result<()> {
    let bundle = store().load(id).await?;
    let json = serde_json::to_string_pretty(&bundle)?;
    println!("{json}");
    Ok(())
}

async fn templates() -> anyhow::Result<()> {
    let names = registry::list_template_names();
    for name in names {
        if let Some(tpl) = registry::get(&name) {
            println!("{:<20} {}", name, tpl.display_name());
        }
    }
    Ok(())
}

async fn run_inline(id: &str, fixture: &str, decisions: u32, mock: bool) -> anyhow::Result<()> {
    let bundle = store().load(id).await?;
    let est = estimate_pipeline_tokens(&bundle, decisions as u64);
    println!(
        "estimate: input={} output={} total={} (decisions={})",
        est.input, est.output, est.total, decisions
    );

    let dispatch: Arc<dyn LlmDispatch> = if mock {
        Arc::new(MockDispatch::echo(r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#))
    } else {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("set ANTHROPIC_API_KEY or pass --mock"))?;
        Arc::new(AnthropicDispatch::new(key))
    };
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let asset = bundle.manifest.asset_universe.first().cloned()
        .ok_or_else(|| anyhow::anyhow!("bundle has empty asset_universe"))?;
    let mut total_in = 0u32;
    let mut total_out = 0u32;
    for n in 0..decisions {
        let seed = serde_json::json!({
            "decision_index": n,
            "asset": asset,
            "fixture": fixture,
            "ohlcv_history": "<fetch via tool — Plan #2 wires this>",
            "indicator_panel": "<fetch via tool — Plan #2 wires this>",
        });
        let outs = run_pipeline(PipelineInputs {
            bundle: &bundle,
            seed_inputs: seed,
            dispatch: dispatch.clone(),
            tools: tools.clone(),
        })
        .await?;
        total_in += outs.total_input_tokens;
        total_out += outs.total_output_tokens;
    }
    println!(
        "decisions: {} input_tokens: {} output_tokens: {}",
        decisions, total_in, total_out
    );
    Ok(())
}
