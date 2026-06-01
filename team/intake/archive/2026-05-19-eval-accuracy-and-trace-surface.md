# Intake — 2026-05-19 — eval accuracy & trace surface (V2E)

This is the first intake under the new V2E phase (eval accuracy & trace
surface — see `team/board-v2.md`). It decomposes V2E items 17–25 into
named tracks — the seven items from the research doc's recommended-wave
list plus two added 2026-05-20 from the operator review pass.

V2E is the second of two prerequisites for V3 autooptimizer (V2D memory
is the first). The autooptimizer's diff harness, failed-decision
reservoir, and feature-vector ML hooks all assume the trace shape from
the foundation track already exists.

## Revision history

- 2026-05-19 — initial intake covering V2E items 17–23 (research doc §8.4).
- 2026-05-20 — added two tracks promoted from "Out of this intake" or
  not previously on the V2 roadmap, driven by operator review of the
  research doc + LLM strategy eval results from
  `.worktrees/cli-workbench-wave-b/docs/tests/2026-05-19-llm-strategy-eval-notes.md`:
  - `eval-intra-bar-fill-ordering` (research doc §4.7) promoted into
    the wave. Rationale: without intra-bar fill ordering, any
    limit/stop/TP order still fills at next-bar open even after the
    per-bar cost model and volume-share slippage land — i.e. the
    "trader risk management is theatrical" problem persists through
    V2E. The cost machinery in `eval-per-bar-cost-arrays` plus
    `eval-volume-share-slippage` produces an honest *fill price* but
    not an honest *fill trigger*. Promoting §4.7 closes that gap and
    avoids retrofitting the trace foundation later to record fill
    branch / fill trigger provenance.
  - `eval-net-of-inference-cost-metric` added as a new V2E item
    (board-v2 item 25; item 24 is `eval-intra-bar-fill-ordering` above).
    Rationale: the operator review's most
    consequential point. Today the eval surface reports gross trading
    return but not net of inference cost. The LLM eval notes show
    causal v4 variants returning -0.1% to -1% gross across 49–100
    decisions per scenario; net of inference cost those runs are
    materially worse, and the eval surface doesn't communicate that.
    Without this metric, every "profitable" strategy in xvision is a
    half-truth.
- Notes on review feedback not folded in:
  - **Backtest smoke-test hardening as a standalone track** — proposed
    by the secondary review; rejected as scope. Strengthening tests of
    a model that is being replaced this wave is wasted work.
    Verification of the new model belongs inside each track's contract
    (see "Verification" below). Existing 9 tests at `backtest.rs:830–940`
    either continue to pass against the new model or get updated with
    an explicit `# Updated because <reason>` comment in the relevant
    track's PR.

## Source

- `docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md`
  — codebase audit + SOTA scan + review-derived accept/defer table +
  §8.4 suggested execution order. **This intake does not re-derive the
  research; per-track contracts cite the specific subsections they
  implement.**
- `docs/superpowers/specs/2026-05-08-eval-engine-design.md` — current
  eval engine surface that V2E enriches (run model §4, scenario format §5,
  SSE schema §9, findings extractor §11, open question 16 on canonical
  fixture pinning).
- `docs/superpowers/plans/2026-05-11-perps-eval-simulator.md` — already
  carries the funding/borrow accrual item (§4.10 in the research doc);
  V2E does not duplicate.
- `docs/hummingbot-eval.md` — order-lifecycle reference for partial
  fills (consumed in a later wave, not V2E).

## Current state (what already ships)

Pulled directly from the tree at intake time (full audit in research doc §1):

- **Fill simulator:** `crates/xvision-engine/src/eval/executor/backtest.rs::simulate_fill`
  (lines 692–772). `fill_price = next_open * (1 ± slip_bps/10_000)`; single
  taker fee on notional; no partial fills; no volume cap; no per-asset or
  per-bar variation.
- **Scenario knobs:** `crates/xvision-engine/src/eval/scenario.rs::VenueSettings`
  — `SlippageModel::Linear { bps }` or `None`, flat `Fees { maker_bps, taker_bps }`.
  Read once per run; applied identically to every fill.
- **Candle source:** `crates/xvision-data/src/fixtures.rs::load_ohlcv_fixture`.
  Parquet at `$XVN_PROBES_DIR` / `data/probes/<cache_key>.parquet`. **No
  validation pass** — no monotonic timestamp check, no OHLC sanity, no
  gap detection, no duplicate-bar guard.
- **Run artifacts:** `~/.xvn/runs/<run_id>/{config.json, metrics.json,
  trades.jsonl, decisions.jsonl, equity.parquet, findings.jsonl,
  events.jsonl}` (per eval design spec §4). SQLite tables `runs`,
  `run_metrics_summary`, `run_attestations`, `scenarios`. The `traces`
  table referenced in `CLAUDE.md` is keyed by `cycle_id` but content is
  thin.
- **Fill tests:** 9 tests in `backtest.rs:831–894` cover no-op, slippage
  direction, PnL booking, reversal. No tests for fee accuracy across
  notional sizes, volume-constrained fills, partial fills, latency,
  corporate actions, data gaps.

## Raw items → tracks

| Raw item | Track | Lane | Notes |
|---|---|---|---|
| Trace-surface foundation: schema enrichment, `cycle_features.parquet` sidecar, `determinism_receipts` table, findings `evidence_cycle_ids` backref, indexed columns on cycles | `eval-trace-surface-foundation` | foundation | Lands first; everything downstream emits into it. Research doc §5. |
| Candle integrity validator: OHLC sanity, gap detection, monotonic timestamps, duplicate guard, NaN/negative guard, zero-volume warn, wick-shock outlier | `eval-candle-integrity-validator` | foundation | Independent of trace foundation in scope but emits findings into the new schema. Research doc §3.1. |
| Per-bar cost arrays: scenario accepts optional `fee_bps` / `slip_bps` / `spread_bps` columns aligned to bars; simulator reads per-bar; default falls back to scenario constants | `eval-per-bar-cost-arrays` | foundation | Architectural unlock for volume-share slippage and any future regime-/volume-aware cost model. Research doc §4.2. |
| Volume-share slippage: `fill_price = price * (1 ± impact * volume_share²)` where `volume_share = min(qty / bar_volume, 0.025)`. Default `impact = 0.1` | `eval-volume-share-slippage` | leaf | Blocked by `eval-per-bar-cost-arrays`. Research doc §4.3. |
| Pinned canonical fixtures + content-hash receipts + manifest expansion (`feed` / `adjustment` / `timeframe` / `session_filter` / `calendar` / `timezone`) | `eval-pinned-fixtures-and-manifest` | leaf | Touches the `Run` schema; cleaner after validator's defect types exist. Research doc §3.2. |
| Lookahead-bias prober: full backtest baseline + per-signal sliced replay diff; emit `lookahead_suspected` finding when indicator values for cycle `t` differ between the two passes | `eval-lookahead-bias-prober` | leaf | Independent. Consumes the validator's finding-emission hooks. Research doc §3.5. |
| Broker-rule findings (crypto-first): per-asset-class rule table; emit `broker_rule_violation` / `unsupported_order_type` / `unsupported_time_in_force` / `min_order_size_violation` / `fractional_order_rounding` at order-emission time | `eval-broker-rule-findings` | leaf | Small (enum + per-asset rule table + fill-hook). Equity-specific kinds are no-op stubs until equities reach the marketplace. Research doc §4.12. |
| Adaptive intra-bar fill ordering for stops/TPs: gap-past-trigger fills at open; otherwise process `O→H→L→C` if `H` closer to `O` else `O→L→H→C`; limit orders only fill if price crosses; record `FillBranch` provenance per fill | `eval-intra-bar-fill-ordering` | leaf | **Promoted from "Out of this intake".** Without this, the per-bar cost model still produces dishonest fills for limit/stop/TP orders. Introduces a minimal `OrderState ∈ { Open, PartiallyFilled, Filled, Cancelled, Expired, Rejected }` enum (no queue model, partial-fill mechanics come later) and maker/taker aggressor classification (a limit at `open ± spread/2` that fills passively is maker; market/crossing limits are taker). Per-fill `fee_bps` becomes a function, not a constant. Research doc §4.7 + §4.5. |
| Net-of-inference-cost profitability metric + cost-aware compare/findings: persist per-decision `inference_cost_quote = tokens_in × input_price + tokens_out × output_price`; expose `gross_return_pct`, `inference_cost_quote_total`, `net_return_pct` on `Run` and `ComparisonReport`; emit `inference_cost_dominates_return { ratio, threshold }` finding when `|inference_cost_quote_total| > k × |gross_return_quote|` (default k=0.5) | `eval-net-of-inference-cost-metric` | leaf | **New V2E item (board-v2 item 25).** Token counts + `model_id` come from the trace foundation. Pricing comes from the existing OpenRouter pricing pull (`team/archive/2026-05-17-qa-operator/contracts/qa-openrouter-pricing-pull.md`). Surface in run summary card, compare view header, eval-review-agent payload, and `xvn eval show`. Driver: research doc §0/§5 trace-tokens are recorded but not surfaced as a top-line "is this worth running?" metric. |

## Dependency graph

```
eval-trace-surface-foundation
    │
    ├─→ eval-candle-integrity-validator  (independent in scope; emits into trace)
    │
    ├─→ eval-per-bar-cost-arrays  (foundation for volume-share + intra-bar)
    │       │
    │       ├─→ eval-volume-share-slippage
    │       │
    │       └─→ eval-intra-bar-fill-ordering  (consumes per-bar arrays; records FillBranch;
    │                                           classifies maker/taker)
    │
    ├─→ eval-pinned-fixtures-and-manifest  (cleaner after validator)
    │
    ├─→ eval-lookahead-bias-prober  (independent; pre-condition: audit baselines under
    │                                 crates/xvision-eval/src/baselines/ to confirm
    │                                 indicator code is side-effect-free between passes —
    │                                 audit lives inside the track contract, not after)
    │
    ├─→ eval-broker-rule-findings  (independent; uses same fill-hook surface as volume-share)
    │
    └─→ eval-net-of-inference-cost-metric  (depends on trace foundation's token / cost
                                             fields; otherwise independent; extends Run +
                                             ComparisonReport ts-rs types)
```

Conductor recommendation: ship `eval-trace-surface-foundation` first as a
solo wave, then fan the remaining eight tracks out in parallel where
contracts allow. Three are blocked or eased by sequencing (volume-share
and intra-bar fill ordering after per-bar costs; pinned fixtures after
validator); the rest can run concurrently.

Optional re-pairing the conductor may consider at decomposition: merge
`eval-per-bar-cost-arrays` + `eval-volume-share-slippage` into a single
contract (they share files and §4.3 strictly consumes §4.2). Likewise
merge `eval-candle-integrity-validator` + `eval-pinned-fixtures-and-manifest`
(both touch fixture-load and emit `data_defect`-family findings). That
brings the contract count down from 9 to 7 without changing the work,
at the cost of harder-to-kill-independently scope. Conductor's call.

## Out of this intake

- **§4.9 paper-parity calibration** and **§4.9b live-micro-calibration** —
  scheduled pre-marketplace, gating signed attestations. Separate intake
  when V2C marketplace readback work approaches; calibration must land
  before V2C item 10 (reputation + validation receipt write/readback).
- **§3.6 point-in-time universe / survivorship bias** — punted to
  equities-readiness follow-up. For v1 the eval engine should refuse
  equity scenarios that cross known delisting boundaries; that guardrail
  is a separate small task tagged for the equities track.
- **§3.4 corporate-action ledger** — equities-readiness follow-up.
- **§3.3 multi-source bar cross-check** — useful, not v1 critical.
  Defer until paper-parity drift findings (post-V2C) suggest source-side
  bug hunts are worth the cost.
- **§4.4 partial fills + order rollover** — full partial-fill mechanics
  (mid-bar carry, rollover with state transitions across N bars) defer
  to a follow-up wave. `eval-intra-bar-fill-ordering` introduces the
  minimal `OrderState` enum so the schema is ready; the carry-loop is
  deferred until the binding cap in `eval-volume-share-slippage` is
  producing real cap-hit metrics that motivate the loop's complexity.
- **§4.5 maker/taker aggressor-side fees** — promoted into
  `eval-intra-bar-fill-ordering` (per-fill `fee_bps` becomes a function
  of aggressor side, not a constant). No longer deferred.
- **§4.6 spread-aware fills (Corwin-Schultz)** — depends on §4.2's
  per-bar arrays existing. Available as an optional `spread_bps` source
  in `eval-per-bar-cost-arrays`; full default rollout is a follow-up.
- **§4.7 adaptive intra-bar fill ordering** — **promoted into the wave
  as `eval-intra-bar-fill-ordering`.** See Revision history (2026-05-20)
  for the rationale on why this is v1, not follow-up.
- **§4.8 latency model** — small; defer until the trace foundation makes
  the latency knob inspectable in receipts.
- **§4.10 funding/borrow accrual** — already on the perps-eval-simulator
  plan; not in V2E.
- **§4.11 market-impact research bet (Almgren-Chriss / square-root)** —
  optional; skip until trade size makes the quadratic term bind.
- **Trust-receipt renderer** — UX over the V2E findings substrate; slot
  after the v1 wave is in place. New follow-up entry on `team/board-v2.md`.
- **Marketplace anti-overfitting suite** — hidden eval scenarios,
  walk-forward splits with embargo, metric stability, leakage guards,
  simplicity penalty. Owned by the marketplace / V3 autooptimizer
  tracks, not V2E.
- **AutoOptimizer meta-loop tooling** — counterfactual replay tool,
  cross-run diff harness, failed-decision reservoir reader, feature-vector
  ML hooks. Storage shape lands in V2E foundation; the loop tooling is
  a downstream wave (V3).

## Verification (when a track lands)

Each decomposed track should, at minimum:

- Add or update Rust unit + integration tests under
  `crates/xvision-engine/tests/` covering the new finding kinds, the new
  cost-model branches, or the new schema fields. Specifically expected
  coverage (replaces standalone smoke-test hardening):
  - **`eval-per-bar-cost-arrays`:** fee accuracy at varying notionals;
    slippage sign per side under realistic positions; per-bar array
    consumption with and without scenario default; per-asset override
    precedence.
  - **`eval-volume-share-slippage`:** quadratic price impact math at
    boundary `volume_share` values; cap binding emits the
    `volume_share_excess` finding; behavior collapses to flat-bps at
    very low `volume_share`.
  - **`eval-intra-bar-fill-ordering`:** one test per `FillBranch`
    (gap_past, ohlc_high_first, ohlc_low_first, next_open_only); limit
    orders that don't cross do not fill; maker vs taker classification
    on passive vs crossing fills; `OrderState` enum round-trips through
    JSONL.
  - **`eval-candle-integrity-validator`:** one test per defect kind
    (`NonMonotonicTimestamp`, `DuplicateTimestamp`, `MissingBar`,
    `OhlcViolation`, `NegativeOrNanField`, `ZeroVolumeBar`,
    `WickShockOutlier`); `--allow-defective-data` flag bypasses the
    refusal.
  - **`eval-pinned-fixtures-and-manifest`:** manifest mismatch refusal
    on the compare view; receipt covers `(bars_content_hash ||
    manifest_canonical)` so a feed change without a bar change still
    produces distinct receipts.
  - **`eval-lookahead-bias-prober`:** positive case (a baseline that
    reads forward emits `lookahead_suspected`); negative case (a known
    side-effect-free baseline does not); side-effect-freedom audit
    documented in the contract.
  - **`eval-broker-rule-findings`:** one test per crypto-Alpaca rule
    kind (`unsupported_order_type`, `unsupported_time_in_force`,
    `min_order_size_violation`, `fractional_order_rounding`); equity
    stubs are no-op but enum values round-trip through JSONL.
  - **`eval-net-of-inference-cost-metric`:** `net_return_pct` math
    against fixed `gross_return` + `inference_cost` inputs;
    `inference_cost_dominates_return` finding emits at the configured
    threshold; pricing snapshot persisted (see open question 4 below).
- Existing 9 tests at
  `crates/xvision-engine/src/eval/executor/backtest.rs:830–940` either
  continue to pass against the new model, or each affected test is
  explicitly updated with an `# Updated because <reason>` comment in
  the relevant track's PR.
- Type-check the dashboard if a ts-rs surface changed:
  `pnpm --dir frontend/web typecheck && pnpm --dir frontend/web test --run`.
- Run `bash scripts/board-lint.sh` before pushing the contract edit
  (per `CLAUDE.md`).
- Verify determinism on at least one canonical scenario: re-run
  `(strategy_hash, scenario_id, bars_hash, seed)` and assert the
  `determinism_receipts` row is byte-stable (once
  `eval-trace-surface-foundation` lands). Any simulator change that
  affects metrics must intentionally bump `engine_version` and rebase
  receipts.
- Findings schema lint — every new finding `kind` lands with an example
  payload in `crates/xvision-engine/tests/findings_schema.rs` (or
  equivalent fixture path).
- Do **not** run `cargo` / `cargo build` / `cargo test` on remote / deploy
  hosts (per `CLAUDE.md`). Local build host or CI only.

## Open questions for the conductor

These resolve at decomposition, not in this intake:

1. **Re-pair tracks?** Whether to merge per-bar-costs + volume-share
   into one contract and validator + pinned-fixtures into another (see
   "Dependency graph" optional re-pairing). Cuts contract count from 8
   to 6.
2. **Does `inference_cost_dominates_return` gate the run, or just
   annotate?** Default proposal is annotate-only (no run blocking).
   Argument for gating: if a strategy's net return is dominated by
   inference cost, the run is dishonest as a "profitable" signal.
   Argument against gating: it's a UX surprise on cheap-model runs
   that happen to make tiny profits. Recommend annotate-only in v1;
   revisit when the marketplace adds an attestation gate.
3. **Receipt scope for manifest.** Should the determinism receipt hash
   `(bars_content_hash || manifest_canonical)` so a feed change without
   a bar change still produces distinct receipts? Recommend yes; flag
   to the `eval-trace-surface-foundation` and
   `eval-pinned-fixtures-and-manifest` contract authors so they don't
   re-derive independently.
4. **Inference-cost pricing snapshot scope.** Per-decision (more
   storage, perfectly fair across pricing changes) vs per-run (less
   storage, fair within a run but not across runs of the same strategy
   weeks apart). Recommend per-decision since the trace foundation is
   already a parquet sidecar — adding a `decision_inference_price_in`
   / `decision_inference_price_out` column is cheap and removes a
   class of unfair compares.

## Related artifacts

- `docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md` —
  primary research source; per-track contracts cite specific subsections.
- `docs/superpowers/specs/2026-05-08-eval-engine-design.md` — current
  eval engine surface this wave enriches. Open question 16
  (canonical scenario fixture pinning) is resolved by
  `eval-pinned-fixtures-and-manifest`.
- `docs/superpowers/plans/2026-05-11-perps-eval-simulator.md` — funding
  / borrow accrual lives here, not V2E.
- `crates/xvision-engine/src/eval/executor/backtest.rs` — primary
  edit site for tracks 19, 20, 23.
- `crates/xvision-engine/src/eval/scenario.rs` — primary edit site
  for tracks 19, 21.
- `crates/xvision-data/src/fixtures.rs` — primary edit site for
  tracks 18, 21.
- `team/board-v2.md` — V2 board with V2E items 17–25 in "Not yet
  decomposed" + V2E notes (items 24–25 added 2026-05-20).
- `.worktrees/cli-workbench-wave-b/docs/tests/2026-05-19-llm-strategy-eval-notes.md`
  — motivating LLM strategy eval results (causal v4 returns -0.1% to
  -1% gross over 49–100 decisions); driver for the net-of-inference-cost
  metric track.
- `CLAUDE.md` — terminology lock (`cycle_id`, `Strategy`, `Agent`); the
  trace foundation must keep `cycles` table naming.

## Next deploy snapshot

`main` at audit time: `c5a3cf1` (typed trader-output failures with raw
provider diagnostics, #180). Matches the previous intake's audit point;
deploy-clean. No code changes are part of this intake — every artifact
written today is process/docs only and does not move the runtime image.
