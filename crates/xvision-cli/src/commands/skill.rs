//! `xvn skill ...` — author and attach OSShip-style markdown skills.

use std::env;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use tokio::io::AsyncReadExt;
use xvision_engine::bundle::store::{strategy_store_dir, BundleStore, FilesystemStore};
use xvision_skills::attach::attach_skill_to_agent;
use xvision_skills::parse;
use xvision_skills::store::{FilesystemSkillStore, SkillStore};

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

#[derive(Args, Debug)]
pub struct SkillCmd {
    #[command(subcommand)]
    action: SkillAction,
}

#[derive(Subcommand, Debug)]
enum SkillAction {
    /// Register (or overwrite) a skill from a markdown file. Pass `-` as
    /// the path to read from stdin (e.g. `cat my-trader.md | xvn skill new --from-file -`).
    New {
        /// Path to the skill markdown file. Use `-` for stdin.
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
        /// Print what would change to stdout and exit 0 without writing
        /// the bundle back to disk.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

pub async fn run(cmd: SkillCmd) -> CliResult<()> {
    match cmd.action {
        SkillAction::New { from_file } => new(from_file).await,
        SkillAction::Ls => ls().await,
        SkillAction::Attach {
            agent_id,
            slot,
            skill,
            dry_run,
        } => attach(&agent_id, &slot, &skill, dry_run).await,
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
    FilesystemStore::new(strategy_store_dir(&xvn_home()))
}

/// Read skill markdown from `from_file`. The literal path `-` reads
/// from stdin so callers can pipe LLM output straight in:
///
/// ```text
/// my-llm-tool render-skill | xvn skill new --from-file -
/// ```
async fn read_source(from_file: &Path) -> CliResult<String> {
    if from_file.as_os_str() == "-" {
        let mut buf = String::new();
        tokio::io::stdin()
            .read_to_string(&mut buf)
            .await
            .exit_with(XvnExit::Upstream)?;
        return Ok(buf);
    }
    tokio::fs::read_to_string(from_file)
        .await
        .exit_with(XvnExit::Usage) // file path came from the caller
}

async fn new(from_file: PathBuf) -> CliResult<()> {
    let markdown = read_source(&from_file).await?;
    let parsed = parse(&markdown).exit_with(XvnExit::Usage)?; // input is caller's
    skill_store()
        .save(&parsed.name, &markdown)
        .await
        .exit_with(XvnExit::Upstream)?; // disk write
    println!("{}", parsed.name);
    Ok(())
}

async fn ls() -> CliResult<()> {
    for name in skill_store().list().await.exit_with(XvnExit::Upstream)? {
        println!("{name}");
    }
    Ok(())
}

async fn attach(
    agent_id: &str,
    slot: &str,
    skill_name: &str,
    dry_run: bool,
) -> CliResult<()> {
    let strategies = strategy_store();
    let mut bundle = strategies
        .load(agent_id)
        .await
        .exit_with(XvnExit::NotFound)?; // missing strategy id
    let skill = skill_store()
        .load(skill_name)
        .await
        .exit_with(XvnExit::NotFound)?; // missing skill name

    // Snapshot the slot's BEFORE state so dry-run can render a diff. We do
    // this before the mutate call so we don't have to re-load.
    let before = match slot {
        "regime" => bundle.regime_slot.clone(),
        "intern" => bundle.intern_slot.clone(),
        "trader" => bundle.trader_slot.clone(),
        _ => None, // attach_skill_to_agent below will surface the Usage error
    };

    // attach_skill_to_agent returns anyhow::Error with two distinct messages:
    // "unknown slot role: ..." (caller typo → Usage)
    // "slot 'X' is empty — fill it before attaching" (state conflict → Conflict)
    if let Err(e) = attach_skill_to_agent(&mut bundle, slot, &skill) {
        let msg = e.to_string();
        let exit = if msg.contains("unknown slot role") {
            XvnExit::Usage
        } else {
            XvnExit::Conflict
        };
        return Err(CliError { exit, source: e });
    }

    if dry_run {
        // Pull the AFTER slot from the mutated bundle for the diff.
        let after = match slot {
            "regime" => bundle.regime_slot.as_ref(),
            "intern" => bundle.intern_slot.as_ref(),
            "trader" => bundle.trader_slot.as_ref(),
            _ => None,
        }
        .expect("attach_skill_to_agent returned Ok so the slot is populated");
        let diff = serde_json::json!({
            "agent_id": agent_id,
            "slot": slot,
            "skill_name": skill_name,
            "dry_run": true,
            "would_change": {
                "before": before,
                "after": after,
            },
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&diff).exit_with(XvnExit::Upstream)?
        );
        return Ok(());
    }

    strategies
        .save(&bundle)
        .await
        .exit_with(XvnExit::Upstream)?;
    println!("attached {skill_name} → {agent_id}#{slot}");
    Ok(())
}
