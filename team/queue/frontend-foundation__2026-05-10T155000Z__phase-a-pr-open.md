---
from: frontend-foundation
to: all
topic: phase-a-pr-open
created_at: 2026-05-10T15:50:00Z
ack_required: false
---

# Frontend Foundation — PR #7 open

PR: https://github.com/latentwill/xvision/pull/7
Branch: `feature/frontend-foundation`
Worktree: `.worktrees/frontend-foundation`

## What landed

Phase A of frontend Plan 1 — every piece that does **not** depend on
`xvision_engine::api::*`. PR diff: 4 commits, +new crate +new web app.

1. New `xvision-dashboard` axum crate (`/api/health`, embedded SPA via
   `rust-embed`, JSON 404 on unknown `/api/*`, SPA fallback elsewhere so
   React Router owns deep links).
2. New `frontend/web/` Vite + React 18 + TS 5.5 + Tailwind 3.4 app.
   Folio dark tokens ported verbatim from `frontend/prototype/styles.css`
   into `src/styles/tokens.css` and exposed as Tailwind utilities.
3. Shell + primitives: Sidebar (NavLink-driven primary nav, branded "xvn"
   wordmark, "Setup agent" promo card, user row), Topbar (title/sub/env
   pill/⌘K), Layout (3-col grid), ChatRailPlaceholder, Icon, Pill, Dot,
   Card.
4. Phase A route stubs for `/`, `/strategies`, `/authoring`, `/eval-runs`,
   `/eval-runs/:id`, `/eval-runs/compare`, `/setup`,
   `/settings/{providers,brokers,daemon,identity,danger}`.
5. `xvn dashboard` clap subcommand with `serve --bind <addr>` (default
   `127.0.0.1:8788`).

## Verified

- `cargo build --workspace` green.
- `cargo test -p xvision-dashboard` — 2/2 (`/api/health`, `/api/* → 404`).
- `pnpm install && pnpm typecheck && pnpm build` green.
- Live smoke: `xvn dashboard serve --bind 127.0.0.1:8799` then `curl`
  confirmed `/api/health`, `/`, `/strategies` (SPA fallback), the
  embedded `favicon.svg`, and `/api/nonexistent` JSON 404.

## Independence

This PR is independent of PR #4 (engine-api) and PR #5 (broker-surface);
they can land in any order. Phase B of this track (ts-rs codegen,
`/api/strategies` route, page wired to engine API) is blocked on PR #4
merging.

## Notes for downstream tracks (post-Phase-A)

- The dashboard treats `/api/*` as a hard namespace — unknown routes 404
  with a JSON body matching `DashboardError`. Other tracks adding routes
  should register them on the axum `Router::new().route("/api/...", …)`
  before the fallback layer.
- The Vite `outDir` points at `crates/xvision-dashboard/static/`. That
  directory is gitignored except for `.gitkeep`. CI / Docker builds need
  to run `pnpm build` before `cargo build` so `rust-embed` finds assets.
- The shell uses CSS variables from `tokens.css` AND Tailwind utilities
  that map to those vars. Either form works in components; prefer
  Tailwind for new code.
