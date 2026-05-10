---
track: llm-providers-4
worktree: /Users/edkennedy/Code/xvision/.worktrees/llm-providers-4
branch: feature/llm-providers-phase-4-list-show
phase: phase-b-llm-providers-phase-4-t13
last_updated: 2026-05-10T19:48:00Z
owner: claude-opus session 3 (eighth claim — PRs #6/#8/#11/#14/#16/#20/#22 merged)
---

# What I'm doing right now

Phase 4 Task 13 of Plan #7: the read-only `xvn provider list` + `xvn provider
show` subcommands. T14 (add/remove with toml_edit), T15 (check with TCP+/models
probe), and T16 (cache divergence test in xvision-eval) deferred to follow-up
PRs.

## Plan task progress

- [ ] T13 `xvn provider list` + `xvn provider show` (skeleton + stubs for T14/T15)
- [ ] PR open + merge

# Blocked on

Nothing.

# Followup available

- **Plan #7 Phase 4 T14** — `xvn provider add` / `remove` (in-place TOML mutation)
- **Plan #7 Phase 4 T15** — `xvn provider check` (TCP-connect + `--probe`)
- **Plan #7 Phase 4 T16** — `cache_diverges_on_intern_model_change` test
- **Plan #7 Phase 5** — UI design lock (4 doc tasks)
- **Plan 2a** — 2A.A MCP server skeleton, 2A.B verbs, 2A.C tool dispatch, 2A.E polish
- **Plan 2d** — Dashboard + Wizard
