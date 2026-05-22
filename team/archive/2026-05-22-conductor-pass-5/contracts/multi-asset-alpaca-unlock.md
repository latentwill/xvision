---
track: multi-asset-alpaca-unlock
lane: integration
wave: alpaca-live-eval-2026-05-21
worktree: .worktrees/multi-asset-alpaca-unlock
branch: task/multi-asset-alpaca-unlock
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-core/src/trading.rs
  - crates/xvision-trader/**
  - crates/xvision-intern/**
  - crates/xvision-risk/**
  - crates/xvision-execution/src/alpaca.rs
  - crates/xvision-execution/src/orderly.rs
  - crates/xvision-eval/src/backtest.rs
  - crates/xvision-eval/src/harness.rs
  - crates/xvision-eval/src/ab_compare.rs
  - crates/xvision-eval/src/baselines/**
  - crates/xvision-eval/src/prober/lookahead.rs
  - crates/xvision-harness/src/lib.rs
  - crates/xvision-cli/src/commands/ab_compare.rs
  - crates/xvision-cli/src/commands/risk.rs
  - FOLLOWUPS.md
forbidden_paths:
  - migrations/**
  - frontend/web/**
interfaces_used:
  - xvision_core::trading::{TraderDecision, AssetSymbol}
  - xvision_risk::RiskLayer::evaluate
  - xvision_eval::backtest::BacktestConfig
  - xvision_eval::harness::BacktestRunConfig
  - xvision_execution::{AlpacaExecutor, OrderlyExecutor}
parallel_safe: false
parallel_conflicts:
  - touches Risk surface used by every executor path
verification:
  - cargo build --workspace
  - cargo test --workspace --no-fail-fast
acceptance:
  - TraderDecision.asset is required (not Option)
  - RiskLayer::evaluate takes no separate asset arg; reads decision.asset
  - BacktestConfig.instrument and BacktestRunConfig.instrument removed
  - Trader prompt schema requires `asset` JSON field; parser rejects missing
  - Intern briefing already carries asset (no schema change there)
  - Alpaca executor routes per decision.asset (no fallback default needed for submit)
  - Orderly executor validates decision.asset == Btc; rejects others (BTC-only product)
  - FOLLOWUPS.md F18 marked DONE
---

# Scope

Completes F18 cascade. F30 M1 added `TraderDecision.asset: Option<AssetSymbol>`
and unlocked the Alpaca executor for multi-asset; this track lands the
remaining cascade: trader prompt schema, intern briefing wiring, risk
parameter drop, executor pinning removal, BacktestConfig/BacktestRunConfig
instrument removal, and finally tightens `TraderDecision.asset` to a
required field.

# Out of scope

- Orderly multi-asset product surface (still PERP_BTC_USDC; rejecting non-BTC
  decisions is sufficient — adding ETH/SOL perps is a separate ADR).
- Frontend updates beyond what the schema change forces.
- Memory/observability changes.
- Plan/spec doc resurrection — original `2026-05-21-multi-asset-alpaca-unlock.md`
  plan never landed; this contract supersedes it as the work record.

# Sync-before-work ritual

Working on root checkout directly (no worktree) — single-author contract,
F18 scope is well-defined, no parallel risk.

# Notes

- 2026-05-22: contract authored after audit found F18 cascade still open
  despite F30 M1 unlock (`cc8b5028`) having landed.
