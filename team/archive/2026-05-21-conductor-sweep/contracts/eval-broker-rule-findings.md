---
track: eval-broker-rule-findings
lane: leaf
wave: v2e
worktree: .worktrees/eval-broker-rule-findings
branch: task/eval-broker-rule-findings
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/broker_rules.rs           # NEW — BrokerRuleSet trait + per-asset-class impls
  - crates/xvision-engine/src/eval/executor/backtest.rs      # order-emission hook — disjoint region with cost-model/intra-bar/trace tracks
  - crates/xvision-engine/src/eval/findings.rs               # new finding kinds — disjoint region with other tracks
  - crates/xvision-engine/tests/broker_rules_*.rs            # NEW
  - frontend/web/src/api/types.gen/**                        # ts-rs regenerated
forbidden_paths:
  - frontend/web/src/**                                      # no UI work this track
  - crates/xvision-data/**
  - crates/xvision-eval/**
  - crates/xvision-engine/src/eval/scenario.rs               # cost-model + candle-integrity own this
  - crates/xvision-execution/**                              # this is an offline check at simulator level — live execution rules live elsewhere
  - crates/xvision-engine/migrations/**                      # no schema change — broker findings are JSONL only
interfaces_used:
  - xvision-engine::eval::scenario::AssetClass
  - xvision-engine::eval::scenario::AssetRef
  - xvision-engine::eval::findings::Finding
parallel_safe: true
parallel_conflicts:
  - eval-cost-model-per-bar-and-volume-share (backtest.rs — disjoint regions; cost-model owns fill-price math, this track owns order-emission gate)
  - eval-trace-surface-foundation (backtest.rs + findings.rs — disjoint regions; foundation owns emit schema, this track adds new finding kind variants)
  - eval-intra-bar-fill-ordering (backtest.rs — disjoint regions; intra-bar owns fill-trigger, this track owns order-rejection)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine broker_rules_
  - pnpm --dir frontend/web typecheck
acceptance:
  - **`BrokerRuleSet` trait.** `fn validate(&self, order: &PendingOrder) -> Result<(), BrokerRuleViolation>`. Implementations: `AlpacaCryptoRules`, `AlpacaEquityRules` (no-op stub in v1; satisfies the trait by always returning Ok).
  - **Crypto-on-Alpaca rules (v1 surface).** AlpacaCryptoRules emits:
    * `unsupported_order_type` if order.kind is anything other than `Market`, `Limit`, `StopLimit`.
    * `unsupported_time_in_force` if order.tif is anything other than `Gtc`, `Ioc`, `Fok`.
    * `min_order_size_violation` if order.qty × order.price < `MIN_ORDER_NOTIONAL_USD` (default 1.0; sourced from Alpaca's published minimums per pair, hardcoded table in this contract).
    * `fractional_order_rounding` warning if order.qty has more than `MAX_FRACTIONAL_PRECISION` decimal places (Alpaca crypto: ~9 places for BTC, varies). Hardcoded table.
    * `broker_rule_violation` as the umbrella finding kind, with a `specific_rule: String` field naming the rule that fired.
  - **Equity stubs (v1 no-op, light up at marketplace).** `AlpacaEquityRules::validate` returns Ok always. Enum variants for `pdt_risk_or_rejection`, `extended_hours_not_supported`, `non_marginable_asset`, `short_not_allowed`, `insufficient_buying_power` exist and serialize/deserialize correctly, so when equities scenarios reach the marketplace and the rule impl wires up, no schema change is needed.
  - **Order-emission hook.** Before `simulate_fill` is called, the simulator calls `BrokerRuleSet::validate(order)`. On `Err(violation)`:
    * Emit a `broker_rule_violation` finding with `evidence_cycle_ids: [cycle_id]` and `produced_by_check = "broker:<specific_rule>"`.
    * Reject the order (do not fill); the strategy proceeds to the next decision as if the order never existed. (Operator review: this is "fail-honest", not "fail-soft" — the strategy should see the rejection in the next cycle's trace so it can learn.)
    * Increment per-run counter `broker_rejected_orders`. Surfaces in run metrics.
  - **Rule set selection.** Driven by `Scenario.asset_class`: `Crypto` → `AlpacaCryptoRules`; `Equity` → `AlpacaEquityRules`. (Currently Alpaca is the only supported venue; if a future track adds a non-Alpaca venue, this trait gains another impl.)
  - **ts-rs exports.** All finding kind variants and `BrokerRuleViolation` regenerated under `frontend/web/src/api/types.gen/`.
  - **Tests:**
    * One test per crypto-Alpaca rule kind: `unsupported_order_type` on a hypothetical `Stop` order; `unsupported_time_in_force` on a `Day` order; `min_order_size_violation` on $0.50 BTC order; `fractional_order_rounding` on `0.0000000123 BTC`.
    * Order rejection: a violating order does not appear in `trades.jsonl`; the cycle's intended action is recorded in `decisions.jsonl` but `outcomes.jsonl` shows no fill.
    * Equity stubs round-trip through serde (no panic on legacy runs).
    * Rule set selection: a `Crypto` scenario uses crypto rules; an `Equity` scenario uses equity rules.

---

# Scope

Research doc §4.12 — broker-rule findings, crypto-Alpaca v1. Catches
agent-authored strategies that emit orders the live venue would reject
(wrong order type, wrong TIF, below minimum, over-precision). Without
this, the simulator silently fills any order the strategy asks for —
a dishonesty bug that scales with strategy autogeneration. LLM
strategies in particular will happily propose orders that look fine in
indicator-space but are illegal at the venue.

Conceptually this is the reviewer's "third track" (broker fidelity)
compressed into a single finding family rather than a new
architectural layer.

# Out of scope

- Equity-specific rules (PDT, buying power, margin, extended-hours,
  non-marginable assets). Stubs land in this track; the impl waits for
  equity scenarios to reach the marketplace.
- Non-Alpaca venues (Orderly, Binance, etc.). The trait is general; the
  impl per-venue lands in whichever wave adds the new venue.
- Live broker validation (the broker_surface layer in
  `crates/xvision-execution`). That's a different problem — live trade
  rejection logging is `qa-trace-broker-spans` territory.
- Operator UX for rejected orders. The trace + finding is the
  substrate; the dashboard rendering ("strategy emitted 47 rejected
  orders, here are the rules") is a follow-up over the findings
  surface.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-broker-rule-findings status
git -C .worktrees/eval-broker-rule-findings log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-broker-rule-findings -b task/eval-broker-rule-findings origin/main
```

# Notes

The Alpaca crypto minimums table changes occasionally. Keep the
hardcoded table in `broker_rules.rs` with a comment pointing at the
Alpaca docs URL; flag for refresh during the next Alpaca-related
contract.

A future enhancement is to feed real Alpaca rejection responses into a
calibration loop (research doc §3.8 replay parity). Not v1 — note for
the post-V2C calibration intake.
