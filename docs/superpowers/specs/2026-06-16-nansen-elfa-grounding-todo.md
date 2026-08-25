# Nansen + Elfa data tools — grounding TODO (verify before live use)

- **Date:** 2026-06-16
- **Status:** Fully resolved 2026-08-25. §1 Nansen routes audited against live
  docs and FIXED in `tools/nansen.rs`. §2 Elfa and §3 identity seed VERIFIED.
  §4 secrets resolved via Settings → Tools API-key storage. Only §5 remains
  deferred by design.
  Original plan:
  `docs/superpowers/plans/2026-06-14-nansen-elfa-forward-only-data-tools.md`.
- **Why:** The implementation used endpoint paths + contract addresses taken from
  the spec, NOT verified against live vendor docs / a smoke call. Per the Byreal
  CLI-grounding precedent (invented flags shipped a broken surface), these MUST be
  verified before relying on real Nansen/Elfa responses. Tests assert the routing
  *logic* (mockito), not the real endpoint correctness.

## 1. Nansen endpoint paths — FIXED 2026-08-25 (routes rewritten per audit)

All findings below were applied to `crates/xvision-engine/src/tools/nansen.rs`:
live screener path corrected, smart-money backtest degrades (no vendor
history), request bodies rebuilt for the current schemas, historical
who-bought-sold uses `date_range`, historical screener filtered client-side by
`token_address` with honest truncation handling. Tests updated + extended.

## 2. Elfa endpoint paths — VERIFIED 2026-08-25 (done)

Checked against live https://docs.elfa.ai (REST reference + per-endpoint
pages): all three paths, the `ticker=` query param on top-mentions, and the
`x-elfa-api-key` auth header match exactly. Base URL `https://api.elfa.ai`
confirmed. Response envelope is `{ success, data, metadata }`.


`crates/xvision-engine/src/tools/elfa.rs` (live only): `elfa_smart_mentions` →
`/v2/data/top-mentions` (query `ticker=`); `elfa_trending_tokens` →
`/v2/aggregations/trending-tokens`; `elfa_trending_narratives` →
`/v2/data/trending-narratives`. All confirmed against the live v2 docs.

## 3. On-chain identity seed — VERIFIED 2026-08-25 (done)

All five addresses/mints confirmed canonical (Ethereum mainnet / Solana
mainnet); chain slugs `ethereum`/`solana` match Nansen's current lowercase
chain enums.


`crates/xvision-core/src/asset_registry.rs` `signal_asset_identity()` seeds:
`BTC`/`WBTC` → WBTC `0x2260…c599` ✓; `ETH`/`WETH` → WETH `0xc02a…cc2` ✓;
`USDC` → `0xa0b8…eb48` ✓; `USDT` → `0xdac1…ec7` ✓; `SOL` → wrapped-SOL mint
`So111…112` ✓. Remaining caveat only: Nansen models native tokens via
`include_native_tokens` flags rather than contract addresses — the WETH
representation is a deliberate proxy, fine for signal purposes.

## 4. Secrets (operator) — RESOLVED 2026-08-25 (Settings → Tools now stores keys)

PUT `/api/settings/data-tools` accepts an optional plaintext `api_key` per
entry: persisted to `$XVN_HOME/secrets/data_tools.toml` (mode 0600), exported
into the daemon env under `api_key_env`, re-hydrated at CLI/dashboard startup.
GET surfaces only an `api_key_set` presence flag. Env vars remain the
highest-priority source.

## 5. Deferred (per G3) — re-confirmed still deferred 2026-08-25

`RunTrajectoryMode` remains `{ Live, Record }` only (engine `api/eval.rs`);
replay wiring comment unchanged in `eval.rs` (~line 2507). Still lands with
engine-eval replay.

