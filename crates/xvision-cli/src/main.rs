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

    // Mirror the daemon startup: export provider secrets from
    // $XVN_HOME/secrets/providers.toml into the current process env so
    // commands like `eval run` work identically whether called via
    // `docker exec`, a plain CLI binary, or the HTTP API path inside the
    // daemon. Best-effort — a missing file (new install) or unresolvable
    // home is silently ignored; individual commands surface missing-key
    // errors when they actually need the key.
    if let Ok(home) = xvision_cli::commands::home::resolve_xvn_home_env() {
        let _ = xvision_engine::api::settings::providers::load_providers_secrets_into_env(&home).await;
    }

    let cli = Cli::parse();
    match cli.run().await {
        Ok(()) => XvnExit::Success.into(),
        Err(e) => {
            // CliError::Display already uses alternate formatting ({e:#})
            // for the anyhow source. Walk the error chain explicitly to
            // surface deeply-nested causes in Docker/stderr logs.
            eprintln!("{e}");
            let mut cause: &dyn std::error::Error = &e;
            while let Some(next) = std::error::Error::source(cause) {
                eprintln!("  cause: {next}");
                cause = next;
            }
            e.exit.into()
        }
    }
}
