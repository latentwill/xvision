# `byreal-cli` Spot Grounding (Task C0)

**Probed:** 2026-06-15 via `npx -y @byreal-io/byreal-cli@latest` on macOS, node v25.2.1.
**Resolved CLI version:** **0.3.6** (from `meta.version` in every JSON envelope; package tag `@latest`).
**Package:** `@byreal-io/byreal-cli` — branded "Byreal CLMM DEX on Solana" (the SAME package `byreal_clmm.rs` already uses; spot swaps and CLMM share this CLI). Ships an "experimental / use at your own risk" banner.

This pins the real command surface + JSON shapes for `crates/xvision-execution/src/byreal_spot.rs`. Where it differs from the design spec's assumptions, **this doc wins**.

---

## Global conventions

- Output format: `-o json` (alias `--output json`). Also `--non-interactive`, `--debug`.
- **Envelope** (all `-o json` output):
  ```json
  { "success": true,
    "meta": { "timestamp": "...", "version": "0.3.6", "execution_time_ms": 275 },
    "data": { ... } }
  ```
  Note the extra `meta` object (the perps CLI lacked it). The `Envelope<T> { success, data: Option<T>, error }` struct from `byreal_clmm.rs` deserializes this fine — serde ignores the unknown `meta` field. The exact failure shape (`success:false`) was not triggered in probing; assume an `error`/`message` string as `byreal_clmm.rs` does, and treat any `success:false` as `ExecutorError::Rejected`.
- **Custody:** `wallet set` / `wallet setup` manage the keypair in the CLI's own config (keystore). **No private-key env var.** `--unsigned-tx --wallet-address <addr>` allows external signing (not used in v1). The xvision process never reads/logs the key.
- Network: pass `--network <net>` (we forward `BYREAL_SPOT_NETWORK` → `--network`), mirroring `byreal_clmm.rs`'s `BYREAL_CLMM_NETWORK`.

---

## `swap execute` — buy/sell

`byreal-cli swap execute [options]` ("Preview or execute a swap transaction"):

```
--input-mint <address>      Input token mint address
--output-mint <address>     Output token mint address
--amount <amount>           Amount to swap (UI amount, decimals auto-resolved)
--swap-mode <mode>          Swap mode: in or out (default: "in")
--slippage <bps>            Slippage tolerance in basis points
--raw                       Amount is already in raw (smallest unit) format
--dry-run                   Preview the swap without executing
--confirm                   Execute the swap
--unsigned-tx               Output unsigned transaction as JSON (no signing)
--wallet-address <address>  Wallet public key (for --unsigned-tx without keypair)
```

**Key fact:** `--amount` is a **UI amount with decimals auto-resolved** and `--swap-mode` defaults to **`in`** (amount = the INPUT-mint amount). So our model is exactly right and needs **no manual decimal scaling**:
- **Buy** (USDC→token): `--input-mint <USDC> --output-mint <token> --amount <usd_notional>` (UI USDC).
- **Sell** (token→USDC): `--input-mint <token> --output-mint <USDC> --amount <base_units>` (UI token).

→ The `decimals` field in the curated config is therefore **informational only** (display/validation); the CLI resolves decimals itself.

**`data` payload (real dry-run sample, USDC→SOL amount 10):**
```json
{
  "mode": "dry-run",
  "outAmount": "140693755",
  "inAmount": "10000000",
  "inputMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
  "outputMint": "So11111111111111111111111111111111111111112",
  "transaction": "",
  "priceImpactPct": "0.046433328738864764",
  "routerType": "AMM",
  "orderId": "Byreal-b0741f2e-d32b-4c61-8b5d-df6e97f3615c",
  "poolAddresses": ["9GTj99g9tbz9U6UYDsX6YeRTgUnkYG6GTnHv3qLa5aXq"],
  "uiInAmount": "10",
  "uiOutAmount": "0.140693755",
  "inAmountUsd": "$10.00",
  "outAmountUsd": "$10.00"
}
```

`SwapResult` deserialization (camelCase; all strings):
- `orderId` → stable id, present on dry-run AND (assumed) confirm → use as `broker_order_id`.
- `transaction` → tx signature; **empty `""` on dry-run**, populated on `--confirm` (UNCONFIRMED — verify at first live `--confirm`).
- `uiOutAmount` → output received in display units (parse `String`→`f64`).
- `priceImpactPct` → price-impact **percent** as a string (e.g. `0.0464` ≈ 0.046%). NOT bps.
- `mode` → `"dry-run"` vs (assumed) `"executed"`/`"confirmed"`.
- `routerType` confirms auto-routing (here `"AMM"`; RFQ/CLMM routes appear automatically — free, as the design predicted).

---

## Token price — `tokens list --search <mint>`

There is **no `token price` verb**. Price comes from `tokens list`:

```
byreal-cli tokens list --search <FULL_MINT_ADDRESS> -o json
```
(`--search` matches by token **address, full address only**; also supports `--sort-field price|tvl|volumeUsd24h|...`.)

**`data` payload (real sample, search SOL mint):**
```json
{
  "tokens": [
    { "mint": "So111...112", "symbol": "SOL", "name": "Wrapped SOL",
      "decimals": 9, "logo_uri": "...", "price_usd": 71.04,
      "price_change_24h": 0.0398, "volume_24h_usd": 356375.15, "multiplier": "1.0" }
  ],
  "total": 1, "page": 1, "pageSize": 10
}
```

→ `token_price(mint)`: run `tokens list --search <mint>`, deserialize `data` as `{ tokens: Vec<TokenEntry> }`, take `tokens[0].price_usd` (an `f64`). Error if `tokens` is empty.

---

## Wallet balance — `wallet balance`

`byreal-cli wallet balance` ("Query SOL and SPL token balance"). **No `--mint` filter** — returns ALL balances; filter client-side by mint. Sibling verbs: `wallet address|set|info|reset|setup`.

**JSON shape: UNCONFIRMED** — `wallet balance` needs a configured keypair, absent in the probe env. Assume `data` is a list of `{ mint, uiAmount }`-ish entries; the implementer must confirm the exact field names at the first authenticated run (live smoke) and adjust `BalancePayload`. Until then, `token_balance` is only exercised on the Live sell path and the manual smoke.

---

## Corrections vs the design spec (authoritative)

| Concern | Spec assumed | **Actual (use this)** |
|---|---|---|
| swap flags | `--input-mint/--output-mint/--amount/--slippage/--dry-run\|--confirm` | ✓ confirmed (plus `--swap-mode in` default) |
| amount semantics | base units, needs decimals | **UI amount, decimals auto-resolved** (no scaling; config `decimals` is informational) |
| price | `token price <mint>` | **`tokens list --search <mint>` → `data.tokens[0].price_usd`** |
| balance | `wallet balance --mint <mint>` | **`wallet balance` (all), filter by mint client-side; shape unconfirmed** |
| swap result | `{ signature, out_amount, price_impact_bps }` | **`{ orderId, transaction, uiOutAmount, priceImpactPct, mode, ... }` (camelCase strings)** |
| RFQ/auto-route | free inside swap | ✓ confirmed (`routerType` field; AMM here) |

**No funds were moved** — only `--dry-run` and read-only `tokens list` were run.
