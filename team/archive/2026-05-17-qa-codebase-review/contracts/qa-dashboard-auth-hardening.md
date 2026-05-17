---
track: qa-dashboard-auth-hardening
lane: integration
wave: qa-2026-05-17
worktree: .worktrees/qa-dashboard-auth-hardening
branch: task/qa-dashboard-auth-hardening
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/server.rs
  - crates/xvision-dashboard/src/lib.rs
  - crates/xvision-dashboard/src/auth.rs
  - crates/xvision-dashboard/src/routes/cli.rs
  - crates/xvision-dashboard/src/cli_jobs/runner.rs
  - crates/xvision-dashboard/src/cli_jobs/allowlist.rs
  - crates/xvision-dashboard/src/routes/settings/danger.rs
  - crates/xvision-engine/src/api/settings/danger.rs
  - frontend/web/src/api/settings.ts
  - frontend/web/src/routes/settings/danger.tsx
  - crates/xvision-dashboard/tests/auth.rs
  - crates/xvision-dashboard/tests/cli_jobs_allowlist.rs
  - crates/xvision-dashboard/tests/cli_jobs_routes.rs
  - crates/xvision-dashboard/tests/danger_challenge.rs
  - crates/xvision-dashboard/tests/http.rs
  - docs/runbook/dashboard-auth.md
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-cli/**
  - frontend/web/src/routes/eval-runs.tsx
  - frontend/web/src/routes/home.tsx
interfaces_used:
  - "axum::middleware — auth layer wrapper"
  - "xvision-dashboard::cli_jobs — job submission/runner"
  - "xvision-engine::api::settings::danger — wipe_db, factory_reset"
parallel_safe: false
parallel_conflicts: []
verification:
  - cargo build -p xvision-dashboard
  - cargo build -p xvision-engine
  - cargo test -p xvision-dashboard
  - cargo test -p xvision-engine api::settings::danger
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run settings
  - pnpm --dir frontend/web build
acceptance:
  - "An auth layer gates the dashboard HTTP surface; when the server binds to a non-loopback address, every privileged route requires a configured shared secret (header or query token) or fails with 401"
  - "`/api/cli/jobs` accepts only jobs from a maintained allowlist of safe templates (e.g. `bars fetch`). Unknown subcommands return 400 with a clear error; `fire-trade`, provider/secret mutation, destructive settings, and `dashboard`/`mcp` subcommands are explicitly blocked"
  - "A local-only developer-mode flag (env var or settings field) can opt back into the legacy permissive argv behavior, but is OFF by default and never bypasses the non-loopback auth gate"
  - "Danger routes (`wipe_db`, `factory_reset`) no longer accept the static `yes-i-am-sure` token from the frontend bundle. Instead, the backend issues a short-lived challenge (or requires the operator-typed phrase verbatim, verified server-side) and the frontend must produce a matching response"
  - "Frontend `danger.tsx` operator-entered phrase is the value sent to the backend (no static constant in `api/settings.ts`)"
  - "Tests cover: (a) loopback-bound server still works without auth header; (b) non-loopback bind rejects unauth'd requests with 401; (c) `/api/cli/jobs` accepts allowlisted argv and rejects denied argv; (d) danger challenge round-trip succeeds with the right phrase and fails with the wrong one"
  - "`docs/runbook/dashboard-auth.md` documents how to configure the shared secret for a non-loopback deployment"
---

# Scope

Implements remediation step 3 of `qa/2026-05-17-comprehensive-codebase-review.md`,
combining the two related findings:

- **P1 — `/api/cli/jobs` can execute high-impact `xvn` commands** when the
  server is exposed: replace the argv denylist with an allowlist of safe
  job templates, gate the surface behind auth on non-loopback binds.
- **P2 — danger routes rely on a frontend-embedded confirm token**: replace
  the static token with either operator-typed phrase verification
  server-side OR a short-lived server-issued challenge, and require
  auth on non-loopback binds.

Both findings share the same fix surface (a dashboard auth layer in
`server.rs`), so they ship together. Single-writer claim on
`crates/xvision-dashboard/src/{server,lib}.rs` is re-asserted by this track
(previously released after `q15-tailscale-serve-api-reachability`
deferred).

# Out of scope

- A real identity/RBAC system. A shared-secret header (configurable via
  settings or env) is enough for this wave; richer auth lands in V2B+.
- TLS termination / reverse-proxy guidance beyond a brief runbook note.
- Refactoring the `cli_jobs` runner's process-spawning machinery — only
  the input validation / allowlist surface changes here.
- Engine-side authorization beyond the existing `wipe_db` / `factory_reset`
  confirm gate (those two are the highest-impact endpoints and the focus of
  this track).
- Operator-tour or onboarding copy updates (out of wave; UX-polish lane).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-dashboard-auth-hardening \
  -b task/qa-dashboard-auth-hardening origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
git -C .worktrees/qa-dashboard-auth-hardening status
```

Before claiming, confirm `crates/xvision-dashboard/src/{server,lib}.rs`
remain released in `team/CONFLICT_ZONES.md`. After claiming, push a
conductor PR that re-asserts the claim and bump status to `claimed`.

# Notes

Implementation hints (do not rewrite the contract — use as starting points):

- The default bind is `127.0.0.1:8788`. The auth gate should be a no-op for
  loopback in default config so local dev stays frictionless. Only the
  non-loopback path requires the configured secret.
- The CLI allowlist belongs in a small new module (`cli_jobs/allowlist.rs`)
  with a hard-coded list of `(subcommand, argv-shape)` pairs. Start with
  what the UI actually calls today (bars fetch is the obvious one).
  Anything else, frontend updates over time as new safe templates are
  added.
- For the danger challenge, prefer the operator-typed phrase variant — the
  reviewer's primary concern is that the static token is meaningless when
  shipped in the bundle. A short-lived challenge is fine, but typed-phrase
  is simpler and the UX is already there.
- `dashboard` and `mcp` are already rejected today; preserve those rejects.
