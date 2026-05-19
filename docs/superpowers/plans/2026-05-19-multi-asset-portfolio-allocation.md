# Multi-asset portfolio allocation

**Status:** draft / pre-intake
**Author:** operator + Opus 4.7 paired investigation, 2026-05-19
**Trigger:** eval run `01KRZG1HBEMEB66DWRNYED8RY7` (ETH/USD, paper) — 3 consecutive `broker_min_order_size` rejections aborted the run, root cause was a $99k BTC long left over from a prior run that starved the cash bucket. The fixes for this specific symptom are scoped per-asset; multi-asset will exercise the cross-asset failure modes the per-asset model doesn't see.

This doc is not yet an executable plan. It's a snapshot of what the
existing single-asset codepaths assume, where those assumptions break
when we go multi-asset, and the rough shape of the refactor that
follows. A real plan (with tasks, contracts, ordering) gets cut once
this is workshopped.

## Single-asset assumptions encoded today

Locations referenced as of `origin/main @ 1d6a01ff` (2026-05-19).

1. **One asset per eval run.** `paper.rs:400` and `backtest.rs` pull
   `scenario.asset.first().venue_symbol` and run the whole loop against
   that single symbol. Scenarios carry a `Vec<Asset>` but only index 0
   is used. The `TODO(Task 5)` at `paper.rs:397` is the marker.
2. **Cash bucket queried per-asset on each decision.** `paper.rs:649`
   calls `broker.buying_power(&asset)` immediately before sizing — but
   `buying_power` returns the global cash bucket (for crypto) or global
   `buying_power` (for equities). Two assets asking on the same bar
   both see the same total and would both try to size against it. No
   allocator sits above.
3. **`risk_pct_per_trade` is a single scalar on `Strategy.risk`.**
   `paper.rs:650`: `usd_at_risk = buying_power * strategy.risk.risk_pct_per_trade`.
   With N assets that all see "full buying_power × risk_pct," they'll
   collectively try to allocate `N × risk_pct` of cash on a single bar.
4. **Position checks are per-asset.** The duplicate-open gate
   (`paper.rs:631-638`) checks `position > 0.0` for *the asset being
   submitted*. Cross-asset cash starvation (BTC long swallows cash,
   ETH order then fails) goes undetected. The cash-aware gate added in
   PR <pending> catches the symptom but is still reactive.
5. **Self-healing feedback is per-asset.** `BrokerErrorFeedback`
   (`paper.rs:185`) carries one `asset` string. The agent sees errors
   one asset at a time and can't reason about "I'd rather hold dry
   powder for the SOL setup later."
6. **Equity curve is single-asset.** `equity_samples` is a flat
   `Vec<f64>`; `MetricsSummary` tracks one return stream. No per-asset
   attribution.
7. **Trader prompt seed carries one asset's bars.** The seed has
   `market_data.bar_history`, singular. Adding a second asset means
   either two seeds + two trader calls per bar, or one seed with two
   bar histories.
8. **`AlpacaPaperSurface::buying_power` partitions by crypto vs
   equities** (`broker_surface.rs:618`) but returns the same scalar for
   every crypto symbol. A real multi-asset allocator needs to ask "how
   much of the *unreserved* cash bucket is available to me, given other
   pending allocations on this bar?"

## Failure modes multi-asset will expose

- **Cross-asset cash starvation** (the bug that prompted this doc).
  Today: one stuck position from yesterday → today's order fails. Worse
  in multi-asset: ETH eats the budget on bar 1 → SOL on the same bar
  silently gets 0.
- **Implicit over-allocation.** Two assets with 3% risk_pct each will
  ask for 6% combined on a synchronized bar. The first fills, the
  second is sized against the post-fill cash — but the prompt told the
  agent it had a full 3%, so the trader's reasoning is now based on a
  fiction.
- **Order race conditions.** Alpaca returns 200 on `create_order` but
  the fill is async; two consecutive same-bar orders see the same
  pre-fill cash. Either pipeline serialization or pre-reservation is
  needed.
- **Position-attribution accounting.** Equity curve, Sharpe, drawdown
  all need to be computable per-asset and at portfolio level.
  `metrics.rs` currently has no asset dimension.
- **Self-healing scope creep.** When ETH fails with
  `broker_min_order_size` but SOL fills fine, the agent's "fix" should
  be portfolio-aware ("skip ETH but keep SOL"), not asset-local.
- **Account isolation gets harder.** Paper Alpaca is shared across
  runs. With one asset you can squint and pretend cross-run state
  doesn't matter; with N assets, leftover positions from yesterday's
  BTC run silently break today's ETH+SOL+AAPL run. The
  `scripts/alpaca-paper-reset.sh` helper from PR <pending> is the
  interim manual answer; structurally we need either (a) per-run
  account isolation (separate Alpaca paper keys per run — Alpaca
  doesn't support this for free), or (b) a deterministic close-all at
  run start baked into the executor.

## Refactor shape (sketch, not plan)

In rough dependency order — each step is independently shippable:

1. **Lift `risk_pct_per_trade` from `Strategy.risk` scalar to a
   per-asset map / allocator policy.** Default to "split risk_pct
   equally across assets in `scenario.asset`" so single-asset
   behaviour is preserved. New shape: a `RiskAllocator` trait with
   `equal_weight`, `conviction_weighted`, `capped_max_per_asset` impls.
   Touches: `Strategy.risk` struct, `paper.rs:649`, `backtest.rs` sizing
   path, schema migration, wizard's risk panel.
2. **Introduce a `PortfolioAllocator` between agent decisions and
   broker submits.** Takes N `TraderDecision`s for one bar, returns N
   `OrderRequest`s sized against the *post-allocation* cash for each
   slot — i.e. it reserves cash up front. Replaces the inline sizing
   at `paper.rs:642`. Pure function over `(buying_power,
   decisions, risk_policy)` so tests don't need a broker.
3. **Per-asset positions + portfolio-level cash in the executor
   state.** `let position = self.broker.position(&asset)` becomes
   `let positions: HashMap<String, f64> = self.broker.positions()`.
   `BrokerSurface::positions()` already exists conceptually in
   `AlpacaApi::list_positions` — lift it to the trait.
4. **Run-start account reset.** Add `BrokerSurface::reset()` with
   default no-op. `AlpacaPaperSurface::reset()` calls `DELETE /v2/positions`
   + `DELETE /v2/orders`. Call once at `run_inner` entry. Gate behind
   `scenario.paper_reset_before_run: bool` (default true for paper).
5. **Multi-asset agent seed.** `bar_history` becomes a
   `HashMap<asset, Vec<Bar>>`; the trader emits a
   `Vec<TraderDecision>` (one per asset they want to act on) instead
   of one. Risk gate reviews the bundle. This is the biggest single
   step and probably wants its own plan doc.
6. **Per-asset attribution in metrics.** Equity curve becomes a
   matrix; `MetricsSummary` gets per-asset breakdowns + a portfolio
   row. Charts in the dashboard get an asset filter.
7. **Self-healing feedback bundle.** `BrokerErrorFeedback` becomes
   `Vec<BrokerErrorFeedback>`, scoped per-asset per-bar. Prompt seed
   surfaces all of them so the trader can reason about which assets
   to back off.

## What the round-4 fixes look like through this lens

The recent broker wave (#286 self-healing, #288 buying_power, #314
classifier, #320 circuit breaker, plus the in-flight cash-aware gate)
is **all the right shape** — these fixes just live one level below
where multi-asset will need them. The refactor is more "promote these
from per-asset to portfolio-aware" than "rip and replace." Concretely:

- Self-healing → portfolio-aware self-healing (step 7)
- buying_power split → already correct at the broker layer; just
  needs the allocator (step 2) to consume it correctly
- Classifier → unchanged (errors are already per-call, not per-asset)
- Circuit breaker → portfolio circuit breaker (abort if N rejections
  across *any* asset, not just one)
- Cash-aware gate → subsumed by the allocator's reservation logic
  (step 2)

## Open questions

1. Do we keep crypto + equities in a single eval run, or constrain
   each run to one asset class? Alpaca's `cash` vs `buying_power`
   split makes cross-class allocation tricky — crypto can't use
   margin, equities can.
2. How do we want to expose the allocator policy to the wizard? A
   dropdown ("equal", "conviction", "capped") feels right; a free-form
   per-asset risk_pct table is power-user territory.
3. Do we ever want to support multi-strategy on a single Alpaca
   account, or is one-strategy-per-account a hard constraint? The
   former is much harder (now portfolio is shared across pipelines).

## Concrete next steps

- [ ] Take this to intake.
- [ ] Land the cash-aware gate + reset script (already in flight; this
      doc is a sibling artifact).
- [ ] Pick a target scenario for the multi-asset MVP — probably
      BTC/USD + ETH/USD on a 4h cadence, since both are already in
      the Alpaca crypto whitelist.
- [ ] Cut the per-asset risk allocator as the first contract; it's
      the smallest step that surfaces the schema/migration shape and
      doesn't require trader-prompt changes.
