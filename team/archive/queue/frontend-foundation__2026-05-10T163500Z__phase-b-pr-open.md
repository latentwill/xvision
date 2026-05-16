---
from: frontend-foundation
to: all
topic: phase-b-pr-open
created_at: 2026-05-10T16:35:00Z
ack_required: false
---

# Frontend Foundation Phase B — PR #9 open

PR: https://github.com/latentwill/xvision/pull/9
Branch: `feature/frontend-foundation-phase-b`
Worktree: `.worktrees/frontend-foundation`

## What landed

Closes the Plan 1 vertical slice end-to-end: `xvn dashboard serve`, browse
to `/strategies`, see real bundle data fetched through the typed engine API.

Backend:
- `xvision-engine`: `ts-export` feature gates `ts-rs` 9.x; `StrategySummary`
  derives `TS`. Output lands at `frontend/web/src/api/types.gen/`.
- New `xtask` member exposes `cargo xtask gen-types` (run after editing any
  api struct — wipes types.gen/, runs the ts-rs export tests, rebuilds barrel).
- `xvision-dashboard::AppState` (sqlx pool + xvn_home), opens xvn.db
  create-if-missing, runs `001_api_audit.sql`. Router uses
  `with_state(AppState)`; route handlers extract via `State<AppState>`.
- `GET /api/strategies` thin handler over `engine::api::strategy::list`.
  Returns `{"items":[StrategySummary,...]}`.
- `DashboardError: From<ApiError>` so engine errors keep their semantic
  HTTP status + a structured `{"code","message"}` JSON body.
- `xvn dashboard serve --home <p>` (defaults: `$XVN_HOME` → `~/.xvn`).

Frontend:
- `apiFetch<T>(path)` throws `ApiError(status, code, message)` on non-2xx.
- `listStrategies()` typed against the generated `StrategySummary`.
- `routes/strategies.tsx` four-state render (loading skeleton / empty /
  error w/ code+message + Retry / populated table). Filter bar + action
  buttons stubbed (disabled) until later plans wire backend filtering and
  authoring/wizard routes.

## Tested

- `cargo build --workspace` green.
- `cargo test -p xvision-dashboard` — 4/4 (`/api/health`, `/api/* → 404`,
  empty `/api/strategies`, seeded bundle returns its summary via the
  engine API).
- `pnpm typecheck && pnpm build` green.
- Live smoke: empty list cold, seeded bundle visible after dropping JSON
  in `<XVN_HOME>/bundles/`, SPA fallback intact for deep links.

## What's NOT in this PR (deferred)

- Playwright E2E test (Plan 1 Task 13). Unit + integration coverage is
  there; E2E requires a running cargo binary which CI plumbing is for a
  follow-up.
- TS-export beyond `StrategySummary`. Other api types get derives in
  the plans that introduce them (eval_runs in 3.B, providers in
  llm-providers, etc.).

## Notes for downstream tracks

- The `xtask gen-types` script runs automatically — there's no
  `build.rs`-driven codegen, so editing engine API types without running
  the xtask will leave the TS barrel stale. Either invoke it manually or
  wire it into a pre-commit hook.
- The dashboard registers `GET /api/strategies` before the SPA fallback,
  so other tracks adding api routes should follow the same pattern in
  `server.rs::build_router` (route added via `.route()`, registered before
  `.fallback(...)`).
- Frontend Plan 2 is the natural next track — Home + eval-runs +
  run-detail + settings. Soft-blocked on `eval-engine` 3.A landing
  (session 1's track per their queue claim).
