//! Workspace task runner. Today: Rust → TypeScript codegen for the engine API.
//!
//! Usage:
//!   cargo xtask gen-types

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
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
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .map(|p| p.to_path_buf())
        .context("xtask manifest has no parent dir")
}

fn gen_types(sh: &Shell) -> Result<()> {
    let final_out_dir = Path::new("frontend/web/src/api/types.gen");
    let final_barrel = Path::new("frontend/web/src/api/types.gen.ts");
    let stage_root = PathBuf::from(format!(".clawpatch/types-stage-{}", std::process::id()));
    let export_base = stage_root.join("crates/xvision-engine");
    let staged_out_dir = stage_root.join("frontend/web/src/api/types.gen");

    remove_dir_if_exists(&stage_root)?;
    std::fs::create_dir_all(&export_base)
        .with_context(|| format!("create staged export base {}", export_base.display()))?;

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
    run_ts_export(sh, "xvision-core", &export_base, &staged_out_dir)?;
    run_ts_export(sh, "xvision-memory", &export_base, &staged_out_dir)?;
    run_ts_export(sh, "xvision-engine", &export_base, &staged_out_dir)?;

    let entries: Vec<_> = std::fs::read_dir(&staged_out_dir)
        .with_context(|| format!("read {}", staged_out_dir.display()))?
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

    replace_generated_outputs(&stage_root, &staged_out_dir, final_out_dir, final_barrel, &barrel)?;
    println!("wrote {} type exports", stems.len());
    Ok(())
}

/// Run `cargo test --features ts-export --lib` for a single crate and require
/// that *something* came out of it. A compile failure that emits nothing fails
/// the task loudly so a later barrel rebuild can't paper over a missing crate's
/// types. A test-assertion failure after files were already written is treated
/// as benign — ts-rs writes its `.ts` files from the test body before any
/// assertion fires, so the bindings we care about are already on disk.
fn run_ts_export(sh: &Shell, pkg: &str, export_base: &Path, out_dir: &Path) -> Result<()> {
    let before = count_ts_files(out_dir);
    let result = cmd!(sh, "cargo test -p {pkg} --features ts-export --lib --quiet")
        .env("TS_RS_EXPORT_DIR", export_base)
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

fn count_ts_files(out_dir: &Path) -> usize {
    match std::fs::read_dir(out_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "ts").unwrap_or(false))
            .count(),
        Err(_) => 0,
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("remove {}", path.display())),
    }
}

fn replace_generated_outputs(
    stage_root: &Path,
    staged_out_dir: &Path,
    final_out_dir: &Path,
    final_barrel: &Path,
    barrel: &str,
) -> Result<()> {
    let backup_out_dir = stage_root.join("backup-types.gen");
    let backup_barrel = stage_root.join("backup-types.gen.ts");
    let staged_barrel = stage_root.join("types.gen.ts");

    if let Some(parent) = final_out_dir.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&staged_barrel, barrel)
        .with_context(|| format!("write staged barrel {}", staged_barrel.display()))?;

    if final_out_dir.exists() {
        std::fs::rename(final_out_dir, &backup_out_dir).with_context(|| {
            format!(
                "move existing {} to {}",
                final_out_dir.display(),
                backup_out_dir.display()
            )
        })?;
    }
    if final_barrel.exists() {
        std::fs::rename(final_barrel, &backup_barrel).with_context(|| {
            format!(
                "move existing {} to {}",
                final_barrel.display(),
                backup_barrel.display()
            )
        })?;
    }

    let replace_result = (|| -> Result<()> {
        std::fs::rename(staged_out_dir, final_out_dir).with_context(|| {
            format!(
                "move staged {} to {}",
                staged_out_dir.display(),
                final_out_dir.display()
            )
        })?;
        std::fs::rename(&staged_barrel, final_barrel).with_context(|| {
            format!(
                "move staged {} to {}",
                staged_barrel.display(),
                final_barrel.display()
            )
        })?;
        Ok(())
    })();

    if let Err(err) = replace_result {
        let _ = remove_dir_if_exists(final_out_dir);
        if backup_out_dir.exists() {
            let _ = std::fs::rename(&backup_out_dir, final_out_dir);
        }
        if backup_barrel.exists() {
            let _ = std::fs::rename(&backup_barrel, final_barrel);
        }
        return Err(err);
    }

    remove_dir_if_exists(&backup_out_dir)?;
    match std::fs::remove_file(&backup_barrel) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(err).with_context(|| format!("remove {}", backup_barrel.display())),
    }
    remove_dir_if_exists(stage_root)?;
    Ok(())
}
