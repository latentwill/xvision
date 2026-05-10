---
track: strategy-2a-templates
worktree: /Users/edkennedy/Code/xvision/.worktrees/templates
branch: feature/strategy-2a-templates
phase: phase-b-pr-open
last_updated: 2026-05-10T17:25:00Z
owner: claude-opus session 3 (fourth claim — docker-image PR #6 merged, leverage-items PR #8 open, templates PR #11 open)
---

# What I'm doing right now

PR #11 open: https://github.com/latentwill/xvision/pull/11. Seven v1 templates
+ registry + integration test landed as 8 focused commits, rebased onto
current `main`. All workspace tests green.

## Plan task progress

- [x] T13 `trend_follower`
- [x] T14 `breakout`
- [x] T15 `momentum`
- [x] T16 `range_trade`
- [x] T17 `scalping`
- [x] T18 `news_trader`
- [x] T19 `custom`
- [x] T20 register all 7 + `ma_crossover_baseline`; integration test
- [x] PR open
- [ ] Operator merge

# Blocked on

Nothing.

# Next up

1. Land 7 template files as 7 focused commits.
2. Wire registry + add integration test (T20).
3. `cargo test --workspace` confirms green.
4. Open PR `feat(engine): 7 v1 strategy templates (Plan 2a Phase 2A.D)`.
5. Post `strategy-2a-templates__<utc>__phase-b-pr-open.md` to queue.

# Followup available for next session

The remaining slices of Plan 2a:
- Phase 2A.A — MCP server skeleton
- Phase 2A.B — Authoring MCP verbs
- Phase 2A.C — Tool-call dispatch in agent loop

These are independent of the templates and can ship in parallel.
