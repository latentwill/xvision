---
from: strategy-2a-templates
to: all
topic: phase-b-pr-open
created_at: 2026-05-10T17:25:00Z
ack_required: false
---

# Strategy 2a Phase 2A.D (templates) — PR #11 open

PR: https://github.com/latentwill/xvision/pull/11
Branch: `feature/strategy-2a-templates` (rebased onto current `main`)
Worktree: `.worktrees/templates`

## What landed

Seven v1 strategy templates + registry + integration test, following the
`mean_reversion` pattern already on `main`:

- `trend_follower` (EMA 12/26/50 trend)
- `breakout` (Donchian(20) + volume confirmation)
- `momentum` (MACD + ADX(14))
- `range_trade` (Bollinger %B oscillator)
- `scalping` (1m/5m EMA, fee-aware conviction)
- `news_trader` (price-action stub; news_sentiment tool deferred to Plan 2c)
- `custom` (single-LLM-agent freeform; trader_slot only)

Plus `ma_crossover_baseline` registered as a marketplace seed listing.

## Verification

- `cargo test -p xvision-engine --test seven_templates` — 3/3 pass
- `cargo test --workspace` — green, no regressions
- `cargo build --workspace` — green

## Followup available for next session

The remaining slices of Plan 2a are unclaimed and can ship in parallel — they
don't touch the template files (so safe to land before or after this PR):

- **Phase 2A.A** — MCP server skeleton (`crates/xvision-engine/src/mcp/`)
- **Phase 2A.B** — Six authoring MCP verbs (skill_register, bundle_clone, etc.)
- **Phase 2A.C** — Tool-call dispatch in `LlmDispatch::execute_slot` (Anthropic-style tool-use loop)
- **Phase 2A.E** — Polish + smoke (README + integration smoke)

Also worth picking up after frontend Phase B settles:
- **Item E.1 of leverage-items plan** — `xvn eod` CLI command. Plan provides ready-made test scaffold + implementation. Skipped on PR #8 to avoid CLI conflicts with frontend Phase B.

## No file overlap with active tracks

- `frontend-foundation` Phase B (PR #9): `crates/xvision-cli/`, `crates/xvision-dashboard/`, `frontend/web/`, `crates/xvision-engine/src/api/strategy.rs`
- `eval-engine` Phase 3.A: `crates/xvision-engine/migrations/002_eval.sql`, `src/eval/`
- `leverage-items` (PR #8): pure docs

This track touched only `crates/xvision-engine/src/templates/*` + `tests/seven_templates.rs` + `team/`.
