---
track: indicator-tool-wiring
lane: leaf
wave: eval-honesty-tail-2026-05-22
worktree: .worktrees/indicator-tool-wiring
branch: task/indicator-tool-wiring
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/tools/indicators.rs
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/llm.rs
  - crates/xvision-engine/tests/indicator_tool_wiring.rs
  - crates/xvision-mcp/src/tools.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/strategies/templates.rs
  - frontend/web/**
interfaces_used:
  - xvision_engine::tools::indicators::ToolName "indicator_panel" (existing)
  - xvision_engine::agent::pipeline tool-call dispatch path
  - LLM dispatch `tools` array assembly
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine --test indicator_tool_wiring
  - cargo test -p xvision-engine
acceptance:
  - The trader slot's LLM call carries `tools: [...indicator_panel...]` (not `[]`) when `allowed_tools` lists `indicator_panel` (templates already declare it)
  - Tool-call dispatch on `indicator_panel` returns the computed indicator panel and the result is fed back into the LLM call loop
  - Tests assert: (a) the dispatched `tools` array includes the entry, (b) a fixture trader response invoking `indicator_panel` actually executes the tool, (c) the tool result appears in the trace as a tool_call row
  - F43 (`trace-dock-emitters`) emits the tool_call event when this lands — coordinate ordering
---

# Scope

Wire `indicator_panel` from a declared-but-unused tool to a
functional tool the trader slot can request. Today the strategy
templates declare `allowed_tools: ["ohlcv", "indicator_panel"]`
(see `crates/xvision-engine/src/strategies/templates.rs:122,136,162,174,183`)
but the LLM dispatch ships `"tools": []` — the agent has no surface
to request indicators on demand.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
row "Wire `indicator_panel` tool through to trader slot (currently
`tools: []` in the LLM blob); the agent requests indicators, the
system does not stuff them."

# Out of scope

- Pre-computing indicators into the briefing — explicitly the opposite of this contract
- New indicators beyond the existing `indicator_panel` toolset (separate scope)
- MCP changes beyond surfacing existing indicator tools

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/indicator-tool-wiring -b task/indicator-tool-wiring origin/main
```

# Notes

The `ToolName::new("indicator_panel")` exists at
`crates/xvision-engine/src/tools/indicators.rs:23`. Trace where the
LLM dispatch assembles its `tools` array (likely in
`agent/execute.rs` or `agent/llm.rs`) and ensure `allowed_tools` →
JSON schema entries flows through.
