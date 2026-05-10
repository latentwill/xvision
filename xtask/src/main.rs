//! Workspace task runner. Today: Rust → TypeScript codegen for the engine API.
//!
//! Usage:
//!   cargo xtask gen-types

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use xshell::{cmd, Shell};

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "xvision workspace tasks")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Regenerate `frontend/web/src/api/types.gen/` from `xvision-engine` API
    /// types decorated with `#[ts(export)]`. Also rewrites the barrel
    /// `frontend/web/src/api/types.gen.ts` to re-export every emitted type.
    GenTypes,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let sh = Shell::new()?;
    sh.change_dir(workspace_root()?);
    match cli.cmd {
        Cmd::GenTypes => gen_types(&sh),
    }
}

fn workspace_root() -> Result<PathBuf> {
    // CARGO_MANIFEST_DIR points at xtask/; the workspace root is the parent.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .context("CARGO_MANIFEST_DIR unset; run via `cargo xtask`")?;
    PathBuf::from(manifest_dir)
        .parent()
        .map(|p| p.to_path_buf())
        .context("xtask manifest has no parent dir")
}

fn gen_types(sh: &Shell) -> Result<()> {
    let out_dir = "frontend/web/src/api/types.gen";

    // Wipe first so deletions in Rust propagate (ts-rs only writes, never deletes).
    cmd!(sh, "rm -rf {out_dir}").run().ok();

    // ts-rs emits files as the side effect of running unit tests under the
    // `ts-export` feature. `--lib` because the auto-generated test functions
    // live alongside the type definitions.
    cmd!(
        sh,
        "cargo test -p xvision-engine --features ts-export --lib --quiet"
    )
    .run()
    .context("ts-rs export run failed")?;

    let entries: Vec<_> = std::fs::read_dir(out_dir)
        .with_context(|| format!("read {out_dir}"))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|x| x == "ts").unwrap_or(false)
                && e.file_name() != "index.ts"
        })
        .collect();

    let mut stems: Vec<_> = entries
        .iter()
        .map(|e| {
            e.path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    stems.sort();

    let barrel = stems
        .iter()
        .map(|s| format!("export type {{ {s} }} from \"./types.gen/{s}\";"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    std::fs::write("frontend/web/src/api/types.gen.ts", barrel)?;
    println!("wrote {} type exports", stems.len());
    Ok(())
}
