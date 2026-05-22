# Plan — Orderly multi-asset expansion (post-F18)

> **Status:** ready · 2026-05-22 · plan-only (no spec yet; design intent is captured inline because scope is mechanical).
> **Depends on:** F18 cascade (#533, merged 2026-05-22).
> **Track owner:** TBD.
> **Verification host:** Rust toolchain required — extndly-dev cannot run cargo (see project CLAUDE.md guardrail).

## Why now

F18 (#533) made `TraderDecision.asset` required and routed Alpaca executor per-decision. The same PR made Orderly explicitly reject any `td.asset != Btc` with `ExecutorError::NotActionable`, on the grounds that Orderly v1 (per ADR 0008) was hard-coded to `PERP_BTC_USDC`. That gate is the only thing blocking a strategy from running an ETH or SOL decision through Orderly. Removing it without designing the rest is unsafe — Orderly's perp surface, per-symbol min-notionals, signing, and position bookkeeping all need to extend together.

The marketplace narrative needs Mantle-native multi-asset (per FOLLOWUPS.md F18 + ADR 0010 framing); this plan unlocks Orderly to match what Alpaca already does.

## Out of scope

- New perp products that Orderly doesn't offer on Mantle EVM gateway. Inventory is intentionally locked to whatever `GET /v1/public/info` returns at expansion time; we expand only to symbols on Orderly's existing book.
- Risk-rule rewrites. The `MinNotional` rule already keys per-venue; per-symbol overrides go in `venue_limits` config not in new code.
- Live mainnet activation. Mainnet remains gated on Phase 9 eval clearing per ADR 0008. This plan covers testnet + paper-side expansion.
- Cross-asset margin / collateral rebalancing — Orderly handles USDC collateral pooled across symbols; the plan does NOT introduce per-asset margin accounting.

## Design intent (locked)

1. **Drop `OrderlyExecutor::symbol` field.** Today the executor is constructed with a single symbol and ignores `td.asset`. After this plan, the executor has no symbol field and derives the Orderly market string from `td.asset` on every submit.
2. **Symbol map: `AssetSymbol → "PERP_<X>_USDC"`.** Free function in `crates/xvision-execution/src/orderly.rs`, mirroring `alpaca_symbol_for`. Symbols not listed by Orderly (e.g. USDT, USDC, the SHIB/MATIC/BCH/LTC/AAVE/DOT/UNI long tail) reject with `NotActionable("Orderly does not list <ASSET>")` — same shape as the current BTC-only guard, just per-symbol.
3. **Orderly-supported set is data, not code.** The first cut hardcodes the known set (BTC, ETH, SOL, AVAX, DOGE, LINK — pulled from Orderly public docs). A follow-up track adds a periodic refresh from `GET /v1/public/info` to a SQLite cache so we never silently route to a delisted market.
4. **Position lookups become per-asset.** Today `OrderlyExecutor::submit` looks up the BTC position by `self.symbol`; this becomes a lookup keyed on `orderly_symbol_for(td.asset)`. `close_position` already takes `asset: AssetSymbol`, no signature change.
5. **`MinNotional` continues to govern.** Per-symbol min notionals already plumb through `RiskConfig.venues.orderly.min_notional_usd` — but the rule is global to the venue. The plan adds per-symbol min override under `RiskConfig.venues.orderly.per_asset.<asset>.min_notional_usd` (optional; falls back to venue-level). Only ship the override if any Orderly market's real min ≠ the venue baseline; otherwise this is YAGNI.
6. **Risk-gate `AssetWhitelist` rule is the single source of truth for "is this asset enabled overall."** Orderly's per-venue routing rejects with `NotActionable` (executor-level) only as a defense-in-depth. The first line of defense is the whitelist + Orderly's per-asset enable flag in `config/whitelist.toml` `[venues.orderly]` block.

## Tasks (checkbox tracking for executing-plans skill)

### Phase 1 — symbol mapping + helper

- [ ] **T1.1** Add free function `orderly_symbol_for(asset: AssetSymbol) -> Result<&'static str, ExecutorError>` to `crates/xvision-execution/src/orderly.rs`. Returns `"PERP_BTC_USDC"` / `"PERP_ETH_USDC"` / etc. for the supported set; returns `Err(ExecutorError::NotActionable(...))` otherwise. The supported set lives in a `const ORDERLY_SUPPORTED: &[(AssetSymbol, &str)] = &[...]`.
- [ ] **T1.2** Add inverse helper `asset_symbol_from_orderly(sym: &str) -> Option<AssetSymbol>` mirroring `asset_symbol_from_alpaca` for receipt parsing + position mapping.
- [ ] **T1.3** Unit tests in `orderly.rs` `mod tests`: every Orderly-supported asset round-trips both directions; unsupported AssetSymbol returns `NotActionable`; unknown wire string returns `None`.

### Phase 2 — executor refactor

- [ ] **T2.1** Remove the `symbol: String` field from `OrderlyExecutor<A>` struct and from its constructors (`from_env`, `connect`, `with_api`).
- [ ] **T2.2** Replace `self.symbol` in `submit` (currently lines ~649, ~692, ~720, ~733, ~767) with a local `let symbol = orderly_symbol_for(td.asset)?;` computed from the decision. `submit` returns the same `NotActionable` shape on unsupported assets but with a per-asset message.
- [ ] **T2.3** Delete the F18-era guard at orderly.rs:627–636 (the `if td.asset != AssetSymbol::Btc` block). T1.1 supersedes it.
- [ ] **T2.4** `close_position` already takes `asset: AssetSymbol`; verify it derives the symbol via T1.1 not from a stripped `self.symbol` field. Same for the `close_position` listing scan at orderly.rs:830 — match against `orderly_symbol_for(asset)` not `PERP_BTC_USDC`.
- [ ] **T2.5** `build_receipt` (orderly.rs:590) currently hardcodes `asset: AssetSymbol::Btc` on the receipt. Make it take `asset: AssetSymbol` as a parameter and thread it through all call sites.
- [ ] **T2.6** Remove the now-dead `const PERP_BTC_USDC` and the `v1 scope: BTC-only` mentions in the module docstring; replace with a docstring describing the per-asset routing model.

### Phase 3 — config + risk plumbing

- [ ] **T3.1** Extend `crates/xvision-risk/src/config.rs` `VenueLimits` with optional `pub per_asset: Option<BTreeMap<AssetSymbol, VenueLimits>>` (recursive only for `min_notional_usd`; never for nested per_asset).
- [ ] **T3.2** Update `RiskConfig::venue_limits(v: &str) -> &VenueLimits` callers: where a decision's asset is known, prefer `venue_limits.per_asset.get(asset)` and fall back to the venue baseline. This is contained inside the `MinNotional` rule's `evaluate`.
- [ ] **T3.3** Add `config/risk.toml` documentation comment block (no live config change yet) showing the `[venues.orderly.per_asset.eth] min_notional_usd = 5.0` shape so operators see the path without grepping the code.
- [ ] **T3.4** Per-asset enable in `config/whitelist.toml`: extend the existing `[venues.alpaca]` / `[venues.orderly]` per-asset blocks if any are missing — the format already exists for Alpaca (see `crates/xvision-risk/src/whitelist.rs:VenueEntry`). Orderly's block lands here too, populated with the T1.1 supported set marked `enabled = true`.

### Phase 4 — tests + fixtures

- [ ] **T4.1** Add a `submit_honors_trader_decision_asset` test in `orderly.rs` `mod tests` mirroring the Alpaca equivalent at `alpaca.rs:826` — set `td.asset = AssetSymbol::Eth`, submit, assert the captured request symbol is `"PERP_ETH_USDC"`.
- [ ] **T4.2** Add `submit_rejects_orderly_unsupported_asset` — `td.asset = AssetSymbol::Shib` (or any AssetSymbol not in T1.1's set) returns `ExecutorError::NotActionable` whose message names the asset.
- [ ] **T4.3** Add `close_position_routes_per_asset` — open an ETH position via mock, then call `executor.close_position(AssetSymbol::Eth).await` and assert the close order targets `PERP_ETH_USDC`.
- [ ] **T4.4** Existing BTC tests must continue to pass unchanged — Orderly BTC is still the dominant code path. Run `cargo test -p xvision-execution --test '*' 2>&1 | tee /tmp/orderly.log` and triage anything red against the F18-era fixtures.
- [ ] **T4.5** Integration test from CLI: `xvn fire-trade --venue orderly --asset ETH ...` should succeed end-to-end against the Orderly testnet mock (already wired in `xvision-cli/src/commands/fire_trade.rs` post-F18).

### Phase 5 — docs + FOLLOWUPS

- [ ] **T5.1** Update `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md` Orderly-section references (if any speak in BTC-only terms post-this-plan).
- [ ] **T5.2** Add a FOLLOWUPS.md entry F44 (next free slot): "Orderly market refresh — periodic `GET /v1/public/info` poll into SQLite cache; reject decisions targeting delisted markets at executor boundary, not at submit-time API error." Non-blocking; quality-of-life over the hardcoded set.
- [ ] **T5.3** Add a one-line entry under FOLLOWUPS.md F18 noting that this plan removed the BTC-only Orderly guard.

## Test plan (acceptance gates)

Run from the Rust build host (NOT extndly-dev):

```bash
cargo build --workspace
cargo test --workspace --no-fail-fast
cargo test -p xvision-execution --test '*'
cargo test -p xvision-risk --test '*'
```

Acceptance:

- `OrderlyExecutor::submit` routes per `td.asset` for every variant in the supported set.
- Unsupported assets return `ExecutorError::NotActionable` with a message naming the asset.
- `close_position` correctly resolves the per-asset symbol both on the order request side and on the position-scan side.
- Existing BTC golden-path tests pass without modification.
- `xvn fire-trade --venue orderly --asset ETH ...` succeeds against the testnet mock end-to-end.

## Risk + rollback

- **Risk:** Orderly's mainnet venue may have different min-notionals per symbol; if T3.1 / T3.2 don't ship correctly, an ETH order could be vetoed pre-submit when it would have filled (or vice versa). Mitigation: T3.3 documents the per-asset override path so operators can hot-patch the TOML.
- **Rollback:** revert the squash commit; F18's BTC-only `NotActionable` guard returns. No DB migration involved.
- **Mainnet readiness:** unchanged — mainnet still gated on Phase 9 eval clearing per ADR 0008. This plan does NOT relax that gate.

## Adjacent followups

- Orderly market-info refresh (filed as T5.2 → FOLLOWUPS F44).
- Per-asset position correlation (the `CorrelationCluster` risk rule already keys per-asset via the whitelist's `cluster` field; verify ETH is in `"eth"` cluster and SOL in `"sol"` so that multi-asset positions don't fight for the same cluster slot).
- Live (non-paper) ETH/SOL hookup once Phase 9 mainnet gate opens. Out of scope here.
