//! `xvn strategy ...` — strategy authoring subcommands.
//!
//! All mutations route through `xianvec_engine`: templates, bundle types,
//! filesystem store. Real handlers land in Task 18+; T17 is the skeleton.

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct StrategyCmd {
    #[command(subcommand)]
    action: StrategyAction,
}

#[derive(Subcommand, Debug)]
enum StrategyAction {
    /// Create a new strategy draft from a template.
    New {
        /// Template name (e.g., "mean_reversion").
        #[arg(long)]
        template: String,
        /// Human-readable name.
        #[arg(long)]
        name: String,
        /// Creator handle (default: $XVN_CREATOR or "@anonymous").
        #[arg(long)]
        creator: Option<String>,
    },
    /// Validate a saved strategy bundle by id.
    Validate { id: String },
    /// List all saved strategy ids.
    Ls,
    /// Show a saved strategy bundle as JSON.
    Show { id: String },
}

pub async fn run(cmd: StrategyCmd) -> anyhow::Result<()> {
    match cmd.action {
        StrategyAction::New { template, name, creator } => new(&template, &name, creator).await,
        StrategyAction::Validate { id } => validate(&id).await,
        StrategyAction::Ls => ls().await,
        StrategyAction::Show { id } => show(&id).await,
    }
}

async fn new(_template: &str, _name: &str, _creator: Option<String>) -> anyhow::Result<()> {
    anyhow::bail!("not implemented yet — Task 18")
}
async fn validate(_id: &str) -> anyhow::Result<()> { anyhow::bail!("not implemented yet — Task 18") }
async fn ls() -> anyhow::Result<()> { anyhow::bail!("not implemented yet — Task 18") }
async fn show(_id: &str) -> anyhow::Result<()> { anyhow::bail!("not implemented yet — Task 18") }
