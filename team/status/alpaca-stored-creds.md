---
track: alpaca-stored-creds
worktree: /Users/edkennedy/Code/xvision (main worktree)
branch: feature/alpaca-stored-creds
phase: phase-a-implementation
last_updated: 2026-05-11T03:46:06Z
owner: claude-opus-4-7 (1M ctx) — post-v1-gaps UX
---

# What I'm doing right now

Persisting Alpaca paper credentials so operators don't have to
re-`export APCA_*` every shell session. Engine + dashboard + frontend
in one PR.

## Plan task progress

- [x] Engine: persist `~/.xvn/secrets/brokers.toml` (mode 0600)
- [x] Engine: `set_alpaca` / `clear_alpaca` / `load_alpaca_credentials` +
  audit logging; redacted summary in `get` response
- [x] Engine: `api::eval::run` prefers stored creds, falls back to env
- [x] Dashboard: `POST` / `DELETE /api/settings/brokers/alpaca`
- [x] Frontend: `AlpacaBrokerCard` with key/secret/base-url form
- [x] Tests: 7 engine + 4 dashboard route, all green
- [x] `tsc -b` + `vite build` + `cargo build --workspace` clean
- [ ] Commit + PR + pr-open queue note

# Blocked on

Nothing.
