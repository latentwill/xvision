# Intake — 2026-05-21 — V2B security & operability

Decomposes V2B (security hardening) from the action plan
`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` §V2B (items 4–6
in the V2 phase table). Lands as the next active phase on
`team/board-v2.md` after V2A/V2E/V2F closed.

## Why now

V2B is a hard prerequisite for V2C marketplace flow and V4 mainnet:
- A purchased marketplace strategy can call broker/wallet endpoints; those
  endpoints must require auth and be subject to a kill switch before any
  buyer's runtime exposes them.
- Remote CLI is already in active use over Tailscale (per
  `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md`);
  orphan-job recovery + audit trail are unfinished operational gaps that
  block confident production use.
- Secret redaction is not enforced anywhere consistent. SSE event streams,
  run artifacts, and exported examples are all potential leak channels.

The action plan calls out **five work packages** under V2B; this intake
folds them into **three contracts** so each contract is shippable as a
single track without cross-cutting handoffs:

| Action-plan work package | Intake contract |
|---|---|
| 1. Dashboard and API auth | `v2b-dashboard-auth-boundary` |
| 2. Secret handling | folded into `v2b-dashboard-auth-boundary` (redaction lives at the API surface) |
| 3. Remote CLI job safety | `v2b-remote-cli-job-safety` |
| 4. Broker and wallet guardrails | `v2b-broker-wallet-kill-switch` |
| 5. Audit and observability | split: auth-side log lives in auth-boundary contract; broker/wallet/marketplace log lives in kill-switch contract |

## Source anchors

- `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` §V2B (lines 66–108) — work-package list + exit checks.
- `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md` — remote-CLI architecture (current backend surface lives in `xvision-dashboard`; orphan recovery, restart semantics, and the auth/capability boundary are the named operational gaps).
- `docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md` — wallet-side guardrail requirements (testnet-only labelling, kill switch coupling).
- `docs/superpowers/plans/2026-05-11-typed-exit-codes.md` — typed exit codes the remote-CLI audit trail will record.

## Tracks

### 1. `v2b-dashboard-auth-boundary` (foundation)

**Scope:** Every mutating dashboard API route requires explicit auth.
Read-only status endpoints stay open on localhost; write/run/broker/
testnet/wallet endpoints require a session token. Wire bind-defaults so
the dashboard binds to `127.0.0.1` by default and prints a loud warning
on non-loopback binds. Add redaction tests for API error bodies, SSE
event payloads, and job log lines (provider/broker/wallet tokens never
appear, even on validation errors).

**Why foundation:** the auth surface this track lands is the gate every
other V2B track audits against. The remote-CLI track records "auth
context" per job (which only exists once this lands); the kill-switch
track exposes pause/resume as a mutating endpoint that must be gated.

**Owns:**
- `crates/xvision-dashboard/src/routes/**` — split mutating from read-only; add `require_auth` middleware.
- `crates/xvision-dashboard/src/auth/**` (new) — session token issue + verify.
- `crates/xvision-dashboard/src/redact.rs` (new) — secret-pattern redactor used by error-formatting + SSE serializer + job-log writer.
- Bind-default change in dashboard's bootstrap (probably `main.rs`).
- Tests under `crates/xvision-dashboard/tests/**` for auth gate, redaction, and bind-warning.

**Acceptance:**
- `curl POST` against any mutating route without auth returns 401; with valid session returns 200.
- `curl GET` against any read-only status route works without auth on loopback.
- Tests assert that provider/broker tokens never appear in error responses, SSE events, or job-log lines (snapshot test against known secret patterns).
- Dashboard refuses to bind to non-loopback addresses without an explicit `--bind 0.0.0.0` flag; loud warning when used.
- Session expiry default: 24h; configurable via `dashboard.session_ttl`.
- The Tailscale remote-CLI path either inherits a service-token auth context or is documented as exempt with a written rationale (the auth boundary is per-API, but Tailscale-only routes can rely on Tailscale ACL for the network-layer gate as long as the application records the user).

### 2. `v2b-remote-cli-job-safety` (leaf — depends on auth-boundary)

**Scope:** Close the operational gaps the remote-CLI spec named at
landing:
- **Orphan recovery:** on dashboard restart, jobs whose backing process exited without writing a final status are detected and marked `Orphaned` with a recoverable reason. Stale `*.lock` files older than the job's max runtime are cleaned.
- **Cancellation:** `DELETE /api/cli/jobs/:id` actually sends SIGTERM to the backing process, waits for graceful exit (5s grace), then SIGKILL. Cancel from the CLI side propagates.
- **Audit fields per job:** user (from auth context), source (Tailscale node or local), command class, start/end timestamps, exit code, output byte count. Persist to a `cli_job_audit` table or extend the existing job table.
- **Runtime/output caps:** per-job `max_runtime_seconds` and `max_output_bytes`; on breach, send SIGTERM and mark exceeded.
- **High-risk command allowlist:** explicit deny for arbitrary `sh -c`, `bash`, and shell-injection vectors; allow only verbs already in the CLI binary's clap-derived command list.

**Owns:**
- `crates/xvision-dashboard/src/routes/cli_jobs.rs` (or wherever the CLI-job routes live) — cancel + audit fields.
- `crates/xvision-dashboard/src/cli_jobs/orphan_recovery.rs` (new).
- New migration for `cli_job_audit` columns (or table).
- Tests under `crates/xvision-dashboard/tests/cli_jobs_*.rs`.

**Acceptance:**
- Kill `xvn dash` mid-job, restart; the job's row transitions to `Orphaned` with `recovered_at` timestamp.
- `DELETE` on a running job kills the process within 6s.
- Audit row exists for every CLI job with all named fields.
- Job killed when `max_runtime_seconds` exceeded.
- Job's output truncated + status flagged when `max_output_bytes` exceeded.
- Tests: orphan detection on simulated restart; cancellation timing; output cap enforcement.

**Depends on:** `v2b-dashboard-auth-boundary` for the user/source fields.
Holds dispatch until that PR merges.

### 3. `v2b-broker-wallet-kill-switch` (leaf — depends on auth-boundary)

**Scope:**
- **Global pause/kill switch:** a single dashboard endpoint (`POST /api/safety/pause` / `POST /api/safety/resume`) that gates *all* broker and wallet actions across the engine. State persisted in the dashboard's DB; checked by every broker submit, every wallet transaction, every chain write. Default state: `paused = false` on fresh install, but `paused = true` for any non-testnet/non-paper venue until explicitly resumed.
- **Per-run limits:** wire `notional_cap_usd`, `max_order_count`, `max_leverage`, `max_loss_pct` from `Scenario` and `Strategy` into the executor and broker submit paths; breach aborts the run with a recorded reason.
- **Testnet/paper enforcement:** every non-local-paper execution must carry an explicit `venue_label: Testnet | Paper | Live`; UI shows the label prominently (no popup — inline badge). API responses include the label. Logs prefix it. Confused-deputy-tests (e.g. paper-mode endpoint hit while wallet config is live) reject with a clear error.
- **Audit log:** every broker action, wallet action, marketplace action, contract write writes one row to a `safety_audit` log with user, action, parameters, result, and the pause state at the time.

**Owns:**
- `crates/xvision-engine/src/api/safety/**` (new) — pause/resume endpoints.
- `crates/xvision-execution/src/**` — pause-gate before submit; per-run limit checks.
- `crates/xvision-engine/src/eval/run.rs` + `crates/xvision-engine/src/eval/scenario.rs` — `venue_label` field threading.
- `crates/xvision-engine/src/wallet/**` (or wherever wallet writes live) — pause-gate.
- Migration for `safety_audit` table.
- Frontend: badge components in `frontend/web/src/features/safety/` + global pause indicator in the chrome.

**Acceptance:**
- Hit the pause endpoint; broker submit returns 503 with `safety_paused`.
- Per-run limit breach aborts the run with `RunAbort::SafetyLimit { kind }`.
- A scenario labelled `Paper` cannot be submitted to a live-configured broker.
- Every broker/wallet/marketplace action writes a `safety_audit` row.
- Dashboard renders the pause state and venue labels inline (no popup, per `CLAUDE.md`).

**Depends on:** `v2b-dashboard-auth-boundary` (pause/resume is a
mutating endpoint that needs auth). Holds dispatch until that PR merges.

## Sequencing

1. **Solo:** `v2b-dashboard-auth-boundary` lands first. Defines the
   auth surface the other two tracks depend on for user/source recording
   and for the pause/resume gate.
2. **Parallel fan-out (after #1 merges):** `v2b-remote-cli-job-safety`
   and `v2b-broker-wallet-kill-switch` are independent of each other —
   different code paths, no shared files except the audit-log convention
   (which they don't share a table for).

## Verification (when contracts land)

For each track:
- `cargo fmt --all -- --check`
- `cargo clippy -p <touched crates> -- -D warnings`
- Track-specific test files (named per contract)
- `pnpm --dir frontend/web typecheck` for any TS changes
- No popups (`/CLAUDE.md` frontend rule)
- Manual smoke against the xvn Tailscale node before merge — security
  work especially needs a live confirmation that auth gates fire and
  pause/resume actually halts the executor.

## Out of scope (deferred)

- ACPX backend open items (F21). Different surface; doesn't block V2B
  exit checks.
- Full marketplace transaction signing flow. Lives in V2C; this track
  only lands the gate (pause/resume) that V2C's signing flow must
  honour.
- Mainnet runbook. V4 work.

## Operator decision points before dispatch

The conductor should confirm one decision before dispatching the agents:

1. **Auth model.** The default proposal is a simple session-token model
   (issued at dashboard launch, stored in a secure HttpOnly cookie,
   bearer for CLI/HTTP). Operator may prefer OAuth/OIDC integration
   instead — that changes the foundation contract's scope materially.
   If the answer is "session token for now, OIDC later," the contract
   ships as scoped here.

If the operator approves, dispatch the foundation track solo, then fan
out the remaining two once it merges.
