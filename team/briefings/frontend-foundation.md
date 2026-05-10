# Briefing — `frontend-foundation` track

You are working on **Frontend Plan 1: Foundation + Strategies vertical slice**, but
**Phase A scope is limited to scaffolding only** — the parts that don't depend on
`xvision_engine::api::strategy::*` (which the engine-api track is producing in
parallel).

## Plan

[`docs/superpowers/plans/2026-05-10-frontend-1-foundation-and-strategies.md`](../../docs/superpowers/plans/2026-05-10-frontend-1-foundation-and-strategies.md)

## Why split this way

Frontend Plan 1 has ~10 tasks. The first half (Vite/Tailwind/Tokens/Router/Layout
shell) doesn't need any API; the second half (`/strategies` data fetching with
TanStack Query) does. We start the first half now in parallel with engine-api,
then resume after engine-api merges.

## Skills required

- `superpowers:executing-plans`
- `frontend-design:frontend-design` — the user wants distinctive, polished UI; no generic AI aesthetics
- `superpowers:test-driven-development` (where it makes sense for FE — Vitest unit, Playwright e2e)
- `superpowers:verification-before-completion`

## Phase A scope (this track, this phase)

- [ ] Task 1 — Crate scaffold: `crates/xvision-dashboard/` Cargo.toml, lib.rs, error.rs (server.rs left as a stub returning 200 from `/api/health`)
- [ ] Task 2 — `frontend/web/` scaffold: package.json, vite.config.ts, tailwind.config.ts, tsconfig.json, postcss.config.js, .gitignore, index.html, src/main.tsx + minimal `<App/>` rendering "xvision" title
- [ ] Task 3 — Port the prototype's Folio dark tokens (typography, colors, spacing) from `frontend/prototype/` into `frontend/web/src/styles/tokens.css` + Tailwind theme extension
- [ ] Task 4 — App shell: top nav, route tabs (Setup/Strategies/Eval/Settings), env pill, user menu — visuals only, routes are stubs
- [ ] Task 5 — `xvn dashboard serve` CLI subcommand that starts axum on `localhost:8788` and serves the embedded SPA

## Phase B scope (this track, AFTER engine-api lands)

- [ ] Task 6 — `ts-rs` codegen: emit TypeScript types for `xvision_engine::api::strategy::StrategySummary` and friends to `frontend/web/src/types/api.ts`
- [ ] Task 7 — `GET /api/strategies` route in `crates/xvision-dashboard/src/routes/strategies.rs`
- [ ] Task 8 — Strategies page wired to real API via TanStack Query
- [ ] Task 9 — Empty state, loading state, error state per design tokens
- [ ] Task 10 — Vitest + Playwright happy-path test

## What you do NOT do

- ❌ Authoring inspector, slot editor, lineage tree — Plan 3.
- ❌ Wizard, chat rail — Plan 4.
- ❌ Findings, compare, command palette — Plan 5.
- ❌ Auth — explicit non-goal in v1 (localhost trust model).

## Branch / worktree

- Worktree: `.worktrees/frontend-foundation`
- Branch: `feature/frontend-foundation`
- PR title (Phase A): `feat(frontend): scaffold xvision-dashboard crate + web app shell`
- PR title (Phase B): `feat(frontend): /strategies vertical slice wired to engine api`

## Cross-track contracts

You **consume** from engine-api (in Phase B):
- `xvision_engine::api::strategy::list(ctx) -> Vec<StrategySummary>`

You **produce**:
- `xvn dashboard serve [--port 8788]` CLI subcommand
- `crates/xvision-dashboard/` axum crate
- `frontend/web/` Vite app

Watch the queue for `engine-api__*__phase-a-complete.md` — that's your green
light to start Phase B tasks.

## Tips

- **Read `frontend/DESIGN.md` first.** It's the master design doc.
- **Read `frontend/prototype/`** — that's the visual source of truth. Port tokens,
  don't reinvent them.
- pnpm 9 is the package manager. Don't use npm/yarn.
- `rust-embed` 8.x. Check the workspace Cargo.toml — if not yet a dep, add it.
- `ts-rs` 9.x. Same — add to workspace deps if missing.
- The `xtask/` workspace member doesn't exist yet — you'll create it.

## Completion definition

**Phase A done when:**
- `cargo build -p xvision-dashboard` green.
- `cd frontend/web && pnpm install && pnpm build` green.
- `xvn dashboard serve --port 8788` returns 200 from `/api/health` and serves
  the SPA shell at `/`.
- PR opened.
- Queue message `frontend-foundation__*__phase-a-complete.md` posted.

**Phase B done when:**
- `/strategies` page renders real data from a running engine.
- Vitest + Playwright tests green.
- Phase-B PR opened.
