//! `xvn obs retention {show,set,clear}` — operator surface for the
//! agent-run retention policy.

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use xvision_observability::{
    clear_config, default_config_path, resolve_retention, write_config, CliOverrides,
    ObservabilityConfig, RetentionMode,
};

#[derive(Args, Debug)]
pub struct RetentionCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Print the resolved retention policy with provenance per toggle.
    Show(ShowArgs),
    /// Write retention overrides to observability.toml.
    Set(SetArgs),
    /// Delete observability.toml so defaults take over.
    Clear(ClearArgs),
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Path to observability.toml. Defaults to
    /// `$XVN_HOME/config/observability.toml`.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Emit JSON instead of the human-readable table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct SetArgs {
    /// Path to observability.toml. Defaults to
    /// `$XVN_HOME/config/observability.toml`.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Retention mode. `full_debug` stores raw prompts/responses on disk
    /// and surfaces a banner in the dashboard.
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
    #[arg(long)]
    pub store_prompts: Option<bool>,
    #[arg(long)]
    pub store_responses: Option<bool>,
    #[arg(long)]
    pub store_tool_inputs: Option<bool>,
    #[arg(long)]
    pub store_tool_outputs: Option<bool>,
    #[arg(long)]
    pub redact_secrets: Option<bool>,
    #[arg(long)]
    pub payload_ttl_days: Option<u64>,
    #[arg(long)]
    pub max_payload_bytes: Option<u64>,
    #[arg(long)]
    pub sqlite_enabled: Option<bool>,
    #[arg(long)]
    pub otel_enabled: Option<bool>,
}

#[derive(Args, Debug)]
pub struct ClearArgs {
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ModeArg {
    HashOnly,
    Redacted,
    FullDebug,
}

impl From<ModeArg> for RetentionMode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::HashOnly => RetentionMode::HashOnly,
            ModeArg::Redacted => RetentionMode::Redacted,
            ModeArg::FullDebug => RetentionMode::FullDebug,
        }
    }
}

pub async fn run(cmd: RetentionCmd) -> anyhow::Result<()> {
    match cmd.op {
        Op::Show(args) => show(args),
        Op::Set(args) => set(args),
        Op::Clear(args) => clear(args),
    }
}

fn show(args: ShowArgs) -> anyhow::Result<()> {
    let path = args.config.unwrap_or_else(default_config_path);
    let view = resolve_retention(&path, &CliOverrides::default())?;
    if args.json {
        let v = serde_json::to_string_pretty(&view)?;
        println!("{v}");
    } else {
        print!("{}", view.to_table());
    }
    Ok(())
}

fn set(args: SetArgs) -> anyhow::Result<()> {
    let path = args
        .config
        .clone()
        .unwrap_or_else(default_config_path);
    // Start from the currently-resolved view (so unspecified knobs
    // survive the rewrite), then overlay the CLI overrides.
    let view = resolve_retention(&path, &CliOverrides::default())?;
    let mut cfg: ObservabilityConfig = view.config();

    if let Some(m) = args.mode {
        cfg.retention.mode = m.into();
    }
    if let Some(v) = args.store_prompts {
        cfg.retention.store_prompts = v;
    }
    if let Some(v) = args.store_responses {
        cfg.retention.store_responses = v;
    }
    if let Some(v) = args.store_tool_inputs {
        cfg.retention.store_tool_inputs = v;
    }
    if let Some(v) = args.store_tool_outputs {
        cfg.retention.store_tool_outputs = v;
    }
    if let Some(v) = args.redact_secrets {
        cfg.retention.redact_secrets = v;
    }
    if let Some(v) = args.payload_ttl_days {
        cfg.retention.payload_ttl_days = v;
    }
    if let Some(v) = args.max_payload_bytes {
        cfg.retention.max_payload_bytes = v;
    }
    if let Some(v) = args.sqlite_enabled {
        cfg.sqlite_enabled = v;
    }
    if let Some(v) = args.otel_enabled {
        cfg.otel_enabled = v;
    }

    write_config(&path, &cfg)?;
    eprintln!("wrote {}", path.display());
    if cfg.retention.mode == RetentionMode::FullDebug {
        eprintln!(
            "WARNING: full_debug retention enabled — raw prompts/responses \
             may land on disk. Disable for shared/client work."
        );
    }
    Ok(())
}

fn clear(args: ClearArgs) -> anyhow::Result<()> {
    let path = args.config.unwrap_or_else(default_config_path);
    let removed = clear_config(&path)?;
    if removed {
        eprintln!("removed {}", path.display());
    } else {
        eprintln!("no config file at {} (already cleared)", path.display());
    }
    Ok(())
}
