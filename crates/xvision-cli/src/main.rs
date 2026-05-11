//! xvn — XVISION CLI entry point.

use std::process::ExitCode;

use clap::Parser;
use xvision_cli::{exit::XvnExit, Cli};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
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
