# Eval data + execution accuracy — gap analysis and strategy menu

> **Status:** Research/proposal · 2026-05-19 · For Edward to read and pick from.
> **Source conversation:** "How can I confirm the accuracy of the data of my evaluations?" (2026-05-19)
> **Companion docs:** `docs/superpowers/specs/2026-05-08-eval-engine-design.md` (the current eval surface), `docs/superpowers/plans/2026-05-11-perps-eval-simulator.md` (perp follow-up that already touches this), `docs/hummingbot-eval.md`, `docs/superpowers/research/2026-05-10-freqtrade-lumibot-comparison.html`.

---

## 0. TL;DR

The eval engine has **two independent accuracy questions** that get mixed together:

1. **Data fidelity** — are the bars we replay actually what the market saw at that timestamp?
2. **Execution fidelity** — given a correct bar, is the simulated fill (price, size, fee, slippage) a realistic stand-in for what would have happened on the real venue at that moment?

Today, xvision treats both lightly. There is **no candle-integrity layer** (no monotonicity check, no OHLC sanity, no gap detection), and the fill simulator applies **one flat slippage + fee value to every fill across the whole scenario** (linear bps multiplied to `next_bar.open`, taker fee on notional). The math is in `crates/xvision-engine/src/eval/executor/backtest.rs::simulate_fill` and the scenario knobs are `VenueSettings { fees, slippage, fill_model }` in `crates/xvision-engine/src/eval/scenario.rs`. There is no facility today to vary slippage by bar, by asset, by volume, by regime, or by venue tier.

This doc lays out a menu of strategies — small, large, and outright research bets — that the codebase could adopt. They split into a **Data integrity track** and an **Execution realism track** that can ship independently. Each option lists rough scope ("afternoon" → "multi-wave"), what it buys you, and the OSS prior art that already does it.

---

## 1. What's wired today (codebase audit)

Pulled directly from the current tree, not from specs.

| Layer | File | Today |
|---|---|---|
| Candle source | `crates/xvision-data/src/fixtures.rs::load_ohlcv_fixture` | Reads Parquet from `$XVN_PROBES_DIR` or `data/probes/<cache_key>.parquet`. Returns last `lookback_bars` chronologically. **No validation.** |
| Alpaca client | `crates/xvision-execution/src/alpaca.rs:410-622` | Wraps `apca` crate. Paper-mode v1; live stubbed. Crypto vs equity symbol routing. |
| Fill simulator | `crates/xvision-engine/src/eval/executor/backtest.rs::simulate_fill (lines 692–772)` | `fill_price = next_open * (1 ± slip_bps/10_000)`. Single taker fee. No partial fills. No volume cap. No queue model. |
| Slippage knob | `eval/scenario.rs::VenueSettings.slippage` | `SlippageModel::Linear { bps }` or `None` — **flat across run**. |
| Fee knob | `eval/scenario.rs::VenueSettings.fees` | `{ maker_bps, taker_bps }` — **flat across run**. Maker side is read but the simulator hardcodes taker. |
| Fill price selection | backtest loop | **Next bar's open**, always. No cheat-on-close, no VWAP, no intra-bar logic. |
| Baselines | `crates/xvision-eval/src/baselines/{always_long,ma_crossover,macd_momentum,rsi_mean_reversion}.rs` | Pure-rule signals. Trust the executor's fill price entirely. |
| Tests | `backtest.rs:831-894` (9 tests) | Cover no-op, slippage direction sign, PnL booking, reversal. **No** test for fee accuracy at different notionals, volume-constrained fills, partial fills, latency, corporate actions, or data gaps. |
| Data integrity checks | — | **None found.** No monotonic-timestamp check. No duplicate-bar check. No OHLC sanity (low ≤ open/close ≤ high). No gap detection. No NaN guard. |
| Corporate actions | — | None. Alpaca's `adjustment` flag isn't validated and the forum thread suggests it's not always honored anyway. |
| Lookahead detection | implicit only | The "fill at next bar's open" pattern prevents one class of lookahead but doesn't prove the strategy isn't reading bar t while deciding for bar t (e.g., indicator computed over `bars[..=t]` instead of `bars[..t]`). |

Net: xvision has a working, *correct-shape* spot backtester with a small surface and minimal realism. Everything below is upgrade paths.

---

## 2. State-of-the-art reference points

Brief — full citations at the end. The pattern: production-grade backtesters layer **data validation as a separate pipeline stage**, then expose **pluggable per-asset fill/slippage/fee models**, then provide **diagnostics that flag bias before you trust a result**.

- **NautilusTrader** (Rust core — closest peer) — has a `SimulatedExchange` with configurable `FillModel` and `prob_slippage` (per-order probability of one-tick slip). When backtesting on bar data, it processes OHLC in adaptive order (O→H→L→C if H is closer to O, else O→L→H→C). Stop orders handle "gap past trigger" (fill at open) vs "move through trigger" (fill at trigger) explicitly. Documents the bar-data limitation: tight stops/TPs need higher-granularity data.
- **QuantConnect Lean** — has the cleanest pluggable abstractions: `ISlippageModel`, `IFeeModel`, set **per security**, not per scenario. Ships `VolumeShareSlippageModel` (default 2.5% volume cap, 0.1 price-impact coefficient) and `MarketImpactSlippageModel` (Almgren-Chriss-derived; authors explicitly warn the defaults are decades-old calibrations).
- **zipline / zipline-reloaded** — canonical: `fill_price = price * (1 ± price_impact * volume_share²)`, where `volume_share = min(order_qty / bar_volume, 0.025)`. Caps fills at 2.5% of bar volume; any leftover order rolls to the next bar.
- **vectorbt** — supports `slippage` and `fees` as **per-bar numpy arrays**, broadcasting across portfolio. The "right answer" if you want regime-aware or volume-aware costs computed offline once and consumed verbatim.
- **freqtrade** — ships `lookahead-analysis` and `recursive-analysis` CLI tools that diff a full backtest against per-signal sliced backtests to surface cells where indicators "saw the future." Useful even if you don't share the rest of their architecture.
- **Hummingbot** — only one I'd point at if you want CEX-parity order-lifecycle modeling (`InFlightOrder`, partial fills, queue position). The `docs/hummingbot-eval.md` file already calls this out as a future direction.

The **Corwin-Schultz (2012)** bid-ask-spread estimator deserves a name-check: it gives you a per-bar spread proxy from `(H, L)` alone. The bias is known (downward, concentrated in thinly-traded names), but for liquid Alpaca symbols it's a better default than a constant bps.

---

## 3. Strategy menu — Data fidelity

Each strategy is independent and stackable. **Highest-leverage first.**

### 3.1 Candle integrity validator (afternoon)

A `validate_ohlcv(bars: &[Ohlcv], cadence: Duration) -> Vec<DataDefect>` function that runs at fixture load time and at scenario start. Returns structured defects, not a panic:

- `NonMonotonicTimestamp { at, prev_ts, this_ts }`
- `DuplicateTimestamp { at }`
- `MissingBar { expected_ts, gap_bars }` (compares to expected cadence: 1m/5m/15m/1h/1d; weekends + market-hours-aware for equities)
- `OhlcViolation { at, kind }` where `kind ∈ { low > open, low > close, high < open, high < close, high < low }`
- `NegativeOrNanField { at, field }`
- `ZeroVolumeBar { at }` (warn, don't fail — common on overnight crypto pairs)
- `WickShockOutlier { at, sigma }` (e.g., `(high-low)/median(range, 200) > 8σ` — often a feed glitch).

Surface as a **finding** in the run's `findings.jsonl` so the eval engine treats data defects as first-class outputs alongside strategy findings. Tier by severity. A scenario whose underlying bars fail this validator should require an explicit `--allow-defective-data` flag.

**Why this first:** It's small, it's lossless (defects become a CI signal), and it catches the ugly silent-failure modes today.

### 3.2 Pinned canonical fixtures with content-hash receipts (1–2 days)

The eval design spec already flags this as open question 16 (§16, third bullet). The concrete shape:

- Resolve every scenario's bars to a **Parquet snapshot** stored at `data/probes/<sha256>.parquet`.
- Persist `bars_content_hash` alongside the `Run` record. A re-run of the same `(strategy_hash, scenario_id, bars_content_hash, seed)` must produce byte-identical metrics.
- The scenario's `data_seed` field becomes a real ID, not a label.
- Persist a **data manifest** alongside the bars hash, recording every dimension that can shift comparability: `feed` (`iex` / `sip` / `crypto`), `adjustment` (`raw` / `split` / `dividend` / `all`), `timeframe`, `session_filter` (`regular` / `extended` / `overnight`), `calendar` (`NYSE` / `24x7` / venue-specific), `timezone`. Two runs that share a `bars_content_hash` but disagree on the manifest are not comparable; the eval engine should refuse to render them on the same comparison chart without an explicit override.

This converts "Alpaca returned slightly different bars on re-pull" from a silent reproducibility leak into a hash mismatch you can see — and converts "two runs look comparable but used different market views" into an explicit refusal.

### 3.3 Multi-source cross-check (2–4 days)

For top-of-book equities, cross-check each bar's `(open, high, low, close, volume)` against a second source — Polygon, Tiingo, Yahoo daily, or even Alpaca's IEX vs SIP for the same window — and flag bars where any field disagrees by more than `max(0.05%, 1 tick)`. Bar-level diffs become findings. **Doesn't have to gate runs** — just instrument them.

This is the only way to catch the [Alpaca forum-documented bug](https://forum.alpaca.markets/t/data-is-not-adjusted-for-splits-despite-adjustment-split-flag/7753) where the `adjustment=split` flag is not always honored across the full bar range.

### 3.4 Corporate-action ledger (3–5 days)

For equity scenarios, ship a separate `splits.parquet` + `dividends.parquet` and apply them at fixture-load time with adjustments verified against the bars. Flag any bar that *looks like* a split (~2× price gap with no news) but is missing from the ledger. Same for dividends + ex-div discontinuities. Alpaca's `adjustment` flag is not trustworthy on its own (see forum threads); a separate ledger is.

Cheaper alternative: **block any scenario that crosses a known split/div boundary for any of its symbols until adjustment is verified.** This is the "fail-closed" version.

### 3.5 Lookahead-bias prober (1 day)

Borrow freqtrade's two-pass technique: run the full backtest, then for each signal-firing bar `t` re-run the strategy with `bars[..=t-1]` and assert that the decision for `t` is identical. Any divergence → `LookaheadBias` finding pointing at the indicator that read forward. Doesn't catch every form of leakage (cross-asset, regime labels, etc.) but catches 90%.

### 3.6 Point-in-time universe (research → wave-sized)

Survivorship bias is hard to retrofit. The cheapest realistic step: **ship a static `delisted.parquet` ledger** of symbols the scenario universe should include for the time window but no longer exist on Alpaca, and *fail* the scenario if a delisted symbol is in the universe range but missing from the bars. For crypto, less critical (the dataset is the dataset). For US equities, this is the only honest way to backtest portfolio strategies.

### 3.7 Bar-vs-tick fidelity guard (varies)

If a strategy uses tight stops/TPs (< 0.5× median bar range), the eval engine should refuse the run on bar data alone, or downgrade the result to "indicative." NautilusTrader documents this as a hard limitation. The right escape hatch is **upgrading to minute or trade-level data** for symbols with tight-stop strategies. This is a scenario-level policy, not a code change — a finding rule plus docs.

### 3.8 Replay parity test against Alpaca paper (1 wave)

The strongest external validator we have: a strategy that runs in `Backtest` mode over the last 30 days against pinned Parquet bars vs the same strategy running in `Paper` mode against the same 30 days. The two should produce **the same decisions** at the same timestamps, and the simulated fills should agree with the actual Alpaca paper fills within a calibrated envelope. Any drift between the two is the dataset.

This is also the data source for **calibrating** the execution model (§4.9 below).

---

## 4. Strategy menu — Execution realism

Stackable; not ordered by priority but by complexity.

### 4.1 Per-asset fee/slippage table (afternoon)

Replace `VenueSettings { fees, slippage }` with a `Vec<VenueOverride { symbol_pattern, fees, slippage }>` and a default. Now a scenario can say "BTC/USD is 5 bps taker, NVDA is 1 bps, default 10 bps." This is the smallest meaningful step away from "one number for a year." No new ideas required; just plumbing.

### 4.2 Per-bar cost arrays (1–2 days)

vectorbt's pattern: scenarios accept optional Parquet columns `fee_bps`, `slip_bps`, `spread_bps` aligned to the bars. If present, the simulator uses them per-bar. If absent, falls back to the scenario default. This is the **single highest-leverage change** because it unlocks every downstream cost model — regime-aware, volatility-aware, time-of-day, exchange-fee-tier — without re-architecting the simulator.

The author of the strategy or the scenario controls how those columns are filled. Compute-them-once, replay-many is the workflow.

### 4.3 Volume-share slippage (1 day)

Port zipline's `VolumeShareSlippage` directly:

```
volume_share = min(order_qty / bar_volume, volume_limit)   // default 0.025
fill_price = mid * (1 ± price_impact * volume_share²)      // default impact 0.1
```

For sizes within 0.5% of bar volume this is approximately the current flat-bps behavior. For sizes near 2.5% it pushes fills meaningfully against you. Stops you (or rather, the strategy you're evaluating) from quietly assuming you can move millions through a thinly-traded bar. Lean uses this as its default slippage model for a reason.

### 4.4 Partial fills and order rollover (2–3 days)

Currently an order is either fully filled or no-op. The volume cap above naturally pushes you to partial fills: if the cap binds, fill the cap, carry the remainder to the next bar as an open order. Now you have the makings of an order lifecycle:

```
OrderState ∈ { Open, PartiallyFilled, Filled, Cancelled, Expired, Rejected }
```

Doesn't have to be a queue model — single-shot per-bar cap is enough to make the math honest. This unblocks live-paper parity (Hummingbot's `InFlightOrder` pattern, called out in `docs/hummingbot-eval.md`).

### 4.5 Maker/taker aggressor-side fees (afternoon, once 4.4 exists)

`VenueSettings.fees` already has `maker_bps` and `taker_bps`. The simulator hardcodes taker. Once orders have a side and a price, the simulator can classify: a limit at `open ± spread/2` that fills passively is maker; a market order or a limit that crosses is taker. Per-fill fee_bps becomes a function, not a constant. Free realism win.

### 4.6 Spread-aware fill price (1 day)

Replace `next_open * (1 ± slip_bps)` with `next_open * (1 ± slip_bps) ± spread/2`, where `spread` comes from one of:
- **Corwin-Schultz from H/L** — per-bar, no extra data, downward-biased on illiquid names.
- **Per-asset fixed bps add-on** — simple, works for liquid names.
- **L1 quotes when available** — Alpaca free tier doesn't give you historical quotes but the paid tier does; this is the upgrade path.

Wired through §4.2's per-bar arrays so the choice of spread proxy is a scenario decision, not a simulator decision.

### 4.7 Adaptive intra-bar fill ordering for stops/TPs (1–2 days, copy NautilusTrader)

Today the simulator only knows about market-style next-bar-open fills. For limits, stops, and TPs to be honest, the simulator has to decide which intra-bar price the order would have hit. Steal NautilusTrader's rule:

- If `gap_open` is past the trigger → fill at open (gap case, no price guarantee).
- Else process O→H→L→C if H is closer to O than L is; else O→L→H→C.
- Limit orders only fill if price actually crossed them; no inference from L/H alone.

This makes risk-management features (stop-loss, take-profit, brackets) testable instead of theatrical.

### 4.8 Latency model (afternoon)

`scenario.latency.decision_to_fill_ms` already exists in the scenario format (§5 of eval design spec). The simulator should consume it: shift the fill timestamp by `latency_ms`, recompute the fill price from the bar that owns that timestamp. For 1m bars this only matters if latency > 60s; for sub-minute bars and HFT-ish strategies it matters a lot. Mostly value as a sanity check.

### 4.9 Paper-parity calibration (1 wave)

> **Correction (2026-05-19):** an earlier draft of this section called this "calibration from actual Alpaca fills" and framed it as the truth anchor for live execution. That overstates what paper data proves. Alpaca's own docs are explicit that paper trading is a simulation: it does not model market impact, queue position, latency slippage beyond the simulator's own internal latency, price improvement, regulatory fees, or dividends. Paper fills can validate **simulator-vs-Alpaca parity**, not live-execution edge.

The honest shape: every `Paper` mode run captures Alpaca's simulated fill price, timestamp, and size. Build a parallel `Backtest` run over the same timestamps. Compute per-fill drift `paper_fill − backtest_fill` in bps, conditioned on `(symbol, size_pct_of_bar_volume, volatility_bucket, time_of_day)`. The output is a parity envelope: "our backtest matches Alpaca's simulator within ±N bps in the p90 case for this symbol."

Two outputs:
- A `PaperParityProfile { symbol, p50_drift_bps, p90_drift_bps, sample_n }` per `(symbol, regime)` written to `data/parity_profiles/`.
- A finding: `BacktestPaperParityDrift { p50_drift_bps, p90_drift_bps, sample_n }` on any backtest run that uses a profile, telling the user how well-matched the simulator is to Alpaca paper.

What this **does not** prove: that the backtest matches live-market reality. Backtest and Alpaca paper share the same blind spots (no real market impact, no queue position, no price improvement). For that, see §4.9b.

### 4.9b Live-micro-calibration (1 wave, gates signed marketplace attestations)

The only honest input for "does our cost model match reality?" is real fills on real money. A tightly-scoped live-money harness submits small, controlled orders against a curated symbol list under explicit risk limits, captures the actual fills, and uses them as the calibration source for the cost model.

Constraints:
- **Whitelisted symbols** only — high-liquidity, low-tail-risk. No earnings windows. No thin crypto pairs.
- **Notional cap** per order and per day, enforced at submission. The harness can never spend more than a configured budget.
- **Segregated key** — the live broker credential for the harness is separate from any strategy execution surface. Read-only outside the harness.
- **Asymmetric capture** — every harness fill writes to `data/live_calibration/`. Backtests against the same `(symbol, regime)` cells produce a `LiveExecutionDrift` finding using these fills as truth.

Outputs:
- `LiveCalibrationProfile { symbol, p50_slip_bps, p90_slip_bps, observed_fees_bps, observed_rejects, sample_n }` per `(symbol, regime)`.
- Finding `LiveExecutionDrift { ... }` analogous to `BacktestPaperParityDrift` but against live fills.

**This is what gates the signed marketplace attestation.** A marketplace listing's eval attestation is only as honest as the live-calibration sample size and recency behind it. Attestations should carry `calibration_age_days` and `sample_n`; a stale or thin profile downgrades the attestation badge.

Scope cost is mostly operational (live-account plumbing, kill-switches, audit trail) rather than algorithmic. The math is identical to §4.9; only the data source changes.

### 4.10 Funding / borrow accrual (already planned)

The perp-eval-simulator plan (`docs/superpowers/plans/2026-05-11-perps-eval-simulator.md`) already specifies a `FundingProvider` trait and an 8-hour funding accrual loop. Once that lands, spot-short positions and perp leverage stop being free. Not a new proposal — just don't lose it.

### 4.11 Market-impact research bet (multi-wave, optional)

Almgren-Chriss / square-root-law impact is the academic next step beyond §4.3. The MACE project (FinRL-Meta extension, 2026) ships a clean reference implementation. Honest answer: most strategies xvision wants to evaluate are nowhere near the size where impact matters; this is mostly a research-credibility checkbox. Skip until the marketplace ships a strategy where order sizes are big enough that 4.3's quadratic-in-volume-share is the binding term.

### 4.12 Broker-rule findings (crypto-first, afternoon)

A strategy that backtests beautifully but emits orders the broker would reject is a dishonest strategy. Today the simulator silently fills any order the strategy asks for. The fix is a small per-asset-class rule checker that runs at order-emission time and emits a finding if the order would be rejected at the live venue. New finding kinds:

```
broker_rule_violation
unsupported_order_type
unsupported_time_in_force
insufficient_buying_power
non_marginable_asset
short_not_allowed
fractional_order_rounding
min_order_size_violation
pdt_risk_or_rejection            # equities, v1 no-op
extended_hours_not_supported     # equities, v1 no-op
```

The kind list is asset-class-aware. Equity-specific kinds (`pdt_risk_or_rejection`, `extended_hours_not_supported`, `non_marginable_asset`) are no-op stubs in v1 and light up when equity scenarios reach the marketplace (see §3.6 follow-up). Crypto-on-Alpaca rules — `market` / `limit` / `stop_limit` only, TIF restrictions, minimum order sizes, fractional rounding — are the v1 surface.

This is small (an enum + a per-asset-class rule table + a hook in the fill simulator) and high-leverage for agent-authored strategies, which will happily propose orders that look fine in indicator-space but are illegal at the venue. Conceptually this is the reviewer's "third track" (broker fidelity) compressed into a single finding family rather than a new architectural layer.

---

## 5. Trace surface — for users, autooptimizer, and dev loop

The eval engine already emits `decisions.jsonl`, `trades.jsonl`, `events.jsonl`, `findings.jsonl`, and a `traces` table keyed by `cycle_id`. The shape is right; the *content* is too thin for three downstream consumers. This section enumerates what each consumer needs out of the trace surface so we can land the enrichment as a single coordinated change rather than three drive-by additions.

### 5.1 For users — the "why did this happen?" surface

Today a user looking at a losing run can see the trade tape and the equity curve but not *why* the strategy made any given decision. Enrich per-cycle so that's answerable without a re-run.

**Per decision, record:**
- **Inputs visible to the agent**: which bars were in context (window cursor), indicator values computed, prior n decisions seen, regime tag at decision time.
- **Prompt actually sent**: `prompt_template_hash` + the filled prompt (or a pointer to a content-addressed prompt store).
- **Model parameters**: `model_id`, temperature, top_p, seed.
- **Raw response + parsed structured decision**: rejected alternatives if structured output expressed them.
- **Tokens** (input / output / total) and **latency_ms**.
- **Tools called** during the decision, with arguments and returned results.

**Per fill, record:**
- Which bar drove the fill (next-bar open vs intra-bar trigger), and which intra-bar branch fired (gap-past-trigger vs move-through-trigger; O→H→L→C vs O→L→H→C per §4.7).
- Per-bar `slip_bps`, `spread_bps`, `fee_bps` applied, with source (default / scenario override / per-asset override / computed-from-bar-array).
- Volume-cap status: `order_qty / bar_volume`, cap-binding y/n, rollover state if partial (§4.4).
- Fee classification: maker vs taker, source rule.

**Per data load, record (from §3.2 manifest):**
- `bars_content_hash`, `feed`, `adjustment`, `calendar`, `timezone`.
- Defects flagged by §3.1 validator for this window.

**User-facing rendering:** the reviewer's "trust receipt" surface is exactly a renderer over this trace. Dashboard drill-down per cycle expands a row into the full "why?" view without re-running anything.

### 5.2 For the autooptimizer (Karpathy self-improvement loop)

The eval design spec §16 names the autooptimizer loop as a downstream consumer of findings. For the loop to do real work it needs more than findings — it needs structured features it can learn from.

- **Decision feature vector** per decision: a parquet sidecar `cycle_features.parquet` with numeric/categorical features — regime tags, indicator values, position state, equity drawdown depth, prior-decision outcomes. This is the substrate the loop's ML side feeds on. Without it, every loop iteration has to re-extract features from raw JSON; wasteful and brittle.
- **Decision outcome signal**: realized PnL attributable to this decision over the next K bars, holding-period adjusted. Without a pinned attribution rule, "did this decision work?" is unanswerable from trade tapes alone. Pin `decision_outcome.attribution_window_bars` per run so the loop compares apples to apples.
- **Counterfactual baselines pre-computed**: for each run, store the trace that *would have* been produced by N reference strategies (always_long, buy-and-hold, ma_crossover) over the same scenario with the same data hash. The loop learns from the gap; baselines are the floor.
- **Prompt + model lineage**: every prompt template gets a content hash; every `(template_hash, model_id, temperature)` triple is a treatment arm the loop can A/B across runs.
- **Cross-run diff harness**: `diff(trace_a, trace_b) -> Vec<CycleDivergence>` returns the smallest set of cycle-level differences that explain the metrics delta. Lets the loop attribute "strategy A beat strategy B by 200 bps via cycles 17, 43, 88." Doesn't require new storage — requires the cycle records to be structured enough to diff.
- **Failed-decision reservoir**: a flagged subset of decisions where the strategy did the wrong thing — lost money outside expected variance, or missed an obvious opportunity vs a baseline. The loop draws prompt-improvement candidates from this reservoir. Definition: `realized_pnl_z_score < -2` against the strategy's own distribution, or `realized_pnl < baseline_at_same_cycle - threshold`.
- **Findings ↔ cycles backreference**: every finding record carries `evidence_cycle_ids: Vec<Ulid>` pointing at the cycles that produced its evidence. Loop reads findings, walks back to cycles, mines features. (Findings schema in eval design spec §11 already has `evidence` but not a structured cycle backref — extend it.)

### 5.3 For xvision dev — the regression / debug loop

Internal engineering needs trace surfaces the autooptimizer doesn't. The failure modes are different.

- **Determinism receipt** per run: `sha256(strategy_hash || scenario_id || bars_hash || seed || engine_version)` → `metrics_summary_hash`. Tests assert receipt stability across PRs. Any simulator change that affects metrics fails the receipt check loudly; intentional changes bump `engine_version` and rebase the receipts.
- **Simulator state snapshots** per N bars: open positions, pending orders, cash, equity, last fill, last finding. Replay tools can resume the simulator from any snapshot rather than from t=0. Essential for debugging long backtests.
- **Code-path traces**: which slippage formula fired, which intra-bar ordering branch, which validation rule triggered. Per-run histogram of branch hits. Spotting "we never exercised the gap-past-trigger branch in this scenario" is how you know test coverage maps to your scenarios.
- **Counterfactual replay**: load a trace, swap one knob (slippage model, fee, model_id, prompt_template), replay only from the first divergence point forward. Much faster than re-running the whole scenario; also the substrate for the user-facing cost-sensitivity analysis ("what if slippage were 2x?").
- **Per-cycle timing breakdown**: latency split into data-load / indicator-compute / LLM-call / risk-check / simulator / persistence. Surfaces perf regressions before they become user-visible.

### 5.4 Storage and shape

Don't rebuild the trace store — enrich what exists.

- **Decisions and fills JSONL**: extend the per-line schema. Bump `schema_version`. Backfill not required; old runs declare a lower version and consumers handle it.
- **`cycle_features.parquet` sidecar** per run: one row per decision, columns = the feature vector. Parquet because the autooptimizer will do columnar reads at scale.
- **`determinism_receipts` table** keyed by `(run_id)`: stores receipt hashes plus `engine_version` and `schema_version` they were minted under.
- **Findings schema extension**: add `evidence_cycle_ids: Vec<Ulid>` and `produced_by_check: String` (e.g., `"validator:ohlc"`, `"prober:lookahead"`, `"sim:volume_cap"`, `"broker:unsupported_order_type"`).
- **Indexed query columns** on the `cycles` table: `model_id`, `prompt_template_hash`, `regime_tag`. Every meta-loop query asks "all decisions made by model X under regime Y."

**Scope estimate:** 1 wave for schema enrichment + parquet sidecar + receipts table + findings backreference. The counterfactual replay tool, cross-run diff harness, and feature-vector ML hooks are subsequent waves driven by autooptimizer need; the storage shape they need lands now.

### 5.5 Why this should ship inside the v1 wave, not after

If we ship the validator (§3.1), per-bar costs (§4.2), volume-share slippage (§4.3), and the lookahead prober (§3.5) without the trace enrichment, every one of those checks emits a finding that the user can read but the autooptimizer can't reason about — the cycle backref doesn't exist yet, the features aren't structured, the determinism receipt isn't minted. Then someone has to retrofit traces for every emitted finding kind. The cheaper move is one coordinated schema bump at the front.

That means the "five-plan wave" becomes a **trace-foundation plan plus five item plans**, with the foundation landing first or in parallel.

---

## 6. What I'd actually pick (opinionated)

If it were one wave of work I'd ship:

1. **§3.1 Candle integrity validator** — catches silent dataset corruption today, tiny scope.
2. **§3.2 Pinned canonical fixtures with content hash** — converts reproducibility from a promise into a verifiable contract; the eval design spec is already asking for this.
3. **§4.2 Per-bar cost arrays** — the one architectural change that unlocks everything else.
4. **§4.3 Volume-share slippage** — default that's instantly more honest than a flat bps for any non-trivial size.
5. **§3.5 Lookahead-bias prober** — quick, cheap, catches the embarrassing class of bugs.

If it were a follow-up wave:

6. **§4.4 Partial fills + order lifecycle** — unblocks live-paper parity.
7. **§4.7 Adaptive intra-bar ordering** — makes stops/TPs testable.
8. **§3.8 + §4.9 Paper-fill replay and calibration** — the empirical anchor; this is what makes you trustworthy when an institutional buyer asks how you validated.

Everything else is optional or already on a separate roadmap (perps, corporate actions for equities, point-in-time universe).

---

## 7. Cross-cutting concerns

- **Findings, not panics.** Every defect or drift here should surface as a typed finding in `findings.jsonl`. The eval engine already has the schema (§11 of the design spec). New `kind`s: `data_defect`, `lookahead_suspected`, `corporate_action_uncertain`, `execution_drift`, `volume_share_excess`.
- **Per-asset, not per-scenario.** Lean's biggest design win is `IFeeModel` and `ISlippageModel` set per security. Even if xvision keeps a scenario-default, the override layer should be per-symbol-pattern.
- **All of this should be observable in the dashboard.** Findings panel (already specified) is the natural surface for defects and drift; equity-curve overlay (already specified) is the natural surface for "backtest vs paper" parity comparison.

---

## 8. Decisions (2026-05-19)

### 8.1 Initial decisions

- **Packaging:** one plan per item, not a combined wave. §3.1, §3.2, §4.2, §4.3, §3.5 each get their own dated plan under `docs/superpowers/plans/`. Lets each ship (and be killed) independently.
- **§4.9 paper-fill calibration:** scheduled **pre-marketplace, gating signed attestations.** The signed attestation is only as honest as the cost model behind it; calibration is what makes it honest. Calibration plan must land before the marketplace's signed-attestation work begins.
- **§3.6 survivorship bias / point-in-time universe:** **punted for v1.** Add a follow-up task tagged for equities — when US equities become a real marketplace asset class, this gets revived. For v1 the eval engine should refuse equity scenarios that cross known delisting boundaries (cheap guardrail) rather than ship full point-in-time reconstruction.

### 8.2 Review-derived decisions (after spec review, 2026-05-19)

A post-draft review surfaced one substantive technical correction and several scope-creep risks. Concrete acceptances and rejections, with reasons:

| Review point | Decision | Reason |
|---|---|---|
| Alpaca paper fills are not live truth — split §4.9 into paper-parity vs live-micro-calibration | **Accept.** §4.9 now scoped to paper-parity only; §4.9b added for live-micro-calibration as the gate for signed attestations. | Original wording overstated what paper data proves. Live-micro-calibration is the only honest empirical anchor; paper-parity is a parity test, not a truth claim. |
| Add `feed` / `adjustment` / `calendar` / `timezone` / `session_filter` to the run-receipt manifest | **Accept.** Folded into §3.2 (pinned fixtures) as a manifest expansion. | Two runs that share a `bars_content_hash` but disagree on feed are not comparable. Cheap, right, prevents silent miscomparison. |
| Add `broker_rule_violation` family of findings | **Accept as new §4.12.** Crypto-Alpaca rules in v1; equity-specific kinds (PDT, extended-hours, margin) are no-op stubs that light up when equities reach the marketplace. | Agents will produce strategies that emit orders the broker would reject. Treating these as findings is small (enum + per-asset rule table) and prevents the dishonest result. |
| User-facing "trust receipt" surface | **Accept as a renderer, scheduled after the findings substrate exists.** | The trust receipt is a UX over §3.1 + §3.2 + §4.3 + §3.5 + §5 outputs. It can't ship before the findings exist. Slot after the v1 wave. |
| Agent anti-overfitting controls (hidden scenarios, walk-forward + embargo, metric stability, leakage guards, simplicity penalty) | **Defer to marketplace track.** Captured as a follow-up plan seed. | These are correct and important *for the marketplace*. They are not v1 accuracy work. |
| Equity broker constraints (PDT, buying power, margin, extended-hours) | **Defer to equities-readiness follow-up.** | The user already punted equities for v1 (decision 8.1, third bullet). Building PDT-trip detection before equity strategies exist is sequencing inversion. |
| State-of-the-art citations bloat agent context | **Noted, no change to research doc.** Per-item plans cite only what they specifically need. | Research doc is human-read once; per-item plans are agent-context-loaded repeatedly and stay focused by construction. |
| Three-phase roadmap restructure | **Skip.** | Per-item ordering is finer-grained and lets us reorder freely. Phase labels add overhead without changing what ships when. |
| Almgren-Chriss / square-root market impact (§4.11) | **Stay deferred** as already marked. | Most strategies are nowhere near the size where impact matters. |

### 8.3 New requirement added 2026-05-19: trace surface

The five-plan wave is augmented with a **trace-surface foundation plan** (§5 of this doc) that lands first or in parallel with the validator. Rationale: every finding emitted by §3.1, §3.5, §4.3, §4.9, and §4.12 is consumed by users, the autooptimizer loop, and the dev regression loop. Building the trace shape once is cheaper than retrofitting it per finding kind.

### 8.4 Suggested execution order across the v1 plans

1. **§5 Trace-surface foundation** — schema enrichment, cycle features parquet, determinism receipts, findings backreferences. Lands first because everything downstream emits into it.
2. **§3.1 Candle integrity validator** — smallest, fully independent. First real findings flowing into the trace.
3. **§4.2 Per-bar cost arrays** — architectural unlock for §4.3 and every future cost model.
4. **§4.3 Volume-share slippage** — consumes §4.2's machinery; this is where the "single flat number for a year" problem actually gets fixed.
5. **§3.2 Pinned canonical fixtures + content-hash receipts + data manifest** — small but touches the `Run` schema; cleaner after §3.1's defect types exist. Includes the feed/adjustment/calendar manifest from the review folds.
6. **§3.5 Lookahead-bias prober** — independent; runs late because it consumes the candle validator's hooks for finding emission.
7. **§4.12 Broker-rule findings (crypto-first)** — small follow-on to §4.3; uses the same fill-hook surface.

### 8.5 Follow-up task seed (for the team board / intake)

- **Equities readiness checklist** — point-in-time universe (§3.6), corporate-action ledger (§3.4), Alpaca SIP/IEX feed parity check (§3.3 variant), equity broker rules (PDT, margin, extended-hours, non-marginable). Revisit when equities go from "supported asset class" to "actively listed on marketplace."
- **Marketplace anti-overfitting suite** — hidden eval scenarios, walk-forward splits with embargo, metric stability over single-best, prompt/output leakage guard for agents, strategy simplicity penalty. Owned by the marketplace track.
- **Trust receipt renderer** — UX surface over findings + run manifest + parity profiles. Slot after the v1 accuracy wave is in place.
- **§4.9b live-micro-calibration harness** — gates signed marketplace attestations. Must include kill-switch, notional cap, whitelisted symbols, audit trail.
- **§4.10 funding/borrow accrual** — already on the perps-eval-simulator plan.
- **§4.11 market-impact research bet** — Almgren-Chriss / square-root. Skip until trade size justifies it.
- **AutoOptimizer meta-loop wave** — counterfactual replay tool, cross-run diff harness, failed-decision reservoir reader, feature-vector ML hooks. Storage shape lands in §5 v1; the loop tooling is a downstream wave.

---

## 9. Citations

- [NautilusTrader — Backtesting concepts](https://nautilustrader.io/docs/latest/concepts/backtesting/)
- [NautilusTrader — Enhanced order-fill simulation in backtesting (Issue #2194)](https://github.com/nautechsystems/nautilus_trader/issues/2194)
- [QuantConnect Lean — Supported slippage models](https://www.quantconnect.com/docs/v2/writing-algorithms/reality-modeling/slippage/supported-models)
- [QuantConnect Lean — VolumeShareSlippageModel.cs source](https://github.com/QuantConnect/Lean/blob/master/Common/Orders/Slippage/VolumeShareSlippageModel.cs)
- [QuantConnect Lean — MarketImpactSlippageModel reference](https://www.lean.io/docs/v2/lean-engine/class-reference/classQuantConnect_1_1Orders_1_1Slippage_1_1MarketImpactSlippageModel.html)
- [zipline — slippage.py source (VolumeShareSlippage, 2.5% cap, quadratic price impact)](https://github.com/quantopian/zipline/blob/master/zipline/finance/slippage.py)
- [vectorbt — Portfolio base API (per-bar slippage/fees arrays)](https://vectorbt.dev/api/portfolio/base/)
- [freqtrade — Lookahead analysis](https://www.freqtrade.io/en/stable/lookahead-analysis/)
- [freqtrade — Recursive analysis](https://www.freqtrade.io/en/stable/recursive-analysis/)
- [Alpaca Market Data FAQ](https://docs.alpaca.markets/us/docs/market-data-faq)
- [Alpaca forum — Data is not adjusted for splits despite adjustment="split" flag](https://forum.alpaca.markets/t/data-is-not-adjusted-for-splits-despite-adjustment-split-flag/7753/4)
- [Alpaca forum — Difference between IEX and SIP in historical data](https://forum.alpaca.markets/t/difference-between-iex-and-sip-in-historical-data/10191)
- [Corwin & Schultz 2012 — Bid-ask spread from daily H/L (paper)](https://acfr.aut.ac.nz/__data/assets/pdf_file/0016/570202/Efficient_Estimation_of_Bid_Ask_Spreads_from_OHLC_Prices-39.pdf)
- [MACE — Realistic Market Impact Modeling for RL Trading Environments (Almgren-Chriss + square-root)](https://arxiv.org/html/2603.29086)
- [Interactive Brokers — Slippage in Model Backtesting](https://www.interactivebrokers.com/campus/ibkr-quant-news/slippage-in-model-backtesting/)
- [QuestDB glossary — Almgren-Chriss optimal execution](https://questdb.com/glossary/optimal-execution-strategies-almgren-chriss-model/)
- [Alpaca Paper Trading docs — paper is a simulation, not a substitute for live](https://docs.alpaca.markets/docs/paper-trading)
- [Alpaca Market Data — historical bars adjustment + feed parameters](https://docs.alpaca.markets/us/reference/stockbars)
