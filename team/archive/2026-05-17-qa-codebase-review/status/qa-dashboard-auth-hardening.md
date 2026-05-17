---
track: qa-dashboard-auth-hardening
status: pr-open
last_update: 2026-05-17
worker: Claude (xvision conductor session)
pr: 237
commits:
  - (single commit) qa: dashboard auth gate + cli allowlist + danger typed phrases
---

## Outcome

PR #237 open. Branch pushed. Integration-lane PR covering three
remediation steps from QA finding #3.

## What changed

| Area | Files |
|---|---|
| Auth gate | `crates/xvision-dashboard/src/auth.rs` (new), `src/server.rs`, `src/lib.rs` |
| CLI allowlist | `crates/xvision-dashboard/src/cli_jobs/allowlist.rs` (new), `src/cli_jobs/mod.rs`, `src/routes/cli.rs` |
| Danger phrases | `crates/xvision-engine/src/api/settings/danger.rs`, `frontend/web/src/api/settings.ts`, `frontend/web/src/routes/settings/danger.tsx` |
| Tests | `crates/xvision-dashboard/tests/{auth,cli_jobs_allowlist,danger_challenge}.rs` (new) |
| Test plumbing | `crates/xvision-dashboard/tests/cli_jobs_routes.rs` (devmode armed), `tests/http.rs` (phrases updated) |
| Runbook | `docs/runbook/dashboard-auth.md` (new) |

## Verification

| Suite | Result |
|---|---|
| `cargo test -p xvision-engine --lib api::settings::danger` | 8/8 pass (incl. legacy-token-rejected + cross-route-phrase-rejected) |
| `cargo test -p xvision-dashboard --lib` | 13 lib tests pass (auth + allowlist matrices, env-mutex serialized) |
| `cargo test -p xvision-dashboard --test auth` | 4/4 |
| `cargo test -p xvision-dashboard --test cli_jobs_allowlist` | 5/5 |
| `cargo test -p xvision-dashboard --test danger_challenge` | 4/4 |
| `cargo test -p xvision-dashboard --test cli_jobs_routes` | 11/11 (devmode armed for `help`/`slow` fake-cli) |
| `cargo test -p xvision-dashboard --test http` | 52/52 (4 pre-existing failures on origin/main excluded: 3 scenario-create deser mismatches, 1 eval-compare report — verified to reproduce on clean main checkout) |
| `pnpm --dir frontend/web typecheck` | clean |
| `pnpm --dir frontend/web build` | clean |

## Contract amendments (conductor-side)

- Added `crates/xvision-dashboard/tests/cli_jobs_routes.rs` and `tests/http.rs` to `allowed_paths` — both existing test suites needed updates to live with the new contracts (devmode env + per-route phrases). Documented in OWNERSHIP.md is not needed since the amendments are within the contract's own intent.

## Out-of-scope reminders honored

- No new identity / RBAC system.
- No TLS termination logic beyond a runbook pointer at reverse proxies.
- No refactor of cli_jobs runner machinery.
- No engine-side auth beyond the typed-phrase verification this contract introduces.
- No frontend lint script invocation (no `lint` script in `frontend/web/package.json` on main; not a regression).

## Ready for

PR review. Stacked-on-main, no other PR dependency. Merge order: any
time — independent of other operator-wave PRs.
