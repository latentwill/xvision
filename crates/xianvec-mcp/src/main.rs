//! `xvn-mcp` — Model Context Protocol server (stdio transport).
//!
//! Started by ACPX from a `mcpServers: [...]` registration in
//! `acpx.config.json`. Speaks MCP over stdin/stdout; logs go to stderr so
//! they don't corrupt the JSON-RPC stream.

use anyhow::Result;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

use xianvec_mcp::tools::XianvecTools;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Logs to stderr only — stdout is the JSON-RPC channel.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!(
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
        "xvn-mcp starting on stdio"
    );

    let service = XianvecTools::new()
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!(error = %e, "service init failed"))?;

    service.waiting().await?;
    Ok(())
}
