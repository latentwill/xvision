---
track: v2b-remote-cli-job-safety
lane: leaf
wave: v2b
worktree: .worktrees/v2b-remote-cli-job-safety
branch: task/v2b-remote-cli-job-safety
base: origin/main
status: ready
depends_on: []                                           # auth-boundary not yet merged — stub AuthContext per Notes
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/cli_jobs/**             # orphan recovery + cancellation + audit fields
  - crates/xvision-dashboard/src/routes/cli_jobs.rs      # DELETE endpoint kills process
  - crates/xvision-dashboard/migrations/**               # cli_job_audit columns / table
  - crates/xvision-dashboard/tests/cli_jobs_*.rs         # NEW
  - frontend/web/src/api/types.gen/**                    # ts-rs regen for new audit fields
  - frontend/web/src/features/cli-jobs/**                # surface orphan + audit state
forbidden_paths:
  - crates/xvision-dashboard/src/auth/**                 # v2b-dashboard-auth-boundary owns
  - crates/xvision-dashboard/src/routes/auth.rs          # ditto
  - crates/xvision-engine/src/api/safety/**              # v2b-broker-wallet-kill-switch owns
  - crates/xvision-execution/**                          # ditto
interfaces_used:
  - xvision_dashboard::auth::AuthContext                 # landed by v2b-dashboard-auth-boundary
  - tokio::process::Child
  - sqlx::SqlitePool
parallel_safe: true
parallel_conflicts:
  - v2b-broker-wallet-kill-switch (no shared files; both depend on auth-boundary)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-dashboard -- -D warnings
  - cargo test -p xvision-dashboard cli_jobs
  - pnpm --dir frontend/web typecheck
acceptance:
  - **Orphan recovery on dashboard restart.** On startup, scan the job table for rows in `Running` state. For each: check if the recorded PID is alive (using `procfs`/`sysinfo` or pid-file check). If not alive: transition to `Orphaned` with `recovered_at = now()` and `recovery_reason = "process_not_found"`. Stale `*.lock` files older than the job's `max_runtime_seconds` are removed.
  - **Cancellation actually kills the process.** `DELETE /api/cli/jobs/:id` sends `SIGTERM`, polls every 250ms for up to 5s, then sends `SIGKILL`. Job row transitions to `Cancelled` with `cancelled_at` and `cancel_signal` fields. CLI-side `xvn job cancel <id>` propagates.
  - **Audit fields per job.** Every job row carries: `user` (from `AuthContext.user`), `source` (`"tailscale:<node>"` or `"localhost"`), `command_class` (the verb name, e.g. `eval-run`, `strategy-new`), `started_at`, `completed_at`, `exit_code`, `output_bytes`. Persisted to a new migration or as additive columns on the existing job table.
  - **Runtime cap.** Per-job `max_runtime_seconds` (default 3600 = 1h, configurable in dashboard config). On breach, send SIGTERM and mark `RuntimeCapExceeded`.
  - **Output cap.** Per-job `max_output_bytes` (default 10 MB, configurable). On breach, truncate the output and mark `OutputCapExceeded`; the running process is killed.
  - **High-risk command allowlist + safe-eval-allowlist expansion** (folds P1 #12 from `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`). Reject any job whose command isn't a known `xvn` verb (clap-derived subcommand list). No `sh -c`, no `bash`, no arbitrary executable paths. Allowlist is hardcoded in `crates/xvision-dashboard/src/cli_jobs/allowlist.rs`, expanded beyond the current `bars fetch` to include: `eval list`, `eval show`, `eval results`, `eval watch`, `eval compare`, `eval cancel`, `strategy show`, `scenario show`, and bounded variants of `experiment run` / `model bakeoff` (the operator-safety P0 verbs that already enforce hard limits + scope caps). Document the allowlist with a comment block explaining the principle: "safe to surface remotely = read-only OR explicitly scoped + hard-limited + cancellable." Verbs that haven't met that bar stay off the allowlist.
  - **Frontend surfaces orphan + audit state.** The CLI Jobs list (or wherever it lives in `frontend/web/src/features/`) shows orphan status with an inline "Recovered" badge (no popup); the job-detail surface shows audit fields. No popups, per `CLAUDE.md`.
  - **Tests:**
    * Orphan detection on simulated restart: insert a `Running` row whose PID doesn't exist, run the recovery sweep, assert it transitions to `Orphaned`.
    * Cancellation timing: spawn a sleep-loop job, hit `DELETE`, assert the process exits within 6s and the row is `Cancelled`.
    * Output cap enforcement: run a job that writes >cap bytes, assert truncation + status flag.
    * Runtime cap enforcement: run a job that sleeps past cap, assert SIGTERM sent + status flag.
    * Allowlist rejection: attempt to create a job with `command = "sh"`, assert 400 with `CommandNotAllowed`.

---

# Scope

V2B operational hardening for the remote-CLI surface. Closes the
operational gaps the remote-CLI design doc
(`docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md`)
explicitly named at landing: orphan recovery, restart semantics,
cancellation, audit trail, and runtime/output caps.

# Out of scope

- Dashboard-API auth gate (`v2b-dashboard-auth-boundary` owns this — this contract just consumes `AuthContext`).
- Broker/wallet pause-gate (`v2b-broker-wallet-kill-switch`).
- ACPX backend items (F21) — different surface, not in V2B scope.
- Tailscale ACL changes — the application layer records the user; the network layer is operator-managed.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/v2b-remote-cli-job-safety status
git -C .worktrees/v2b-remote-cli-job-safety log --oneline -3 origin/main..HEAD

# Confirm:
#   - rebased on top of v2b-dashboard-auth-boundary's merged commit
#   - `crates/xvision-dashboard/src/auth/` exists (the AuthContext type)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/v2b-remote-cli-job-safety -b task/v2b-remote-cli-job-safety origin/main
```

# Notes

- The remote-CLI backend's current state is documented in `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md` §4 (Architecture overview) and §5 (API contract). The agent should grep the existing `crates/xvision-dashboard/src/cli_jobs/` (or wherever the existing routes live) before designing the orphan recovery — much of the schema may already exist.
- PID tracking via `tokio::process::Child::id()`; persist alongside the job row. On startup, use `sysinfo::System` to check liveness.
- For `SIGTERM/SIGKILL` cross-platform: `nix::sys::signal::kill` on Unix; the dashboard runs on macOS/Linux for now (Windows isn't in the deployment targets per CLAUDE.md).
- **`xvn eval cancel` already exists on main** (PR #425, slice 1 of `cli-operator-safety-p0`). The verb hits the existing `POST /api/eval/runs/:id/cancel` endpoint. That endpoint marks the run row but does NOT kill the backing dashboard process. This contract closes that gap: `DELETE /api/cli/jobs/:id` must actually send SIGTERM + SIGKILL to the process tracked by the `cli_jobs` machinery.
- **`mcp-eval-run-job-bridge` is on main** (commit `11959db`) — synthetic `eval_run_<ULID>` IDs resolve in `crates/xvision-dashboard/src/cli_jobs/eval_run_bridge.rs`. Do not break that path. The orphan-recovery sweep should skip synthetic eval-run bridge IDs (they have no backing process) or handle them via the bridge's terminal-state convention.
- **AuthContext stub (V2B coordination).** `v2b-dashboard-auth-boundary` lands `xvision_dashboard::auth::AuthContext` but is not yet merged. For this track: define a local `AuthContext` placeholder in `crates/xvision-dashboard/src/cli_jobs/auth_stub.rs` with the same shape (`user`, `source`, etc.) and use it for the audit columns. When auth-boundary merges, the placeholder gets deleted and the import switches over in a small follow-up PR. Note this explicitly in the PR body so the operator can pair the follow-up.
- **Operator-safety overlap.** The hard-limits work (per-run `max_decisions` / `max_*_tokens` / `max_wall_clock`) is **already on main** via PR #428. This contract's `max_runtime_seconds` and `max_output_bytes` caps live at the *dashboard process supervision* layer (different from the engine's per-run token caps). Don't re-implement those — orient the runtime cap as "the dashboard kills runaway child processes," distinct from "the engine enforces decision/token budgets at the eval level."
