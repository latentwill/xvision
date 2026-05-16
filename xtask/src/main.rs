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
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR unset; run via `cargo xtask`")?;
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
    //
    // We run the export test in both crates because ts-rs registers its
    // export hook inside the crate where the type is defined: types like
    // `LlmRequest` live in `xvision-engine`, while `Catalog`/`ModelEntry`
    // live in `xvision-core`. Skipping core would silently drop those
    // bindings.
    //
    // We deliberately use `.ok()` rather than `?` for the run() result.
    // ts-rs writes its `.ts` file from each test's body BEFORE assertions
    // fire, so an unrelated failing test elsewhere in the same package
    // doesn't prevent the type emissions we care about. If we hard-failed
    // here, every flaky test in the workspace would block barrel
    // regeneration — exactly the bug that left `Catalog`/`ModelEntry`
    // out of `types.gen.ts` after they were merged.
    //
    // The "tests already wrote the file" rationale collapses when the
    // package fails to compile — no test runs, no file is written. After
    // each invocation we therefore require: either the command exited 0,
    // or it produced at least one new .ts file. A failing compile that
    // emits nothing fails the task loudly instead of leaving the barrel
    // silently incomplete.
    run_ts_export(sh, "xvision-core", out_dir)?;
    run_ts_export(sh, "xvision-engine", out_dir)?;

    let entries: Vec<_> = std::fs::read_dir(out_dir)
        .with_context(|| format!("read {out_dir}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "ts").unwrap_or(false) && e.file_name() != "index.ts")
        .collect();

    let mut stems: Vec<_> = entries
        .iter()
        .map(|e| e.path().file_stem().unwrap().to_string_lossy().into_owned())
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

/// Run `cargo test --features ts-export --lib` for a single crate and require
/// that *something* came out of it. A compile failure that emits nothing fails
/// the task loudly so a later barrel rebuild can't paper over a missing crate's
/// types. A test-assertion failure after files were already written is treated
/// as benign — ts-rs writes its `.ts` files from the test body before any
/// assertion fires, so the bindings we care about are already on disk.
fn run_ts_export(sh: &Shell, pkg: &str, out_dir: &str) -> Result<()> {
    let before = count_ts_files(out_dir);
    let result = cmd!(
        sh,
        "cargo test -p {pkg} --features ts-export --lib --quiet"
    )
    .run();
    let after = count_ts_files(out_dir);
    if result.is_err() && after <= before {
        anyhow::bail!(
            "cargo test -p {pkg} --features ts-export failed and produced no new .ts files \
             (likely a compile error). Run that command manually to see the failure."
        );
    }
    Ok(())
}

fn count_ts_files(out_dir: &str) -> usize {
    match std::fs::read_dir(out_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "ts").unwrap_or(false))
            .count(),
        Err(_) => 0,
    }
}
