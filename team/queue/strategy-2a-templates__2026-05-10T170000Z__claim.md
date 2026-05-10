---
from: strategy-2a-templates
to: all
topic: claim
created_at: 2026-05-10T17:00:00Z
ack_required: false
---

# `strategy-2a-templates` track claimed (slice of B.2)

A Claude CLI session (session 3 — formerly docker-image, then leverage-items)
is taking a clean slice of Plan 2a. Worktree
`.worktrees/templates`, branch `feature/strategy-2a-templates`. Plan slice:
[`docs/superpowers/plans/2026-05-08-strategy-engine-2a-mcp-tools-templates.md`](../../docs/superpowers/plans/2026-05-08-strategy-engine-2a-mcp-tools-templates.md)
**Phase 2A.D only — Tasks 13–20**.

## Scope

The 7 v1 strategy templates plus the registry wiring:

- T13 `trend_follower` (EMA crossover trend)
- T14 `breakout` (Donchian(20) with volume confirmation)
- T15 `momentum` (MACD + ADX)
- T16 `range_trade` (Bollinger %B)
- T17 `scalping` (1m/5m EMA, fee-aware)
- T18 `news_trader` (stub — operates on price action only; real news tool in Plan 2c)
- T19 `custom` (single-LLM freeform; trader_slot only)
- T20 register all 7 + `ma_crossover_baseline` in `templates/registry.rs`; integration test

## What's NOT in this PR (deferred)

- Phase 2A.A — MCP server skeleton
- Phase 2A.B — Authoring MCP verbs
- Phase 2A.C — Tool-call dispatch in agent loop
- Phase 2A.E — Polish + smoke

These can be picked up by another session in parallel; templates are
independent of the MCP and dispatch work and have immediate user-facing value
(`xvn strategy create --from <template>`).

## Files this track touches (no overlap with active tracks)

Inside `crates/xvision-engine/`:
- `src/templates/trend_follower.rs` (new)
- `src/templates/breakout.rs` (new)
- `src/templates/momentum.rs` (new)
- `src/templates/range_trade.rs` (new)
- `src/templates/scalping.rs` (new)
- `src/templates/news_trader.rs` (new)
- `src/templates/custom.rs` (new)
- `src/templates/mod.rs` (add 7 mod declarations)
- `src/templates/registry.rs` (extend `registry()` to include the 7 + `ma_crossover_baseline`)
- `tests/seven_templates.rs` (new — integration test)
- `team/MANIFEST.md` (add row to Phase B in-flight table)
- `team/status/strategy-2a-templates.md` (new)
- `team/queue/strategy-2a-templates__*` (this message)

Zero conflict with:
- `frontend-foundation` Phase B — touches `crates/xvision-cli/`, `crates/xvision-dashboard/`, `frontend/web/`, `crates/xvision-engine/src/api/strategy.rs` (api integration, not templates)
- `eval-engine` Phase 3.A — touches `crates/xvision-engine/migrations/002_eval.sql`, `src/eval/`
- `leverage-items` Items A–D (PR #8 open) — pure docs

## Why this slice

`xvn strategy create --from <template>` is a v1 QA test surface. Once these 7
templates ship, QA can author and backtest a strategy without writing any
Rust. That's the smallest meaningful expansion of v1 user-facing capability.
