//! `xvn-mcp` / `xvision-mcp` — Model Context Protocol server (stdio transport).
//!
//! Started by an MCP host (the `xvision-agentd` Cline sidecar, or a local MCP
//! client). Speaks MCP over stdin/stdout; logs go to stderr so they don't
//! corrupt the JSON-RPC stream.

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    xvision_mcp::run_stdio_server().await
}
