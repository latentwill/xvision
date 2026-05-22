//! xvn — XVISION CLI entry point.

use std::process::ExitCode;

use clap::Parser;
use xvision_cli::{exit::XvnExit, Cli};

#[tokio::main]
async fn main() -> ExitCode {
    // Route tracing/log output to **stderr** so it never pollutes the
    // structured JSON channel on stdout. The CLI's stdout discipline
    // (`crates/xvision-cli/src/io.rs`) treats stdout as
    // structured-JSON-only when a verb is invoked with `--json`; that
    // contract is only meaningful if `tracing` itself respects the same
    // separation.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();
    match cli.run().await {
        Ok(()) => XvnExit::Success.into(),
        Err(e) => {
            eprintln!("{e}");
            e.exit.into()
        }
    }
}
