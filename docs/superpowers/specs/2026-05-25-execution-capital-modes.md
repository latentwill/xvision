# Execution & Capital Modes — Behavior Spec

**Date:** 2026-05-25
**Status:** Design spec. Phase 3 of `2026-05-25-multi-asset-followups.md`
(design + not-implemented tests; **no mode implementation in this track**).
**Code:** `crates/xvision-engine/src/strategies/exec_mode.rs` (`ExecutionMode`,
`CapitalMode`); rejection gate in
`crates/xvision-engine/src/eval/executor/backtest.rs`.

## Current state (v1)

Both modes are **Strategy data with a default**, not harness invariants — so a
prompt-optimizer can vary them without engine edits (DSPy-reachability). v1
implements only the default arms; every other arm **parses + validates** but
the backtest executor returns a clear `not yet implemented` error.

| Field | Variants | v1 implemented | v1 rejected at runtime |
|---|---|---|---|
| `ExecutionMode` | `PerAsset` (default), `Portfolio`, `Custom(String)` | `PerAsset` | `Portfolio`, `Custom(_)` |
| `CapitalMode` | `Pooled` (default), `PerAsset` | `Pooled` | `PerAsset` |

The rejection lives at the top of the backtest run loop, before bars are
required, so an unsupported mode fails fast with an actionable message:
`execution_mode `portfolio` not yet implemented`, `execution_mode
`custom:{name}` not yet implemented`, `capital_mode `per_asset` not yet
implemented`.

**Authoring posture (acceptance):** unsupported modes are intentionally
*stored as experimental* and *rejected at run time* — strategy authoring may
save them, but a run rejects them with the message above. This satisfies "no
silent acceptance" without blocking the optimizer from materializing the
hypotheses. UI surfaces should label non-default modes as experimental /
backtest-rejected (follow-up; no UI in this track).

## ExecutionMode::PerAsset vs Portfolio

### PerAsset (implemented)
- The pipeline runs **once per active asset, each bar**. Each (bar, asset) is
  an independent decision cycle with its own briefing.
- The trader sees one asset at a time; it cannot reason about cross-asset
  exposure within a single call.
- N active assets × M bars ⇒ up to N×M decisions.

### Portfolio (reserved)
- **One agent call per bar** sees the whole book: the trader reasons over all
  active assets together and emits a set of per-asset actions (or a target
  allocation).
- Required to express cross-asset logic: pairs, rotation, risk-parity,
  net-exposure caps.

**Portfolio briefing shape (contract for the implementer):**
- `as_of` timestamp + the active asset set.
- **Open positions for all assets** (symbol, side, qty, entry, unrealized) so
  the trader sees current exposure.
- **Per-asset market snapshot**: latest OHLCV + indicators per active asset,
  keyed by symbol.
- Pooled NAV + cash available.
- Output: a list of per-asset `TraderDecision`s (or a target-weight vector the
  executor translates into per-asset orders), validated against the active set
  — **no implicit asset**; an action naming an out-of-universe asset is an
  error, not a fallback.

## CapitalMode::Pooled vs PerAsset

### Pooled (implemented)
- One capital pool; per-asset positions debit/credit shared cash; **one**
  pooled NAV / equity series (one equity sample per distinct timestamp, not per
  decision). Drawdown is portfolio-level. See `PortfolioBook` and
  `multi_asset_backtest.rs::backtest_fans_out_over_universe_with_shared_nav`.

### PerAsset (reserved)
- Each asset gets a **segregated sleeve** with its own cash + equity series.
- **Equity/drawdown accounting (contract for the implementer):**
  - Split `capital.initial` across the active sleeves at run start (define the
    split policy: equal-weight by default; document any rebalancing).
  - Each sleeve tracks its own NAV and max-drawdown; the run also reports an
    aggregate NAV = Σ sleeve NAVs for a comparable top-line.
  - A sleeve cannot spend another sleeve's cash; an order that would overdraw a
    sleeve is rejected (not borrowed from the pool).
  - Report per-sleeve and aggregate drawdown distinctly so per-asset risk is
    visible.

## Not-implemented contract (tests in this track)

`crates/xvision-engine/tests/multi_asset_backtest.rs` pins the rejection of
every unsupported combination so the contract can't regress silently:

- `portfolio_mode_returns_not_implemented` (pre-existing)
- `custom_execution_mode_returns_not_implemented` (added here)
- `capital_mode_per_asset_returns_not_implemented` (added here)

When a mode is implemented, the matching test flips from asserting the
rejection to asserting the new behavior, in the same change — so the
not-implemented guard and its removal are reviewed together.

## Sequencing for the implementer

1. Implement `CapitalMode::PerAsset` accounting on top of the existing
   `PortfolioBook` (sleeves) — backtest only.
2. Implement `ExecutionMode::Portfolio` (one-call-per-bar over the book
   briefing) — backtest only.
3. Leave `Custom(_)` rejected until a concrete optimizer-authored mode needs it.
4. Live multi-asset (any mode) stays gated by the cline-live L2 plan — see
   `docs/superpowers/notes/2026-05-25-live-multi-asset-invariants.md`.
