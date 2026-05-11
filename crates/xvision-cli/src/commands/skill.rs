//! `xvn skill ...` — author and attach OSShip-style markdown skills.

use std::env;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use xvision_engine::bundle::store::{BundleStore, FilesystemStore};
use xvision_skills::attach::attach_skill_to_agent;
use xvision_skills::parse;
use xvision_skills::store::{FilesystemSkillStore, SkillStore};

#[derive(Args, Debug)]
pub struct SkillCmd {
    #[command(subcommand)]
    action: SkillAction,
}

#[derive(Subcommand, Debug)]
enum SkillAction {
    /// Register (or overwrite) a skill from a markdown file.
    New {
        /// Path to the skill markdown file.
        #[arg(long)]
        from_file: PathBuf,
    },
    /// List skills saved under `$XVN_HOME/skills/`.
    Ls,
    /// Attach a skill to a slot (`regime` | `intern` | `trader`) of a saved
    /// strategy bundle. Replaces the slot prompt + model_requirement and
    /// unions the skill's allowed_tools.
    Attach {
        /// ULID of the strategy bundle to mutate.
        agent_id: String,
        #[arg(long)]
        slot: String,
        #[arg(long)]
        skill: String,
    },
}

pub async fn run(cmd: SkillCmd) -> anyhow::Result<()> {
    match cmd.action {
        SkillAction::New { from_file } => new(from_file).await,
        SkillAction::Ls => ls().await,
        SkillAction::Attach {
            agent_id,
            slot,
            skill,
        } => attach(&agent_id, &slot, &skill).await,
    }
}

fn xvn_home() -> PathBuf {
    if let Ok(p) = env::var("XVN_HOME") {
        return PathBuf::from(p);
    }
    dirs::home_dir().expect("$HOME").join(".xvn")
}

fn skill_store() -> FilesystemSkillStore {
    FilesystemSkillStore::new(xvn_home().join("skills"))
}

fn strategy_store() -> FilesystemStore {
    FilesystemStore::new(xvn_home().join("strategies"))
}

async fn new(from_file: PathBuf) -> anyhow::Result<()> {
    let markdown = tokio::fs::read_to_string(&from_file).await?;
    let parsed = parse(&markdown)?;
    skill_store().save(&parsed.name, &markdown).await?;
    println!("{}", parsed.name);
    Ok(())
}

async fn ls() -> anyhow::Result<()> {
    for name in skill_store().list().await? {
        println!("{name}");
    }
    Ok(())
}

async fn attach(agent_id: &str, slot: &str, skill_name: &str) -> anyhow::Result<()> {
    let strategies = strategy_store();
    let mut bundle = strategies.load(agent_id).await?;
    let skill = skill_store().load(skill_name).await?;
    attach_skill_to_agent(&mut bundle, slot, &skill)?;
    strategies.save(&bundle).await?;
    println!("attached {skill_name} → {agent_id}#{slot}");
    Ok(())
}
