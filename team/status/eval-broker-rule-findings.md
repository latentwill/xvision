# Status: eval-broker-rule-findings

**Track:** V2E item 23 — broker-rule findings, crypto-Alpaca v1
**Worker:** claude-sonnet-4-6
**Updated:** 2026-05-21

## Status: complete

All acceptance items implemented and verified. PR open.

## What was built

- `crates/xvision-engine/src/eval/broker_rules.rs` (NEW): `BrokerRuleSet` trait,
  `AlpacaCryptoRules` (4 checks), `AlpacaEquityRules` (no-op stub v1),
  `PendingOrder`, `BrokerRuleViolation`, `BrokerViolationSeverity`,
  `rule_set_for_asset_class(AssetClass)`.

- `crates/xvision-engine/src/eval/mod.rs` (MODIFIED): added `pub mod broker_rules`
  and re-exports.

- `crates/xvision-engine/src/eval/executor/backtest.rs` (MODIFIED): order-emission
  hook inserted before `simulate_fill`; per-decision and aggregate findings emitted
  via `store.record_finding()`.

- `crates/xvision-engine/tests/broker_rules_crypto.rs` (NEW): 19 unit tests for
  AlpacaCryptoRules, equity stub, rule_set_for_asset_class selection.

- `crates/xvision-engine/tests/broker_rules_integration.rs` (NEW): 3 integration
  tests against full backtest executor + in-memory SQLite.

## Test results

- 18 lib unit tests: all pass
- 19 broker_rules_crypto tests: all pass
- 3 broker_rules_integration tests: all pass
- `cargo fmt --all -- --check`: clean

## Deferred: broker_rejected_orders in MetricsSummary

`MetricsSummary` has ~25 construction sites across the repo (outside allowed_paths).
Adding a field would break all of them. Count surfaces through:
1. Per-decision `broker_rule_violation` findings in JSONL
2. Run-level aggregate finding
3. `tracing::info!` log at finalize

A follow-up track can add `broker_rejected_orders: u32` to `MetricsSummary`
when it also updates all struct literal sites.
