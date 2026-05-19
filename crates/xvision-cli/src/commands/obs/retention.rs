//! `xvn obs retention {show,set,clear}` — operator surface for the
//! agent-run retention policy.

use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Subcommand, ValueEnum};
use xvision_observability::{
    clear_config, default_config_path, resolve_retention, write_config, CliOverrides, ObservabilityConfig,
    RetentionMode,
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
    let path = args.config.clone().unwrap_or_else(default_config_path);
    // Seed from the file on disk only — never from the env-resolved
    // view. Otherwise a transient `XVISION_OBSERVABILITY_*` export in
    // the shell would get baked into `observability.toml` whenever the
    // operator runs an unrelated `set`. Missing file → start from
    // defaults; the CLI flags below overlay on top.
    let mut cfg: ObservabilityConfig =
        ObservabilityConfig::load_from_file(&path).with_context(|| format!("reading {}", path.display()))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Regression: `xvn obs retention set` used to seed from
    /// `resolve_retention`, which folds in any
    /// `XVISION_OBSERVABILITY_*` env override. That meant a transient
    /// shell export (e.g. `XVISION_OBSERVABILITY_RETENTION=full_debug`)
    /// would get baked into `observability.toml` on the next unrelated
    /// `set --payload-ttl-days 30`. After the fix, `set` reads the
    /// file directly so env vars do not influence what is persisted.
    ///
    /// The old version of this test asserted `persisted.mode ==
    /// HashOnly` against an empty file. That worked when
    /// `RetentionConfig::default()` was HashOnly. After the
    /// sibling track flipped the default to FullDebug (so fresh
    /// installs can debug from the first run), an empty file resolved
    /// to FullDebug regardless of env, making the test a no-op for the
    /// regression it was supposed to pin. This version pre-populates
    /// the file with an explicit non-default mode so the env-leakage
    /// check has signal again.
    #[test]
    fn set_does_not_persist_env_overrides() {
        const KEY: &str = "XVISION_OBSERVABILITY_RETENTION";

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("observability.toml");

        // Pre-populate the file with HashOnly so the env-vs-file
        // contrast is meaningful. If `set` ever folds env back in, the
        // env value (FullDebug) will overwrite the file's HashOnly when
        // we run a flag-only `set` below.
        {
            let mut seed = ObservabilityConfig::default();
            seed.retention.mode = RetentionMode::HashOnly;
            seed.retention.store_prompts = false;
            seed.retention.store_responses = false;
            seed.retention.store_tool_inputs = false;
            seed.retention.store_tool_outputs = false;
            write_config(&path, &seed).unwrap();
        }

        // Set the env var, run `set` with a CLI flag that does NOT
        // touch the mode, then immediately remove the env var so this
        // test does not leak state to siblings if run in parallel.
        let prior = std::env::var(KEY).ok();
        std::env::set_var(KEY, "full_debug");
        let result = set(SetArgs {
            config: Some(path.clone()),
            mode: None,
            store_prompts: None,
            store_responses: None,
            store_tool_inputs: None,
            store_tool_outputs: None,
            redact_secrets: None,
            payload_ttl_days: Some(30),
            max_payload_bytes: None,
            sqlite_enabled: None,
            otel_enabled: None,
        });
        match prior {
            Some(v) => std::env::set_var(KEY, v),
            None => std::env::remove_var(KEY),
        }
        result.expect("set should succeed");

        // Read the file back via load_from_file (no env application).
        let persisted = ObservabilityConfig::load_from_file(&path).unwrap();
        assert_eq!(
            persisted.retention.mode,
            RetentionMode::HashOnly,
            "env var must NOT have leaked into the persisted mode (file said HashOnly, env said FullDebug, file should win)"
        );
        assert_eq!(
            persisted.retention.payload_ttl_days, 30,
            "the explicitly-passed CLI flag should be the only thing the file changed to"
        );
    }
}
