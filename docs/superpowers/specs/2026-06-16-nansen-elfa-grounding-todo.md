# Nansen + Elfa data tools — grounding TODO (verify before live use)

- **Date:** 2026-06-16
- **Status:** Open grounding items from the forward-only data-tools implementation
  (`docs/superpowers/plans/2026-06-14-nansen-elfa-forward-only-data-tools.md`).
- **Why:** The implementation used endpoint paths + contract addresses taken from
  the spec, NOT verified against live vendor docs / a smoke call. Per the Byreal
  CLI-grounding precedent (invented flags shipped a broken surface), these MUST be
  verified before relying on real Nansen/Elfa responses. Tests assert the routing
  *logic* (mockito), not the real endpoint correctness.

## 1. Nansen endpoint paths (verify against live Nansen v1/v1beta1 docs)

`crates/xvision-engine/src/tools/nansen.rs` routes by `as_of_date` presence
(present ⇒ historical/backtest ⇒ `/v1beta1`; absent ⇒ live ⇒ `/v1`):

| Tool | Live (`/api/v1`) | Historical (`/api/v1beta1`) |
|---|---|---|
| `nansen_smart_money_flow` | `smart-money/netflow` | `smart-money/historical-token-balances` |
| `nansen_token_screener` | `tgm/token-screener` | `token-screener/historical` |
| `nansen_flow_intel` | `tgm/flow-intelligence` | `tgm/historical-who-bought-sold` |

Verify: exact paths exist; request body field names (we send
`{ chain, token_address[, as_of_date] }`); that `as_of_date` is day-granular and
the snapshot/window semantics match; response shapes. For any metric lacking a
usable historical counterpart, change its historical route to
`degrade("backtest-unavailable for <tool>")` (the degrade plumbing already exists).

## 2. Elfa endpoint paths (verify against live Elfa v2 docs)

`crates/xvision-engine/src/tools/elfa.rs` (live only): `elfa_smart_mentions` →
`/v2/data/top-mentions` (query `ticker=`); `elfa_trending_tokens` →
`/v2/aggregations/trending-tokens`; `elfa_trending_narratives` →
`/v2/data/trending-narratives`. Verify paths + query params + the `x-elfa-api-key`
header + response shapes.

## 3. On-chain identity seed (verify addresses)

`crates/xvision-core/src/asset_registry.rs` `signal_asset_identity()` seeds:
`BTC`/`WBTC` → ethereum WBTC `0x2260…c599`; `ETH`/`WETH` → ethereum WETH
`0xc02a…cc2`; `USDC` → `0xa0b8…eb48`; `USDT` → `0xdac1…ec7`; `SOL` → solana mint
`So111…112`. Verify each chain slug + contract/mint against what Nansen expects
(esp. Nansen's native-token convention and chain naming). Unmapped assets degrade
(`{available:false, reason:"no on-chain identity mapped for X"}`) — extend the seed
for any additional whitelisted crypto as needed.

## 4. Secrets (operator)

Add `NANSEN_API_KEY` and `ELFA_API_KEY` to `.op_env` (1Password). Config
(`[[data_tools]]` in the runtime config) references the env-var NAME via
`api_key_env`; the secret never lands in config or the DB. Configure via
Settings → Tools (or the `/api/settings/data-tools` PUT).

## 5. Deferred (per G3)

Production run-level backtest *replay* trigger is NOT wired — `RunTrajectoryMode`
is `{ Live, Record }` only and engine-eval replay is deliberately out of scope
(`eval.rs` comment). The record half + dispatch-level replay determinism are done
and tested; the production re-run trigger lands with engine-eval replay.
