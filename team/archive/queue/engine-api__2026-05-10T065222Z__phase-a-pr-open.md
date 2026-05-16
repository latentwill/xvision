---
from: engine-api
to: all
topic: phase-a-pr-open
created_at: 2026-05-10T06:52:22Z
ack_required: false
---

# Engine API Foundation — PR #4 open

PR: https://github.com/latentwill/xvision/pull/4
Branch: `feature/engine-api-foundation`
Worktree: `.worktrees/engine-api`

## What landed

All 5 phases of `docs/superpowers/plans/2026-05-10-engine-api-foundation.md`:

1. `crates/xvision-engine/migrations/001_api_audit.sql` — append-only ops log
2. `xvision_engine::api::ApiContext`, `Actor`, `ApiError`, `ApiResult`
3. `xvision_engine::api::audit::record` — single audit entry-point
4. `xvision_engine::api::strategy::{list, get}` — representative ops
5. `crates/xvision-engine/src/api/README.md` — pattern doc

Tests: 38/38 pass (11 new), 0 failures, 1 ignored (pre-existing).
Workspace: `cargo build --workspace` green.

## Downstream tracks unblocked (after merge)

These tracks can begin once PR #4 merges to `main`:

- `eval-engine` (Plan #5) — also depends on `broker-surface`
- `frontend-foundation` Phase B — `/strategies` route wired to real api
- `strategy-2a-mcp` (Plan #6)
- `llm-providers` (Plan #7)
- `settings-onboarding` (Plan #10)
- `chat-rail` (Plan #11)
- `command-palette` (Plan #12)

## Pattern reminder for the next track to start a domain

```rust
// crates/xvision-engine/src/api/<domain>.rs
pub async fn <op>(ctx: &ApiContext, req: ReqType) -> ApiResult<RespType> {
    let started = Instant::now();
    let result = <op>_inner(ctx, req).await;
    let outcome = match &result { Ok(_) => Outcome::Ok, Err(e) => Outcome::Error(e.to_string()) };
    let _ = audit::record(ctx, "<domain>", "<op>", target, args_json,
                          outcome, started.elapsed().as_millis() as i64).await;
    result
}
```

CLI handlers in `xvision-cli/src/commands/<X>.rs` stay thin (≤15 lines): parse
clap args → build `ApiContext` → call `engine::api::<domain>::<fn>` → render.
MCP tool handlers in `xvision-mcp/` register `engine::api::*` directly. **No
business logic outside the api module.**
