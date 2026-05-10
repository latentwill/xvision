---
track: frontend-2-settings
worktree: /Users/edkennedy/Code/xvision/.worktrees/frontend-2-settings
branch: feature/frontend-2-settings
phase: in-progress
last_updated: 2026-05-10T17:30:00Z
owner: claude-opus session 2
---

# What I'm doing right now

Frontend Plan 2 Settings sub-slice — read-only Settings tabs for brokers,
daemon, identity (Tasks 6, 7, 8 + frontend-side of Task 16). No config
mutations in this PR.

Scope (intentionally narrow to avoid conflict with concurrent llm-providers
work):
- Backend `engine::api::settings::{brokers,daemon,identity}` — read-only
  GETs returning current config + env-var presence (without exposing
  secret values).
- Dashboard routes `/api/settings/{brokers,daemon,identity}`.
- Frontend `routes/settings/index.tsx` — replace placeholders for those
  three tabs with real data; keep providers/danger as placeholders (those
  depend on llm-providers Phase 2+ landing).
- ts-rs derives + xtask gen-types regenerate.

# Blocked on

Nothing structural. Plan 2 Tasks 5/15 (providers CRUD) and Task 9 (danger
zone wipe) are NOT in this PR — those depend on llm-providers Phases 2+.

# Next up

1. Backend api/settings/ module + three GET routes.
2. Dashboard route registration + integration tests.
3. Frontend api/settings.ts fetchers + render in three tabs.
4. cargo xtask gen-types.
5. Open PR.
