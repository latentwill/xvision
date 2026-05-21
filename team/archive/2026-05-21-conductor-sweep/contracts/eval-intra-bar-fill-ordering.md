---
track: eval-intra-bar-fill-ordering
lane: leaf
wave: v2e
worktree: .worktrees/eval-intra-bar-fill-ordering
branch: task/eval-intra-bar-fill-ordering
base: origin/main
status: merged
depends_on:
  - eval-cost-model-per-bar-and-volume-share
  - eval-trace-surface-foundation
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/backtest.rs     # intra-bar ordering — disjoint region with cost-model/trace/broker tracks
  - crates/xvision-engine/src/eval/orders.rs                # NEW — OrderState, FillBranch, AggressorSide enums
  - crates/xvision-engine/tests/intra_bar_*.rs              # NEW
  - frontend/web/src/api/types.gen/**                       # ts-rs regenerated
forbidden_paths:
  - frontend/web/src/**                                     # no UI work this track
  - crates/xvision-data/**
  - crates/xvision-eval/**
  - crates/xvision-engine/src/eval/scenario.rs              # cost-model owns this
  - crates/xvision-engine/migrations/**                     # no schema change — OrderState/FillBranch are runtime enums + JSONL fields
interfaces_used:
  - xvision-engine::eval::executor::backtest::simulate_fill (post-cost-model rewrite)
  - xvision-engine::eval::scenario::VenueSettings
  - xvision-data::fixtures::Ohlcv
parallel_safe: false  # depends on cost-model landing first
parallel_conflicts:
  - eval-cost-model-per-bar-and-volume-share (backtest.rs — sequential dependency; rebase on its merged branch)
  - eval-trace-surface-foundation (backtest.rs — disjoint regions; foundation owns the emit schema, this track populates fill_branch + aggressor_side)
  - eval-broker-rule-findings (backtest.rs — disjoint regions; broker owns order-emission, this track owns fill-trigger)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine intra_bar_
  - cargo test -p xvision-engine eval::executor::backtest
  - pnpm --dir frontend/web typecheck
acceptance:
  - **`OrderState` enum.** `Open`, `PartiallyFilled`, `Filled`, `Cancelled`, `Expired`, `Rejected`. Serde+ts-rs. Minimal — no queue model. `PartiallyFilled` exists as a variant for `eval-cost-model-per-bar-and-volume-share`'s volume-cap-binding case to write into; the carry-to-next-bar loop is deferred to a follow-up.
  - **`FillBranch` enum.** `GapPastTrigger`, `OhlcHighFirst`, `OhlcLowFirst`, `NextOpenOnly`. Recorded per fill in the trace.
  - **`AggressorSide` enum.** `Maker`, `Taker`. Recorded per fill.
  - **NautilusTrader-style intra-bar ordering.** For limit/stop/TP orders triggered within a bar:
    * If `gap_open` is past the trigger (e.g. stop at 100, bar opens at 95) → fill at the open with `FillBranch::GapPastTrigger`. No price guarantee.
    * Else process the bar's price walk in O→H→L→C order if `H` is closer to `O` than `L` is, else `O→L→H→C`. First crossing fills.
    * Limit orders only fill if the price actually crossed the limit; inference from L/H alone is insufficient.
  - **Market orders unchanged.** Still fill at next-bar open with `FillBranch::NextOpenOnly`. The whole `O→H→L→C` walk is irrelevant for market orders.
  - **Maker/taker classification.** A limit at `open ± spread/2` that fills passively → `AggressorSide::Maker`. A market order or a limit that crosses → `AggressorSide::Taker`. Per-fill `fee_bps_applied` becomes a function of aggressor side, not the constant `taker_bps`. Specifically: maker fills use `fees.maker_bps`, taker fills use `fees.taker_bps`.
  - **Spread proxy for maker classification.** Uses the `spread_bps_applied` field populated by `eval-cost-model-per-bar-and-volume-share` (per-bar array if present, else per-asset override, else scenario default; if all absent, fall back to Corwin-Schultz proxy `2 * sqrt(max(0, log(H/L)² - 2*log(2)*sigma²))` over a small rolling window — keep this code in a helper so it can be tuned later).
  - **Existing simulator behavior preserved when no orders trigger intra-bar.** Market-order paths through `simulate_fill` are unchanged; this track only introduces new paths for triggered orders. Existing 9 tests at `backtest.rs:830–940` continue to pass (or are explicitly updated with `# Updated because <reason>`).
  - **Tests:**
    * One test per `FillBranch` variant — gap-past-trigger, ohlc_high_first, ohlc_low_first, next_open_only.
    * Limit orders that don't cross do not fill (state stays `Open`).
    * Maker classification: passive limit at `open + spread/2 + epsilon` on a sell side gets `AggressorSide::Maker` and uses `maker_bps`.
    * Taker classification: market buy gets `AggressorSide::Taker` and uses `taker_bps`.
    * `OrderState` enum round-trips through JSONL (no panic on legacy runs that don't have the field).
    * Corwin-Schultz spread proxy returns a finite, non-negative value for synthetic O/H/L/C tuples.

---

# Scope

Research doc §4.7 (adaptive intra-bar ordering) + §4.5 (maker/taker
aggressor-side fees). Promoted from "Out of this intake" in the
2026-05-20 intake update because without it, every limit/stop/TP order
silently fills at next-bar open — i.e. the cost machinery in
`eval-cost-model-per-bar-and-volume-share` produces an honest fill
*price* but a dishonest fill *trigger*. Closes the trader risk
management is theatrical problem for V2E.

Steals NautilusTrader's bar-data fill ordering convention. Does not
attempt tick-level realism — the doc's §3.7 bar-vs-tick fidelity guard
remains a scenario-level policy decision.

# Out of scope

- Partial fill carry-loop across N bars. The `PartiallyFilled`
  `OrderState` variant exists for the volume-cap-binding case in
  `eval-cost-model-per-bar-and-volume-share`; the multi-bar rollover
  is deferred until the cap is producing real cap-hit metrics that
  motivate the complexity.
- Order queue model (Hummingbot-style `InFlightOrder` with queue
  position). Different architecture; not v1.
- Latency-shifted fill timestamps (§4.8). Defer.
- Funding/borrow accrual (§4.10). Already on
  `docs/superpowers/plans/2026-05-11-perps-eval-simulator.md`.

# Migration coordination

No migration. `OrderState` lives in the events stream (already in the
trace shape from `eval-trace-surface-foundation`); `FillBranch` and
`AggressorSide` are per-fill JSONL fields populated by this track into
columns reserved by foundation.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-intra-bar-fill-ordering status
git -C .worktrees/eval-intra-bar-fill-ordering log --oneline -3 origin/main..HEAD

# Confirm:
#   - rebased on top of eval-cost-model-per-bar-and-volume-share's merged commit
#   - foundation's trace-shape columns (fill_branch, aggressor_side) are present
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-intra-bar-fill-ordering -b task/eval-intra-bar-fill-ordering origin/main
```

# Notes

The "H closer to O than L is" heuristic is NautilusTrader's published
rule for bar-data backtests. It's a heuristic — the real intra-bar
walk could go any order. The convention is documented as a known
limitation, not a claim of realism. When tighter realism is needed,
the scenario should upgrade to minute-or-trade-level data (§3.7).

Maker fills assume there is liquidity at the passive price. For bar
data that's an inference, not a fact. The convention here is
permissive (any limit inside `open ± spread/2` fills as maker if the
bar's range covers it). Tighten if backtests start showing unrealistic
maker rates.
