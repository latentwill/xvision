# Track briefing — `xvn-eod`

**Plan:** [Leverage items](../../docs/superpowers/plans/2026-05-10-leverage-items.md), Item E.1 — `xvn eod` end-of-day report.

**Worktree:** `.worktrees/xvn-eod`
**Branch:** `feature/leverage-eod`
**Base:** `origin/main` @ `c07dfd3`

## Why this track

v1 success criterion §168: *"Reset: `xvn eod` produces a sensible markdown report from the test-session data."* That criterion is unsatisfied today — operators have no built-in summary of what happened during a test session.

## Scope adjustment from the plan

The plan's queries target post-v1.1 tables (`positions`, `decisions.stage='reject'`, `pending_reservations`, `global_state`) that come from the deferred wallet plan + Plan 2c live daemon. **None of those tables exist in v1.**

For v1, the actual session-time data lives in:

- `api_audit` (every API call — domain, operation, target, outcome, duration)
- `eval_runs` + `eval_decisions` + `eval_equity_samples` + `eval_attestations` (the eval pipeline)

The v1 cut of `xvn eod` reports against those tables and stubs out the wallet/live sections with a "(available once `xvn live` lands)" line. When Plan 2c + the wallet plan ship, the stubbed sections fill in.

## v1 report sections

1. **Header** — date, window length.
2. **Eval runs** — count by status (queued/running/completed/failed), total decisions, total trades.
3. **Per-strategy summary** — table of strategy_bundle_hash → run count, completion rate, best Sharpe, best return %.
4. **Audit activity** — top domain/operation pairs by frequency in window.
5. **Errors** — count of `outcome != 'ok'` audit rows; top error messages.
6. **Stubs** — Halt status / Positions / Reservations sections each render a one-line "available once `xvn live` lands" placeholder so the report layout stays stable when those tables arrive.

## Files this track touches

- `crates/xvision-cli/src/commands/eod.rs` (new)
- `crates/xvision-cli/src/commands/mod.rs` (alphabetical insertion `eod` between `dashboard` and `eval`)
- `crates/xvision-cli/src/lib.rs` (`Eod(commands::eod::EodArgs)` Command variant + dispatch arm)
- `crates/xvision-cli/tests/eod_cli.rs` (new — empty-state + populated-state tests)
- `crates/xvision-cli/Cargo.toml` (`tempfile` dev-dep if not already there)

## Out of scope (deferred)

- Item E.2 — scheduler registration via Plan 2c (not started).
- Wallet/live sections rendered with real data (waits for the wallet plan + Plan 2c).
- Item G — runtime agent rename (waits for the wallet plan's `strategies` table).

## Zero overlap with active sessions

- PR #27 (`xvn provider add/remove/check`) — `xvision-cli/commands/provider.rs`
- PR #28 (`xvn eval compare`) — already merged
- PR #29 (Plan #7 Phase 5 docs) — docs only
- PR #31 (`strategy-2a-mcp-authoring`) — `xvision-mcp`
- PR #32 (`BacktestExecutor`) — `xvision-engine`

`commands/mod.rs` and `lib.rs` are additive (new variant + new module declaration appended in alphabetical position); no overlap with the in-flight CLI work in PR #27.

## v1 QA value

After this lands, an operator running through the v1 demo can hit `xvn eod --hours 24 --db /tmp/xvn.db` and get a deterministic markdown summary of every eval run + audit-logged operation in the window. Closes one of the five §168 success criteria.
