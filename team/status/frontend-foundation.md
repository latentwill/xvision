---
track: frontend-foundation
worktree: /Users/edkennedy/Code/xvision/.worktrees/frontend-foundation
branch: feature/frontend-foundation
phase: phase-a-scaffolding
last_updated: 2026-05-10T15:25:00Z
owner: claude-opus-coordinator (session 2 — picked up from coordinator)
---

# What I'm doing right now

Picking up frontend-foundation Phase A (scaffolding only). Engine-api just
finished Phase 5 in `.worktrees/engine-api` (last commit 86c5677 "docs(engine):
README for api module pattern"); the other CLI presumably opens that PR shortly.

Phase A scope per briefing:
1. xvision-dashboard crate scaffold (Cargo.toml, lib.rs, error.rs, server.rs
   with /api/health stub).
2. frontend/web Vite + Tailwind scaffold.
3. Port Folio dark tokens from `frontend/prototype/styles.css`.
4. App shell — Sidebar + Topbar + route stubs (visuals only).
5. `xvn dashboard serve [--bind]` CLI subcommand.

# Blocked on

Nothing for Phase A. Phase B (ts-rs codegen, /api/strategies wired) waits on
engine-api landing — I will pick that up after seeing the
`engine-api__*__phase-a-complete.md` queue message.

# Next up

1. Move into `.worktrees/frontend-foundation`.
2. TDD Task 1 (crate scaffold + /api/health passing test).
3. Task 2 (frontend/web scaffold) → Task 3 (tokens) → Task 4 (shell) → Task 5 (CLI).
4. Open PR `feat(frontend): scaffold xvision-dashboard crate + web app shell`.
5. Post `frontend-foundation__<utc>__phase-a-complete.md`.
