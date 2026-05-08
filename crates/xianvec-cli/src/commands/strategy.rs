//! `xvn strategy ...` — strategy authoring subcommands.

use std::env;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use ulid::Ulid;
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_engine::bundle::validate::validate_bundle;
use xianvec_engine::templates::registry;

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
}

pub async fn run(cmd: StrategyCmd) -> anyhow::Result<()> {
    match cmd.action {
        StrategyAction::New { template, name, creator } => new(&template, &name, creator).await,
        StrategyAction::Validate { id } => validate(&id).await,
        StrategyAction::Ls => ls().await,
        StrategyAction::Show { id } => show(&id).await,
        StrategyAction::Templates => templates().await,
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
