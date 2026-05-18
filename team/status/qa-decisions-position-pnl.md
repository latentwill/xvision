# qa-decisions-position-pnl — status

**Owner:** worker (claude session, 2026-05-18)
**Branch:** `task/qa-decisions-position-pnl`
**Worktree:** `.worktrees/qa-decisions-position-pnl`
**Base:** `origin/main` (post-sweep)

## Snapshot

Investigation done — position state is **derivable client-side from
existing `DecisionRowDto` fields**, no engine extension required.
Open-positions cell shipped + new column on the eval-runs decisions
table. PnL display already existed and works for backtest mode; one
gap in paper mode (`paper.rs:565`) is out of scope and queued.

## Acceptance — investigation answer

> **"Investigation note in `team/status/qa-decisions-position-pnl.md`
> states whether the position/PnL state is already in the eval result
> (display-only fix, stays a leaf) or needs to be computed in the
> executor (integration fix, owns the engine slice)."**

**Display-only fix.** The data the operator needs is already in
`DecisionRowDto`:
- `action` (`long_open` / `short_open` / `flat` / `hold`)
- `fill_size`, `fill_price`, `asset`
- `pnl_realized`

Walking the decisions in `decision_index` order is enough to
reconstruct per-asset position state at the end of each bar. The
engine's own `simulate_fill` (`backtest.rs:701-781`) follows the same
state machine — long_open / short_open replace the leg, flat
clears it, hold no-ops — so the client-side derivation mirrors the
backend semantics exactly. No new DTO field, no engine compute, no
migration.

Why the client-side path is correct, not a hack:
- The operator's ambiguity was visual ("is the short still on?"),
  not informational — the data was already on the row, just not
  rendered.
- Adding a new DTO field would require schema migration + engine
  computation in both backtest.rs and paper.rs (one is owned by
  another track post-sweep — `qa-trace-broker-spans` /
  `agent-error-feedback-self-healing`).
- The derivation is testable in isolation (11 unit tests on
  `derivePositionsByDecision`) and matches engine semantics by
  construction.

## What landed in this PR

### Derivation module — `frontend/web/src/features/decisions/positions.ts`

- `OpenPosition` type: `{ asset, side, qty, entry_price }`. Flat
  represented by *absence* from the list, not a sentinel row.
- `derivePositionsByDecision(rows): Map<decision_index, OpenPosition[]>`
  walks rows in `decision_index` order. State machine:
  - `long_open` while flat or short → open/reverse to long with `qty=fill_size, entry=fill_price`
  - `short_open` while flat or long → open/reverse to short
  - `flat` → close (drop asset from state)
  - `hold` (or unknown action) → no change
- Defensive: zero / null `fill_size` or `fill_price` on an open is
  ignored rather than creating a degenerate position. Same-direction
  reopens are no-ops (matches engine `simulate_fill`).

### Decisions table — `frontend/web/src/routes/eval-runs-detail.tsx`

- New "Open positions" column between PnL and Reasoning. Cell renders
  either `flat` (text) or one chip per active position:
  `<asset> <side> <qty> @ <entry>`.
- `OpenPositionsCell` uses dedicated `.dec-pos` CSS class for chip
  layout (long → gold accent, short → danger accent — mirrors the
  existing `.dec-pill` colour convention).
- Derivation runs over the **full unfiltered row set** before the
  filter pill applies, so "positions after CLOSE = flat" stays true
  even when the operator filters to "Close" only.
- Numeric formatters (`fmtPositionQty`, `fmtPositionEntry`) cope with
  fractional crypto sizes (down to 1e-4) and grouped fiat prices
  (>=1000, no decimals).
- `data-testid` hooks on the cell + flat sentinel for stable test
  selectors.

### Styles — `frontend/web/src/styles/globals.css`

- New `.dec-pos` block under the existing `.dec-pill` family.
  Sibling-style: smaller padding, denser layout. Side accent uses
  `color-mix` against existing semantic tokens (`--gold`, `--danger`)
  so dark mode follows the theme tokens (no `border-white`,
  `border-gray-100`, `#fff` — CLAUDE.md rule).

### Tests

- 11 unit tests on `derivePositionsByDecision` covering:
  short_open → flat (operator repro), flat → hold (no carry-over),
  re-entry after close, hold preserves position, same-direction
  reopen is no-op, reverse from long to short, multi-asset, input
  ordering, zero/null fills, unknown action.
- 4 component tests via the eval-runs-detail route harness:
  short_open → close-flat repro, hold-after-close, re-entry,
  realized PnL fills in on close.
- 1 pre-existing eval-runs-detail assertion migrated to
  `getAllByText` since "BTC/USD" now appears both in the Asset
  column and the new Open-positions cell.

## Verification

```
pnpm --dir frontend/web typecheck             # clean
pnpm --dir frontend/web test -- src/routes/eval-runs-detail.test.tsx \
                                 src/features/decisions/positions.test.ts
                                              # 44/44 pass
pnpm --dir frontend/web build                 # clean
pnpm --dir frontend/web test                  # 393/394 pass
```

One pre-existing failure (`components/chart/RunChart.test.tsx::sma20`)
reproduces on `origin/main` with my changes stashed — unrelated to
this PR.

## Contract path correction

The contract's original `allowed_paths` listed five files / dirs
that don't exist in the codebase
(`frontend/web/src/features/decisions/**`, `routes/decisions.tsx`,
`api/decisions.ts`, `crates/xvision-engine/src/eval/portfolio/**`,
`crates/xvision-engine/tests/decisions_position_pnl.rs`). Same
shape as the earlier `qa-budget-cost-precision` path correction.

Actual touch points used here:
- `frontend/web/src/features/decisions/positions.ts` (new — fits the
  contract's intended `features/decisions/**` namespace)
- `frontend/web/src/features/decisions/positions.test.ts` (new)
- `frontend/web/src/routes/eval-runs-detail.tsx` (the real decisions
  surface — was not on either allowed or forbidden list)
- `frontend/web/src/routes/eval-runs-detail.test.tsx` (component tests
  added; one pre-existing assertion migrated)
- `frontend/web/src/styles/globals.css` (CSS for the new `.dec-pos`
  chip family)

The contract Notes section is updated in this PR's diff to reflect
the corrected paths.

## Out of scope — paper.rs `pnl_realized: None`

`crates/xvision-engine/src/eval/executor/paper.rs:565` hardcodes
`pnl_realized: None` on every decision row, so paper-mode runs never
populate the PnL column (backtest mode is correct). The fix is a
2-line change to thread the realized PnL out of the alpaca/orderly
fill response — but `paper.rs` is owned by `qa-trace-broker-spans` /
`agent-error-feedback-self-healing` per `team/OWNERSHIP.md` after
the 2026-05-18 sweep. Filed as
`team/queue/qa-decisions-position-pnl__20260518T060540Z__paper-mode-pnl-realized-hardcoded-none.md`.

This PR ships the open-positions cell + ensures backtest-mode PnL
renders (it already did, but the table column now lives alongside
the new positions cell for visual proximity). Once the paper.rs
gap closes, the same column will populate for paper runs without
further frontend work.

## Open follow-ups

- Backend wire-in for paper-mode `pnl_realized` (see queue note).
  Routes to whichever of `qa-trace-broker-spans` /
  `agent-error-feedback-self-healing` claims `paper.rs` first.
- Optional polish (deferred): once `paper.rs` lands, the
  open-positions cell could optionally render unrealized PnL
  using the next bar's close as mark price. Today the cell shows
  `qty @ entry` only; unrealized requires a second pass against
  the equity curve. Not a v1 requirement and not worth the
  complexity until the realized-PnL gap closes.
