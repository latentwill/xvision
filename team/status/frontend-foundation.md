---
track: frontend-foundation
worktree: /Users/edkennedy/Code/xvision/.worktrees/frontend-foundation
branch: feature/frontend-foundation-phase-b
phase: phase-b-pr-open
last_updated: 2026-05-10T16:35:00Z
owner: claude-opus session 2
pr: https://github.com/latentwill/xvision/pull/9
---

# What I'm doing right now

Phase B of frontend Plan 1 landed in PR #9 (Phase A PR #7 merged earlier).
This closes the Frontend Plan 1 vertical slice end-to-end.

## What's in PR #9

Backend:
- `xvision-engine` `ts-export` feature gates `ts-rs` 9.x; `StrategySummary`
  derives `TS` and exports to `frontend/web/src/api/types.gen/`.
- New `xtask` workspace member: `cargo xtask gen-types` automates the
  ts-rs export run + barrel rebuild.
- `xvision-dashboard` `AppState` (sqlx pool + xvn_home), opens
  `<XVN_HOME>/xvn.db` create-if-missing, runs engine `001_api_audit.sql`.
- `GET /api/strategies` thin axum handler over
  `engine::api::strategy::list`.
- `DashboardError: From<ApiError>` — semantic mapping to HTTP statuses.
- `xvn dashboard serve` resolves XVN_HOME from `--home` / env / ~/.xvn,
  builds AppState once, threads via `Router::with_state`.

Frontend:
- `src/api/client.ts` typed `apiFetch<T>` with `ApiError(status, code, message)`.
- `src/api/strategies.ts` typed `listStrategies()` + `strategyKeys` for
  TanStack Query.
- `routes/strategies.tsx` real screen — filter bar, action buttons (both
  disabled until later plans), four states (loading skeleton / empty /
  error with code+message + Retry / populated table).

## Verified

- `cargo build --workspace` green.
- `cargo test -p xvision-dashboard` 4/4.
- `pnpm typecheck && pnpm build` green.
- Live smoke (XVN_HOME=tmpdir, port 8801): empty list returned cold; after
  seeding `bundles/<id>.json`, the list returns the bundle's agent_id +
  template; SPA `/strategies` deep link serves index.html for React Router.

# Blocked on

Nothing structural. Open work: no E2E (Playwright) tests yet — Plan 1
Task 13 deferred. Filter bar + action buttons are placeholder until
later plans add the backend filtering and authoring routes.

# Next up

Frontend Plan 1 is done after PR #9 merges. Plan 2 (Read-only screens —
Home, Eval runs, Run detail without findings, Settings) is the natural
next track. Plan 2's "soft" prereq is the eval-engine plan landing
`eval_runs` / `eval_events`; until then Plan 2's eval routes return empty
arrays and the UI shows "No runs yet — backend not ready". Session 1 has
already claimed eval-engine 3.A (per
`eval-engine__2026-05-10T073821Z__claim.md`).
