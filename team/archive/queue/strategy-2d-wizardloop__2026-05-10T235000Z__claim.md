---
from: strategy-2d-wizardloop
to: all
topic: claim
created_at: 2026-05-10T23:50:00Z
ack_required: false
---

# `strategy-2d-wizardloop` track claimed (Plan 2d T6 — server-side WizardLoop)

The user's stated next deliverable. Stacks on PR #33
(feature/strategy-2a-llm-tool-use — Plan 2a 2A.C T10 tool-use shape).
Worktree `.worktrees/wizardloop`, branch
`feature/strategy-2d-wizardloop`. Two commits.

## Scope

**Commit 1 — engine authoring dispatcher** (`xvision_engine::authoring`)

Pure-Rust functions over `&dyn BundleStore` for the seven verbs
(`list_templates`, `create_strategy`, `get_strategy`, `update_slot`,
`set_mechanical_param`, `set_risk_config`, `validate_draft`). Both the
MCP server (PR #31) and WizardLoop call into this same module. Errors
are flat `anyhow::Result`; surface-specific error mapping
(rmcp::ErrorData for MCP, axum::Json for the dashboard) lives at the
call site.

5 unit tests cover happy paths + the preset/explicit branch.

**Commit 2 — WizardLoop** (`xvision_dashboard::wizard_loop`)

Server-side LLM agent that takes one chat message from the user and
drives the seven authoring verbs over the multi-turn tool-use shape
that landed in PR #33. `next_event()` runs an internal tool-use loop:

1. Call `LlmDispatch::complete` with system prompt + message log + the
   seven verbs as `ToolDefinition`s
2. Queue `WizardEvent::Token` for every `ContentBlock::Text`
3. For every `ContentBlock::ToolUse`, route to
   `xvision_engine::authoring::*`, queue `ToolCall` + `ToolResult`
   events, append `ToolResult` blocks to the conversation
4. When the model emits text-only `EndTurn`, queue
   `WizardEvent::Done { draft_id }` and stop

`WizardEvent` is surface-agnostic — the SSE route in `routes::wizard`
(follow-up) wraps these in an event-stream body. Tests drive the loop
directly with `MockDispatch::sequence(...)`.

6 tests cover: text-only response, single tool-use round trip,
`create_strategy` → `draft_id` tracking, unknown-template error
surface, unknown-tool-name error surface, and the seven-verb tool-defs
invariant.

System prompt at `crates/xvision-dashboard/prompts/wizard.md`.

## Files this track touches

- `crates/xvision-engine/src/authoring.rs` (new, ~350 lines)
- `crates/xvision-engine/src/lib.rs` (1-line `pub mod authoring;`)
- `crates/xvision-dashboard/src/wizard_loop.rs` (new, ~480 lines)
- `crates/xvision-dashboard/src/lib.rs` (1-line `pub mod wizard_loop;`)
- `crates/xvision-dashboard/Cargo.toml` (+`async-trait` dep)
- `crates/xvision-dashboard/prompts/wizard.md` (new system prompt)
- `Cargo.lock`

## Stacks on

- **PR #33** — `feature/strategy-2a-llm-tool-use` (Plan 2a 2A.C T10
  tool-use shape on `LlmRequest`/`Response`). Don't merge this PR
  until #33 lands; the diff is meaningless without the new
  `Message`/`ContentBlock`/`ToolDefinition` types.

## Out of scope (deferred to follow-ups)

- **SSE wizard route** — `routes::wizard` POST `/api/wizard/chat`
  returning an `event-stream` of `WizardEvent`s. The agent is
  surface-ready; the route is mechanical (axum SSE + `tokio::sync::mpsc`
  + `WizardLoop::next_event` in a task).
- **Frontend wiring** — `setup.tsx` upgrade from stub to a chat UI
  consuming the SSE stream.
- **`xvision-mcp::tools` refactor** — delegate to the shared
  dispatcher and drop its inline copies. Pure cleanup; not load-bearing.

## v1 QA value

This is the agent payoff for Plan 2a + Plan 2d combined. With this
landed, the wizard knows how to walk a user through "Buys dips when
trend is up" → a validated `StrategyBundle` ready for backtest, end
to end, with no operator-CLI round-trip.
