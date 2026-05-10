---
track: llm-providers
worktree: /Users/edkennedy/Code/xvision/.worktrees/llm-providers
branch: feature/llm-providers-phase-1
phase: phase-b-llm-providers-phase-1
last_updated: 2026-05-10T18:00:00Z
owner: claude-opus session 3 (fourth claim — docker-image #6, leverage-items #8, templates #11 all merged)
---

# What I'm doing right now

Phase 1 (config schema) of the LLM providers + per-arm models plan. Pure
`xvision-core` work — no CLI/engine/API/MCP changes. Independent of all
currently-active sessions.

## Plan task progress

- [ ] T1: `ProviderEntry` + `ProviderKind` types (kebab-case)
- [ ] T2: `providers` vec on `RuntimeConfig` with `#[serde(default)]`
- [ ] T3: Auto-derive `_default_intern` synthetic row + uniqueness validation
- [ ] T4: Update `config/default.toml` with explicit `[[providers]]` rows
- [ ] PR open

# Blocked on

Nothing.

# Followup available for next session

Phases 2–5 of this plan can ship in parallel:
- Phase 2 — `SlotRef` newtype + `ArmKind::Trader` extension
- Phase 3 — `ProviderRegistry` + `run_ab_compare` wiring
- Phase 4 — `xvn provider` CLI subcommand
- Phase 5 — UI design lock

Also still open from earlier slices:
- Plan 2a Phases 2A.A/2A.B/2A.C/2A.E (MCP server + tool dispatch)
- Plan 2d (Dashboard + Wizard) — gates B.6/B.7/B.8
- Frontend Plans 3/4/5 — gated by Plan 2 (in flight by session 2)
- Eval Engine Phases 3.C/3.D/3.E (after eval-engine-3b lands)
