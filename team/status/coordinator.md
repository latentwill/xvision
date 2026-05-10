---
track: coordinator
worktree: /Users/edkennedy/Code/xvision (main)
branch: main
phase: phase-a-bootstrap
last_updated: 2026-05-10T00:00:00Z
---

# What I'm doing right now

Bootstrapping the team coordination infrastructure (this directory, manifest,
briefings) and pre-creating the three Phase A worktrees (engine-api,
broker-surface, frontend-foundation). After bootstrap, this session takes the
engine-api track to drive the critical path forward.

# Blocked on

Nothing yet.

# Next up

1. Pre-create worktrees (`engine-api`, `broker-surface`, `frontend-foundation`).
2. Switch into `.worktrees/engine-api` and start Phase 1 of the Engine API
   Foundation plan (sqlx + migration 001).
3. Drop a queue message announcing the team layout so any joining CLI knows
   where to look.

# Tracks ready for external CLI pickup

- `broker-surface` — independent, can start immediately
- `frontend-foundation` — independent for Phase A scope; waits for queue
  signal before Phase B

The operator should `cd .worktrees/<track> && claude` to spawn additional CLI
sessions on those tracks.
