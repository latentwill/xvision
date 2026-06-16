//! xvn-mcp — Model Context Protocol server exposing the `xvision-data`
//! indicator surface as agent-callable tools.
//!
//! Registered as Cline agent tools via the `xvision-agentd` sidecar so any
//! Cline-driven agent stage (trader, risk, …) can recompute
//! indicators at parameter sets the snapshot doesn't pre-bake (e.g.
//! RSI(7) when the snapshot only carries RSI(14)).
//!
//! Stateless by design: every tool takes the price/HLC series as a
//! parameter. No on-disk data root, no API keys, no determinism risk —
//! the agent supplies the input from prompt context and the server is a
//! pure compute layer over `xvision-data`.
//!
//! See `crates/xvision-mcp/src/tools.rs` for the tool surface.

use anyhow::Result;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

pub mod marketplace_client;
pub mod tools;

use tools::XvisionTools;

pub async fn run_stdio_server() -> Result<()> {
    init_tracing();

    tracing::info!(
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
        "xvn-mcp/xvision-mcp starting on stdio"
    );

    let service = XvisionTools::new()
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!(error = %e, "service init failed"))?;

    service.waiting().await?;
    Ok(())
}

fn init_tracing() {
    // Logs to stderr only; stdout is the JSON-RPC channel.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init()
        .ok();
}

#[cfg(test)]
mod tests {
    #[test]
    fn tracing_init_does_not_panic_when_global_subscriber_exists() {
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::sink)
            .with_ansi(false)
            .try_init();

        super::init_tracing();
    }
}
