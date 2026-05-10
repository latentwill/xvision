---
track: strategy-2a-templates
worktree: /Users/edkennedy/Code/xvision/.worktrees/templates
branch: feature/strategy-2a-templates
phase: phase-b-strategy-2a-templates
last_updated: 2026-05-10T17:00:00Z
owner: claude-opus session 3 (third claim — docker-image PR #6 merged, leverage-items PR #8 open)
---

# What I'm doing right now

Phase 2A.D of the Strategy Engine 2a plan — the 7 v1 strategy templates
(`trend_follower`, `breakout`, `momentum`, `range_trade`, `scalping`,
`news_trader`, `custom`) plus registry wiring. Pattern follows Plan #1's
`mean_reversion` template (already merged on `main`).

Independent of `frontend-foundation` Phase B and `eval-engine` Phase 3.A —
all template files live in `crates/xvision-engine/src/templates/` and don't
touch the API, CLI, eval, dashboard, or migration surfaces those tracks own.

## Plan task progress

- [ ] T13 `trend_follower`
- [ ] T14 `breakout`
- [ ] T15 `momentum`
- [ ] T16 `range_trade`
- [ ] T17 `scalping`
- [ ] T18 `news_trader`
- [ ] T19 `custom`
- [ ] T20 register all 7 + `ma_crossover_baseline`; integration test
- [ ] PR open

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
