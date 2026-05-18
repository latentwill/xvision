---
track: qa-decisions-position-pnl
lane: integration
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-decisions-position-pnl
branch: task/qa-decisions-position-pnl
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/eval/executor/trader_output.rs
  - crates/xvision-engine/src/eval/portfolio/**
  - crates/xvision-engine/tests/decisions_position_pnl.rs
  - frontend/web/src/features/decisions/**
  - frontend/web/src/routes/decisions.tsx
  - frontend/web/src/routes/decisions.test.tsx
  - frontend/web/src/api/decisions.ts
  - frontend/web/src/api/decisions.test.ts
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-execution/**
  - frontend/web/src/features/agent-runs/**
interfaces_used:
  - decisions table (read)
  - portfolio / position state computation
  - TraderDecision → executor → decision row pipeline
parallel_safe: false
parallel_conflicts:
  - "qa-decisions-30day-count: also touches eval/executor/. Coordinate disjoint regions; PnL display assumes the bar-count fix landed first."
  - "qa-trace-broker-spans: instruments broker calls; this contract surfaces position/PnL rollups. Both touch executor — coordinate via team/queue/."
  - "alpaca-paper-crypto-submit: single-writer claim on paper.rs. Avoid that file or stack."
verification:
  - cargo test -p xvision-engine
  - cargo test -p xvision-engine --test decisions_position_pnl
  - cargo clippy -p xvision-engine -- -D warnings
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run decisions
  - pnpm --dir frontend/web build
acceptance:
  - The decisions surface gains a per-row "Open positions" cell that
    lists active positions at the end of that bar (symbol, side, qty,
    entry price, mark price, unrealized PnL). A CLOSE / HOLD row after
    a short-open is now visually unambiguous because the position
    state is shown explicitly.
  - PnL columns (realized + unrealized) populate on close: when an
    order closes, the closing decision row shows the realized PnL
    from that close, and subsequent rows reflect the closed position
    correctly in the open-positions cell.
  - Investigation note in `team/status/qa-decisions-position-pnl.md`
    states whether the position/PnL state is already in the eval
    result (display-only fix, stays a leaf) or needs to be computed in
    the executor (integration fix, owns the engine slice).
  - Regression test in `crates/xvision-engine/tests/decisions_position_pnl.rs`
    walks a short-open → CLOSE-flat → re-enter sequence and asserts:
    (1) the close decision row carries realized PnL; (2) the bar after
    close shows zero open positions; (3) the re-entry row shows the
    new position; (4) HOLD rows after the close don't ambiguously
    appear "still in position."
  - No `border-white` / `border-gray-100` / `border-gray-200` / `#fff`
    on dark mode (CLAUDE.md rule).
  - No schema migration. If a new column is needed on the decisions
    table, file a contract update and reserve a migration through
    `team/MANIFEST.md`.
---

# Scope

Two related operator-reported bugs on the decisions surface
(2026-05-18):

1. CLOSE and HOLD decision rows are ambiguous when a position is still
   open. Example: a short-open then the next bar is CLOSE flat — the
   operator can't tell from the row whether the short is still on.
   Add per-row open-position visibility so position state is part of
   the row's information.
2. PnL doesn't fill in on decisions where the order closes — the
   realized PnL never propagates onto the close row.

Likely the engine computes the position state and PnL but the
decisions surface isn't reading or rendering it. If the data is
missing at the engine layer, scope expands to the executor / portfolio
slice; contract allows that.

# Out of scope

- Broker call span emission (`qa-trace-broker-spans`).
- Off-by-one bar count (`qa-decisions-30day-count`) — that's the
  prerequisite for a 30-bar test; assume it lands first.
- Decisions table schema migrations. If a new column is genuinely
  needed, file a contract update.
- Re-designing the decisions table layout. Add the open-positions
  cell to the existing layout; don't restructure columns.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-decisions-position-pnl status
git -C .worktrees/qa-decisions-position-pnl log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-decisions-position-pnl \
  -b task/qa-decisions-position-pnl origin/main
```

# Notes

Investigation order:

1. Check what shape the decisions API already returns. Grep
   `frontend/web/src/api/decisions.ts` and the engine's
   `crates/xvision-engine/src/api/decisions.rs` (if it exists) for
   per-row position / PnL fields.
2. If the fields exist, this is a frontend rendering fix.
3. If not, walk the executor → decision write path and add the
   computation. Reuse any portfolio / position state the eval
   harness already maintains for the metrics summary.

Append checkpoints / PR links below.
