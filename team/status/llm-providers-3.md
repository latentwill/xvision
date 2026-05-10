---
track: llm-providers-3
worktree: /Users/edkennedy/Code/xvision/.worktrees/llm-providers-3
branch: feature/llm-providers-phase-3-registry
phase: phase-b-llm-providers-phase-3-registry
last_updated: 2026-05-10T18:45:00Z
owner: claude-opus session 3 (sixth claim — PRs #6/#8/#11/#14/#16 merged)
---

# What I'm doing right now

Phase 3 Tasks 9–10 of the LLM Providers plan: the `ProviderRegistry` skeleton
in `xvision-eval`. Purely additive — no existing API changes. T11/T12 (which
do change `run_ab_compare`'s signature) deferred to a follow-up PR.

## Plan task progress

- [ ] T9 ProviderRegistry struct + intern_backend resolver + 2 failure tests
- [ ] T10 trader_backend resolver + 2 memoization tests
- [ ] PR open + merge

# Blocked on

Nothing.

# Followup available

- Phase 3 Task 11 — Wire `ProviderRegistry` into `run_ab_compare` (signature change)
- Phase 3 Task 12 — Swap CLI ab_compare to v2
- Phase 4 — `xvn provider` CLI subcommand (4 tasks)
- Phase 5 — UI design lock (4 tasks)
- Plan 2a remaining (2A.A MCP server, 2A.B verbs, 2A.C tool dispatch, 2A.E polish)
- Plan 2d (Dashboard + Wizard)
