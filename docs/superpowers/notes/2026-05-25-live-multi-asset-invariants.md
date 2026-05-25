# Live Multi-Asset Invariants — L2 Contract

**Date:** 2026-05-25
**Status:** Design note. Output of multi-asset-followups Phase 2 (safety gate).
**Owner of the gate:** `docs/superpowers/plans/2026-05-25-multi-asset-followups.md` Phase 2.
**Owner of the implementation that lifts the gate:** `docs/superpowers/plans/2026-05-25-cline-live-followups.md` §4 (Alpaca Paper Live L2).

## Purpose

Backtest is multi-asset (PRs #592/#593). Live execution is **single-asset by
hard wall** today: `LiveConfig::validate()` rejects `assets.len() != 1`
(`crates/xvision-engine/src/eval/live_config.rs`). This note is the contract the
cline-live L2 loop must satisfy **before** that wall is lifted. It is a
checklist for reviewers, not an implementation plan — the implementation lives
in the cline-live plan.

When all invariants below are met and tested, cline-live L2 may change the
`assets.len() != 1` rule to `assets.is_empty()` (allow N≥1) and update / delete
the `multi_asset_live_rejection_message_is_actionable` test. Until then, the
gate stays.

## Current gate (what Phase 2 shipped)

- `LiveConfig::validate()` returns `AssetCount { actual }` when `assets.len() != 1`.
- For `actual > 1` the operator-facing message names the multi-asset
  limitation and tells them to pick a single asset, and points here.
- The engine surfaces the `Display` message (not the `{:?}` Debug variant) via
  `validate_live_request_shape` in `api/eval.rs`, so CLI and dashboard operators
  see actionable text.
- `venue_label = Live` (real money) remains independently rejected.

## Invariants L2 must satisfy before lifting the single-asset wall

### 1. Broker position lookup is per-asset
Live position reads must be keyed by asset/symbol. A multi-asset run must never
read one asset's position and apply it to another. The broker surface must
expose positions per symbol; the executor must request the symbol it is acting
on.

### 2. Order submit carries an explicit, validated asset
Every live order must carry the asset it targets, validated against the run's
active asset set. No code path may default to `assets[0]` or
`asset_universe[0]` when the asset is ambiguous — ambiguity is an error, not a
fallback. (This mirrors the backtest non-goal: "no silent `asset_universe[0]`".)

### 3. Per-asset risk checks
Notional / order-count / leverage / max-loss limits (`SafetyLimits`) must be
evaluated against the correct asset's position and the run-level aggregate.
Define explicitly whether each limit is per-asset, portfolio-wide, or both, and
test the boundary where one asset is within limits and the portfolio is not.

### 4. Capital semantics: pooled vs per-asset
State which `CapitalMode` live supports at L2 and how equity/drawdown is
accounted across assets:
- **Pooled:** one capital pool; per-asset fills debit/credit the shared pool;
  drawdown is portfolio-level.
- **Per-asset:** each asset gets an isolated sleeve; document how the initial
  `capital.initial` is split and whether sleeves can rebalance.
L2 should implement exactly one to start and reject the other at validation
(consistent with backtest, where only `Pooled` is implemented).

### 5. Kill-switch / stop-policy fans out correctly
`StopPolicy` (time / bar / decision limits) and any kill-switch must apply to
the whole run, not per-asset, unless explicitly designed otherwise. A stop must
halt **all** asset streams and cancel/flatten per the venue's safety rules.
Define the flatten-on-stop behavior per asset.

### 6. Deterministic ordering across assets
Event emission, trajectory recording, and fill application must have a
deterministic per-timestamp asset ordering so replay and audit are stable
(cline-live L2 "maintain deterministic ordering").

### 7. Sparse / missing / reconnect handling
Per cline-live L2: handle sparse bars, missing symbols, and reconnect gaps
explicitly. A missing symbol for one timestamp must not stall or desync the
other assets.

### 8. Audit rows are per-asset
Every order/fill audit row records its asset, broker-native id, normalized fill
id, and venue label. A multi-asset run's audit trail must let an operator
reconstruct each asset's activity independently.

## Acceptance for lifting the gate

- Broker contract tests cover per-asset position reads, fills, and order audit
  rows (Alpaca paper for cline-live; cross-venue via testnet `T1`
  `BrokerSurface` hardening).
- A multi-asset Alpaca paper live run produces decisions and fills per active
  asset with **no** path falling back to simulated fills or a single asset.
- Single-asset live behavior is byte-stable (regression-guarded).
- The Phase 2 gate test is updated/removed in the same change that lifts the
  wall, so the rejection and its removal are reviewed together.

## Cross-references

- Gate code: `crates/xvision-engine/src/eval/live_config.rs`
  (`LiveConfig::validate`, `AssetCount`), `crates/xvision-engine/src/api/eval.rs`
  (`validate_live_request_shape`).
- Lifts the gate: `2026-05-25-cline-live-followups.md` §3 (L1 single-asset loop),
  §4 (L2 multi-asset loop).
- Cross-venue broker contract: `2026-05-25-testnet-venue-plan.md` T1.
