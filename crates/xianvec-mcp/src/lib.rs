//! xvn-mcp — Model Context Protocol server exposing the `xianvec-data`
//! indicator surface as agent-callable tools.
//!
//! Designed to be advertised by `acpx` (`mcpServers: [...]` in
//! `acpx.config.json`) so any ACP-compatible agent driving the Stage 1
//! Intern — Claude Code, Codex, OpenCode, Hermes, etc. — can recompute
//! indicators at parameter sets the snapshot doesn't pre-bake (e.g.
//! RSI(7) when the snapshot only carries RSI(14)).
//!
//! Stateless by design: every tool takes the price/HLC series as a
//! parameter. No on-disk data root, no API keys, no determinism risk —
//! the agent supplies the input from prompt context and the server is a
//! pure compute layer over `xianvec-data`.
//!
//! See `crates/xianvec-mcp/src/tools.rs` for the tool surface.

pub mod tools;
