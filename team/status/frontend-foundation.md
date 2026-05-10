---
track: frontend-foundation
worktree: /Users/edkennedy/Code/xvision/.worktrees/frontend-foundation
branch: feature/frontend-foundation
phase: phase-a-pr-open
last_updated: 2026-05-10T15:50:00Z
owner: claude-opus session 2
pr: https://github.com/latentwill/xvision/pull/7
---

# What I'm doing right now

Phase A of frontend Plan 1 landed in PR #7. Scope:

- New `xvision-dashboard` axum crate (`/api/health`, `/`, `/assets/*`,
  `/api/* → JSON 404`, SPA fallback for non-API paths via rust-embed).
- New `frontend/web/` Vite + React 18 + TS 5.5 + Tailwind 3.4 app.
- Folio dark tokens ported from `frontend/prototype/styles.css`.
- App shell — Sidebar, Topbar, ChatRailPlaceholder, Layout — and primitives
  (Icon, Pill, Dot, Card).
- Phase A route stubs for /, /strategies, /authoring, /eval-runs,
  /eval-runs/:id, /eval-runs/compare, /setup, and /settings/{providers,
  brokers, daemon, identity, danger}.
- `xvn dashboard serve [--bind]` CLI subcommand (default 127.0.0.1:8788).

## Verified

- `cargo build --workspace` green.
- `cargo test -p xvision-dashboard` — 2/2 (`/api/health`, `/api/* → 404`).
- `pnpm install && pnpm typecheck && pnpm build` green.
- Live smoke: `xvn dashboard serve` → curl confirmed `/api/health`,
  `/`, `/strategies` (deep-link fallback), `/favicon.svg`,
  `/api/nonexistent` (JSON 404).

# Blocked on

Phase B (ts-rs codegen, `/api/strategies` route, page wired to TanStack
Query, Vitest+Playwright). Blocked on engine-api PR #4 merging — that's
where the API request/response types live that ts-rs will export.

# Next up

1. Watch queue for engine-api merge or any review feedback on PR #7.
2. Once #4 lands on main: rebase `feature/frontend-foundation` onto main,
   start Phase B Tasks 6–10 from the plan.
3. Open Phase B PR `feat(frontend): /strategies vertical slice wired to engine api`.
