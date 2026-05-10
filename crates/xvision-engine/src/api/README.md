# `xvision_engine::api`

Single source of truth for every operation an external caller can invoke.
CLI handlers (in `xvision-cli`), MCP tools (in `xvision-mcp`), and the
future agent runner / scheduler all dispatch through this module.
**Business logic lives here, nowhere else.**

## Adding a new domain

1. Create `api/<domain>.rs` (e.g., `api/eval.rs`, `api/settings.rs`).
2. Re-export from `api/mod.rs`: `pub mod <domain>;`.
3. Each function takes `ctx: &ApiContext` as its first arg and returns
   `ApiResult<T>`. Domain-specific request types live alongside the function.
4. Every function records to `api_audit` via `audit::record(...)` on
   completion (both success and failure paths). Use `Instant::now()` at the
   top of the function, compute `duration_ms` at the bottom.
5. Map crate-specific errors to `ApiError::{NotFound, Validation, Conflict,
   Internal}` based on semantic. Don't leak underlying error types — that's
   what `ApiError::Internal(e.to_string())` is for. For `tokio::fs` errors
   coming through `anyhow::Error`, walk the cause chain and downcast to
   `std::io::Error` to detect `NotFound` (see `api::strategy::is_not_found`).
6. Add tests in `tests/api_<domain>.rs` following the pattern in
   `api_strategy.rs`. Always cover at least: empty state, success path,
   `NotFound` mapping, and the audit-row write.

## Function shape (canonical)

```rust
pub async fn <op>(ctx: &ApiContext, req: <ReqType>) -> ApiResult<<RespType>> {
    let started = Instant::now();
    let result = <op>_inner(ctx, req).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "<domain>",
        "<op>",
        target_id_or_none,
        Some(serde_json::to_string(&req).unwrap_or_default()).as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}
```

The `_inner` split keeps the audit-record call out of every error-return
path — the outer fn always records exactly once.

## Why this exists

Each action in v1 test has at least two callers (CLI + MCP) from day one.
Without this module, those callers would each implement the same business
logic in parallel, with parallel test surfaces and parallel bug-fix paths.
With this module, every action is written once and tested once.

When the agent runner and scheduler from the xvn-scheduling-and-agent-cli
plan series eventually ship, they slot in as additional callers — no
refactor of existing handlers required. That's why `Actor::AgentRunner` and
`Actor::Scheduler` already exist in the enum.

## CLI / MCP wrapper rules

- **CLI handlers** in `xvision-cli/src/commands/<X>.rs` are thin: parse clap
  args → build `ApiContext` → call `engine::api::<domain>::<fn>` → render
  result. Target: ≤15 lines per handler.
- **MCP tool handlers** in `xvision-mcp/` register `engine::api::*` functions
  directly as tools — no wrapper layer.
- **No business logic** outside this module. If you find yourself writing
  validation or persistence in a CLI handler, lift it into the api fn.

## Migration numbering

This crate owns `crates/xvision-engine/migrations/`. The registry is in
`v1-shipping-plan.md` §"Migration reservations". Never claim a new number
without editing that table in the same commit. Migration 001 (`api_audit`)
is owned by this plan.
