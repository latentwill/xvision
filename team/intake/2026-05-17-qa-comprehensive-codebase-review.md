# Intake — 2026-05-17 — QA: comprehensive codebase review

Decomposition of `qa/2026-05-17-comprehensive-codebase-review.md` into
execution-board tracks.

## Source

Static review of active product code under `crates/`, `frontend/web`,
`xvision-agentd`, `xtask`, `probes`, dashboard routes, engine APIs, eval
executors, LLM dispatch, CLI command surfaces, settings/secrets handling,
streaming/chart paths, scenario/strategy storage, and agent sidecar code. No
runtime validation was executed during the review.

Findings span three severities (3 × P1, 3 × P2, 4 × P3) and group cleanly
along ownership boundaries — engine slot runtime, sidecar control plane,
dashboard auth surface, role-handling consistency, and a handful of
isolated leaves.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P1 | `execute_slot` tool-use loop has no iteration / wall-clock cap | `qa-execute-slot-cap` |
| 2 | P1 | `xvision-agentd` accepts `budget_limits` but never enforces them | `qa-agentd-budget-enforcement` |
| 3 | P1 | `/api/cli/jobs` runs arbitrary `xvn` argv with no auth | `qa-dashboard-auth-hardening` |
| 4 | P2 | Danger routes rely on a frontend-embedded confirm token | `qa-dashboard-auth-hardening` (combined) |
| 5 | P2 | Attached `Trader` role passes eval validation but output is dropped | `qa-role-normalization` |
| 6 | P2 | Role validation accepts whitespace variants that bypass exact graph refs | `qa-role-normalization` (combined) |
| 7 | P3 | Reasoning-class truncation hint misses whitespace-padded trader roles | `qa-role-normalization` (combined) |
| 8 | P3 | `FilesystemStore::path_for` joins arbitrary strategy IDs into paths | `qa-strategy-id-path-safety` |
| 9 | P3 | Eval retry idempotency ignores `params_override` despite comment | `qa-eval-retry-params-override` |
| 10 | P3 | Hold chart markers render at price 0.0 when bar lookup misses | `qa-chart-hold-marker-zero` |

The reviewer's recommended remediation order is preserved: P1 tracks land
first (1, 2, 3+4), then role normalization (5+6+7), then the path-safety and
edge-case leaves.

## Track summaries

### `qa-execute-slot-cap` (P1, foundation)

Bound `execute_slot`'s tool-use loop in `crates/xvision-engine/src/agent/execute.rs`
with a conservative iteration cap (default 8–12, configurable). On exhaustion,
return a typed error carrying slot role, model, tool names requested,
accumulated token counts, and last stop reason. Callers (`run_pipeline`,
eval executors) already propagate errors — scope is limited to `execute.rs`
plus its tests.

### `qa-agentd-budget-enforcement` (P1, leaf)

Make `session.start_run`'s `budget_limits` real: enforce `max_wall_ms` with an
abortable timeout around each `step`/`run`, and abort once cumulative token
usage exceeds `max_input_tokens`/`max_output_tokens`. Return
`status: "aborted"` with a budget-specific error. Touches sidecar TS
(`xvision-agentd/src/{methods/session,session/store,session/build-agent}.ts`)
and may need a protocol field on the Rust client side
(`crates/xvision-agent-client/src/protocol.rs`) for the aborted reason code.

### `qa-dashboard-auth-hardening` (P1, integration)

Combined fix for findings 3 and 4 because both require an auth/local-only
gate on the dashboard:

- Replace `/api/cli/jobs` argv denylist with an allowlist of safe job
  templates (e.g. `bars fetch`). Block `fire-trade`, provider/secret
  mutation, destructive settings, dashboard spawning, and arbitrary
  subcommands unless an explicit local-only developer-mode flag is set.
- Gate danger routes (`wipe_db`, `factory_reset`) behind auth or a
  loopback-only check, and replace the embedded `yes-i-am-sure` token with
  either operator-typed phrase verification server-side or a short-lived
  server-issued challenge.
- Add the auth layer middleware in `crates/xvision-dashboard/src/server.rs`;
  reuse for both the CLI jobs and danger surfaces.

Single-writer claim on `dashboard/src/{server,lib}.rs` (previously held by
deferred `q15-tailscale-serve-api-reachability`).

### `qa-role-normalization` (P2, leaf)

Normalize `AgentRef.role` at mutation boundaries in
`crates/xvision-engine/src/strategies/{agent_ref,validate}.rs`: trim, reject
empty, and pick a single canonicalization (preserve case + exact comparison
everywhere, OR lowercase canonical keys everywhere). Then make the
downstream comparisons consistent — `agent/pipeline.rs` output assignment,
`eval/executor/{backtest,paper}.rs` `trader_model_id` helpers. After this
track, accepted trader strategies (any case, any whitespace pad) reliably
produce a populated `PipelineOutputs.trader` and get the reasoning-class
truncation hint.

Avoids touching `crates/xvision-engine/src/api/eval.rs` so it doesn't
collide with `qa-eval-retry-params-override`. The existing
`validate_eval_trader_source` `eq_ignore_ascii_case` check is already
permissive; the upstream normalization makes it irrelevant in practice.

### `qa-strategy-id-path-safety` (P3, leaf)

Introduce a strict `validate_strategy_id_for_path` helper (or a typed
`StrategyId`) and require an ASCII filename-safe pattern such as
`^[A-Za-z0-9_-]+$` before any filesystem operation in
`crates/xvision-engine/src/strategies/store.rs`. Reject IDs with path
separators, `.` segments, or platform-specific separators. Wire the check
into `authoring.rs` and any other surface that hands raw IDs to the store
(`crates/xvision-engine/src/api/strategy.rs`).

### `qa-eval-retry-params-override` (P3, leaf)

Either include `params_override` equality in the in-flight sibling
predicate in `crates/xvision-engine/src/api/eval.rs` retry handler so the
implementation matches the documented `(agent_id, scenario_id, mode,
params_override)` idempotency key, or update the comment and API
semantics if coalescing across overrides is intentional. Tracks the
contract by adding a regression test that retries with a different
`params_override` and asserts a new run is started.

### `qa-chart-hold-marker-zero` (P3, leaf)

In `crates/xvision-engine/src/api/chart.rs::build_markers`, stop emitting
hold markers at price `0.0` when a decision timestamp doesn't match a
loaded bar. Either skip the marker, or fall back to the nearest prior bar
close within the run granularity (with a diagnostic recorded so silent
fallback can be detected).

## Out of scope

- Anything not flagged in the QA review (no other engine or frontend
  refactors).
- Adding new auth mechanisms beyond what's needed to gate the existing
  unauthenticated surfaces (no SSO, OIDC, RBAC — token or local-only check
  is enough for this wave).
- Architectural restructuring of role handling beyond canonicalizing
  `AgentRef.role` and removing drift between comparison sites.
- Re-running the QA review (the static review report is the input; this
  wave executes its recommended remediations only).

## Open coordination notes

- `crates/xvision-dashboard/src/server.rs` and `lib.rs` are listed in
  `team/CONFLICT_ZONES.md` as released (previously held by deferred
  `q15-tailscale-serve-api-reachability`). `qa-dashboard-auth-hardening`
  re-claims them. If `q15-tailscale-serve-api-reachability` is ever
  revived, it must stack on this track's branch.
- `crates/xvision-engine/src/eval/executor/{backtest,paper}.rs` are
  released conflict-zone rows. `qa-role-normalization` reclaims both.
- `crates/xvision-engine/src/api/eval.rs` is split functionally between
  `qa-role-normalization` (does NOT touch this file) and
  `qa-eval-retry-params-override` (sole owner). Confirmed in the
  decomposition above to avoid a multi-owner glob.
