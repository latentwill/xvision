---
track: qa-agentd-budget-enforcement
lane: leaf
wave: qa-2026-05-17
worktree: .worktrees/qa-agentd-budget-enforcement
branch: task/qa-agentd-budget-enforcement
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - xvision-agentd/src/methods/session.ts
  - xvision-agentd/src/session/store.ts
  - xvision-agentd/src/session/build-agent.ts
  - xvision-agentd/src/session/budget.ts
  - xvision-agentd/test/session/budget.test.ts
  - xvision-agentd/test/session/start-run.test.ts
  - xvision-agentd/test/session/step.test.ts
  - crates/xvision-agent-client/src/protocol.rs
forbidden_paths:
  - xvision-agentd/src/methods/health.ts
  - crates/xvision-engine/**
  - crates/xvision-dashboard/**
  - frontend/**
interfaces_used:
  - "@cline/sdk — agent.run / agent.continue / usage tracking"
  - "xvision-agent-client protocol — start_run / step / end_run / aborted status"
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir xvision-agentd typecheck
  - pnpm --dir xvision-agentd test
  - cargo build -p xvision-agent-client
  - cargo test -p xvision-agent-client
acceptance:
  - "`session.start_run` continues to validate and store `budget_limits` as today"
  - "`session.step` (and any `agent.run`/`agent.continue` invocation) enforces `max_wall_ms` via an abortable timeout per step"
  - "Cumulative input/output tokens are tracked across steps; once `max_input_tokens` or `max_output_tokens` is exceeded, the next step short-circuits without invoking the agent"
  - On wall-clock or token-cap exhaustion, the step result is `status: \"aborted\"` with a typed budget-specific error code (e.g. `budget_wall_ms_exceeded`, `budget_input_tokens_exceeded`, `budget_output_tokens_exceeded`)
  - The Rust client (`crates/xvision-agent-client/src/protocol.rs`) deserializes the aborted status + reason without breaking existing happy-path tests
  - Regression tests cover all three exhaustion paths and the happy-path under-budget path
---

# Scope

Implements remediation step 2 of `qa/2026-05-17-comprehensive-codebase-review.md`
("Agent sidecar accepts budget limits but does not enforce them"). Makes
the documented `budget_limits` contract real: wall-clock and token caps
actually abort runs that exceed them, with a typed status so callers can
distinguish budget exhaustion from other failures.

Two enforcement vectors:

1. **Wall-clock**: wrap each `agent.run` / `agent.continue` invocation in an
   abortable timeout derived from `max_wall_ms` minus elapsed time since
   the run started. The timeout aborts the underlying SDK call and returns
   `status: "aborted"` with reason `budget_wall_ms_exceeded`.
2. **Token caps**: prefer passing supported budget options into `@cline/sdk`
   if available; otherwise track cumulative usage in `session/store.ts` and
   refuse to start the next step once a cap is exceeded.

A new `xvision-agentd/src/session/budget.ts` module is the natural home for
the timer + cumulative-usage helpers so `session.ts` stays thin.

# Out of scope

- Per-tool budget caps (only run-level wall-clock + token caps in this
  track).
- Telemetry/observability emission for budget aborts. The aborted status
  is enough; observability bridging is a Phase B agent-run-observability
  concern.
- Auth/authorization on the agentd JSON-RPC surface (separate concern, not
  flagged in this QA review).
- Changes to `health.ts` or any non-session method.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-agentd-budget-enforcement \
  -b task/qa-agentd-budget-enforcement origin/main
git -C .worktrees/qa-agentd-budget-enforcement status
pnpm --dir xvision-agentd install
```

# Notes

Implementation hints (do not rewrite the contract — use as starting points):

- The `cline-sdk-wave1-2` merge (#208 on 2026-05-17) is the foundation —
  the session lifecycle (`start_run` / `step` / `end_run`) and tool callback
  round-trip already exist; this track adds enforcement on top.
- `@cline/sdk` exposes usage on each `agent.run`/`continue` result. If the
  SDK does not yet support a native budget option, accumulating usage in
  `store.ts` is the fallback.
- The Rust-side protocol change should be additive: add an optional reason
  string on the aborted variant, leaving existing deserialization happy.
