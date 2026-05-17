//! `xvision-mcp` alias for the MCP stdio server.

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    xvision_mcp::run_stdio_server().await
}
