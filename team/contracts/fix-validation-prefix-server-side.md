---
track: fix-validation-prefix-server-side
lane: leaf
wave: qa-operator-2026-05-19
worktree: .worktrees/fix-validation-prefix-server-side
branch: task/fix-validation-prefix-server-side
base: origin/main
status: ready
depends_on: []   # #318 merged 2026-05-19 (commit 2dae30a)
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/error.rs
  - crates/xvision-dashboard/tests/**
  - frontend/web/src/features/eval-runs/review/ReviewPanel.tsx
  - frontend/web/src/features/eval-runs/review/ReviewPanel.test.tsx
  - frontend/web/src/api/client.ts
forbidden_paths:
  - crates/xvision-dashboard/src/routes/**
  - crates/xvision-dashboard/src/llm_dispatch.rs
  - crates/xvision-dashboard/src/wizard_loop.rs
interfaces_used:
  - DashboardError::Validation { field, msg } (response serialization)
  - frontend ApiError shape (code, message, optional field)
verification:
  - cargo test -p xvision-dashboard
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run ReviewPanel
  - pnpm --dir frontend/web build
acceptance:
  - `crates/xvision-dashboard/src/error.rs` — drop the
    `format!("{field}: {msg}")` embedding at line 42. The response
    JSON now emits `{ "code", "message", "field" (when applicable) }`
    so consumers can read the field separately if they care, but the
    `message` is operator-readable without server-side jargon.
  - Concretely, the `into_response` branch becomes:
    ```rust
    DashboardError::Validation { field, msg } => {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": "validation",
                "message": msg.clone(),
                "field": field.clone(),
            })),
        ).into_response();
    }
    ```
    (Or equivalent — the worker may refactor the surrounding match to
    handle the field-bearing case cleanly. The point is: `message` no
    longer contains the `field` prefix.)
  - `frontend/web/src/features/eval-runs/review/ReviewPanel.tsx`'s
    `describeReviewError` no longer needs the prefix-strip branch.
    Remove the `if (error.code === "validation" && message.startsWith("request: "))`
    block — it's a hack that's no longer needed. The function becomes
    a simple passthrough of `error.code` + `error.message`.
  - `frontend/web/src/api/client.ts` — if `ApiError` doesn't already
    expose the optional `field` property, add it (additive — readable
    from the JSON response if present, undefined otherwise). Existing
    consumers that only read `code` + `message` keep working.
  - Tests:
    - Dashboard: existing tests pinning `"request: ..."` strings need
      updates. Grep for `request:` in `crates/xvision-dashboard/tests/`
      and update assertions to match the new clean message.
    - Frontend ReviewPanel: the `describeReviewError` helper tests
      that exercised the prefix-strip path should be updated/removed.
      Add a test asserting the clean (non-prefixed) message renders
      correctly when the backend returns the new shape.
  - All 15+ other `DashboardError::Validation` call sites continue to
    work — the format change is at the response-serialization layer,
    not at the call site. Their messages just become cleaner.
  - No `try/catch` silencing or fallback shims
    (`feedback_alpha_root_cause`). The fix removes a hack, doesn't
    add one.
parallel_safe: true
parallel_conflicts: []
---

# Scope

PR #318 (merged 2026-05-19) made the operator's silent 400 on
`POST /api/eval/runs/.../review` actually surface a structured error.
The root cause was identified as **(b)** in #318's status note —
the backend was already emitting `{ code, message }` correctly, but
`message` was prefixed with `request: ` because
`crates/xvision-dashboard/src/error.rs:42` embeds the `field` name
into the message string:

```rust
DashboardError::Validation { field, msg } => {
    (StatusCode::BAD_REQUEST, "validation", format!("{field}: {msg}"))
}
```

#318 worked around this with a frontend strip-prefix hack in
`describeReviewError`. That hack is targeted (only the review panel),
which means **every other consumer of `DashboardError::Validation`
across the dashboard still gets the `field: msg` prefix in its
message**. There are 15+ call sites — wizard, eval runs, search,
CLI, agent runs — all inheriting the same server-side jargon.

This track fixes the cause: drop the prefix at the response layer,
emit `field` as a separate JSON property if useful, and remove the
frontend strip-prefix hack.

Anchor reading:

- PR #318 status note at
  `team/status/eval-review-400-diagnose.md` (if present; otherwise
  PR #318's body documents the diagnosis).
- `crates/xvision-dashboard/src/error.rs:33-58` (response
  serialization).
- `frontend/web/src/features/eval-runs/review/ReviewPanel.tsx`
  `describeReviewError` helper (the strip-prefix hack).
- All 15+ `DashboardError::Validation` call sites (grep:
  `grep -rn "DashboardError::Validation" crates/xvision-dashboard/src/`).

# Out of scope

- Refactoring the `DashboardError` enum itself.
- Changing the wire protocol's `{ code, message }` envelope shape —
  the change is purely **dropping the `field` prefix from `message`
  and exposing `field` as an optional sibling property**.
- Touching the call sites — they continue to call
  `DashboardError::Validation { field, msg }` with the same args.
- Other dashboard error types (`NotFound`, `Conflict`, `Internal`,
  `HttpError`).
- Adding new error codes or remediation hooks (that was #256
  territory; #318 verified #256 was working).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/fix-validation-prefix-server-side status
git -C .worktrees/fix-validation-prefix-server-side log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/fix-validation-prefix-server-side \
  -b task/fix-validation-prefix-server-side origin/main
```

# Notes

Append checkpoints / PR links below.
