---
from: strategy-2a-tooluse
to: all
topic: claim
created_at: 2026-05-10T23:15:00Z
ack_required: false
---

# `strategy-2a-tooluse` track claimed (Plan 2a Phase 2A.C T10 — tool-use shape)

Stacks on `strategy-2a-mcp` (PR #31, in flight). Worktree
`.worktrees/strategy-2a-tooluse`, branch
`feature/strategy-2a-llm-tool-use`. Single-commit PR.

## Why this PR exists (WizardLoop prerequisite)

The user asked for WizardLoop (Plan 2d T6). WizardLoop's spec
(`crates/xvision-dashboard/src/wizard_loop.rs`) imports
`Message`, `ContentBlock::ToolUse`, `ToolDefinition`, `StopReason`
from `xvision_engine::agent::llm` and runs a multi-turn tool-use
loop driving the MCP authoring verbs (which landed in PR #31).

The existing `LlmRequest { system_prompt, user_prompt: String }` is
single-turn-only — there is no Message log, no ContentBlock, no
ToolUse/ToolResult, no `tools` field on the request. T10 lifts the
trait to the multi-turn tool-use shape so WizardLoop is tractable
in the next PR.

## Scope (T10 only)

- New types: `Message`, `ContentBlock { Text, ToolUse, ToolResult }`,
  `ToolDefinition`, `StopReason { EndTurn, ToolUse, MaxTokens }`.
- `LlmRequest`: drop `user_prompt`, add `messages: Vec<Message>` +
  `tools: Vec<ToolDefinition>`.
- `LlmResponse`: drop `text`, add `content: Vec<ContentBlock>` +
  `stop_reason`, with `.text()` / `.tool_uses()` convenience.
- `MockDispatch`: queue-of-responses + `tool_use(...)` builder for
  loop fixtures.
- `AnthropicDispatch::complete`: now sends `tools` and parses
  `type: "text"|"tool_use"` content blocks.
- 6 in-tree call sites patched minimally (single-turn behavior
  preserved): `agent/execute`, `agent/pipeline` (x2), `eval/findings`,
  `eval/executor/paper`, `cli/strategy`, plus 2 tests.

## Out of scope (still deferred)

- **T11** — wire `tool_use` blocks back to the `ToolRegistry` inside
  `execute_slot` (the in-loop tool dispatch). Easier now that the
  shape exists.
- **T12** — real OHLCV + IndicatorPanel hookup in `xvn strategy run`.
- **WizardLoop (Plan 2d T6)** — the actual user ask. Lands in the
  next PR after this merges.

## Files this track touches

`crates/xvision-engine/src/agent/llm.rs` (rewritten),
`crates/xvision-engine/src/agent/{execute,pipeline}.rs`,
`crates/xvision-engine/src/eval/findings/extractor.rs`,
`crates/xvision-engine/src/eval/executor/paper.rs`,
`crates/xvision-engine/tests/{llm_dispatch,agent_slot}.rs`,
`crates/xvision-cli/src/commands/strategy.rs`.

Zero overlap with currently-open PRs (#27/#28/#29/#31/#32 don't
touch `agent/llm.rs`).

## v1 QA value

Unblocks WizardLoop (Plan 2d T6) and the in-loop tool dispatch
(Plan 2a T11). Doesn't change observable behavior of the existing
strategy pipeline / eval executors / findings extractor.
