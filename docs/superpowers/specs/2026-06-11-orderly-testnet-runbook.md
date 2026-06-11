# Orderly testnet runbook (MANUAL.md M6 — automated)

Status: **M6 exit criterion satisfied 2026-06-11** — full round trip
(buy 3380371731 → verified position → reduce-only close 3380371909 → flat)
completed against the real testnet through `xvn` → `OrderlyExecutor`.
Onboarding AND funding are scriptable end to end; the "manual web flow"
wording in MANUAL.md M6 is obsolete.

## What exists

| Piece | Where |
|---|---|
| Onboarding script (register account, announce ed25519 key, faucet, signed-read verify) | `scripts/orderly_testnet_onboard.py` |
| Funding script (Mantle Sepolia on-chain: token `faucet()` → approve → vault deposit) | `scripts/orderly_testnet_fund_mantle.py` |
| Round-trip smoke (portfolio → buy → close → portfolio via `xvn`) | `scripts/orderly-testnet-smoke.sh` |
| Credentials | 1Password `op://Olympus/xvision-orderly-testnet` (account_id, key, secret, address, broker_id, chain_id, base_url) |
| Executor | `crates/xvision-execution/src/orderly.rs` (`OrderlyExecutor`) — unchanged; testnet selected purely via `ORDERLY_BASE_URL` |

## Verified facts (2026-06-11)

- Wallet `0xb5d2…E553` (XVN Wallet, Olympus vault) registered with broker
  `woofi_dex`, chain 421614 (Arbitrum Sepolia), account id
  `0xb758e177fba3d2575e8abb723961c32131e96ba5dd1ed64716977e0ddcd6c67a`.
- EIP-712 off-chain domain uses verifyingContract
  `0xCcCCccccCCCCcCCCCCCcCcCccCcCCCcCcccccccC` for both `Registration` and
  `AddOrderlyKey`.
- The gateway accepts **both** url-safe and standard base64 ed25519
  signatures (tested with signatures containing `+`/`/`). The Rust
  executor's standard-base64 signing works as-is.
- `xvn fire-trade --venue orderly` and `xvn portfolio --venue orderly`
  reach the testnet with valid auth; order POSTs get structured venue
  responses (e.g. `-1005 quantity invalid` when equity is 0).
- Faucet: `POST https://testnet-operator-evm.orderly.org/v1/faucet/usdc`
  with JSON `{"broker_id":…,"chain_id":"421614","user_address":…}`
  (chain_id MUST be a string) returns success; the credit settles
  asynchronously and credits the **Orderly account ledger directly** (no
  on-chain deposit step — Orderly's own `examples/api/py` mints then
  immediately trades).
- Faucet caveats (2026-06-11): claims for brokers `woofi_dex`,
  `woofi_pro` AND `demo` all returned success but NONE credited within
  an hour. The endpoint is also per-IP rate-limited (HTML 429, long
  window). **Don't rely on it — use the on-chain Mantle path:**
  the test USDC token `0xAcab8129E2cE587fD203FD770ec9ECAFA2C88080` on
  Mantle Sepolia (5003) has a public `faucet()` (mints 1000 units,
  18-dp amount on a 6-dp-registered token — i.e. a huge test balance);
  approve + `Vault.deposit()` at
  `0xfb0E5f3D16758984E668A3d76f0963710E775503` (fee ~0.34 MNT) credits
  the account ledger in ~2 min. `scripts/orderly_testnet_fund_mantle.py`
  does all three steps.
- The FUNDED account is broker **`demo`** (Orderly's CLI default),
  account `0x2e3722ad…0480` — 1Password item updated to it. Trade SOL
  (or ETH) on testnet: the PERP_BTC_USDC book is frequently dead and the
  venue CANCELS unmatched market orders.
- Four executor bugs found+fixed by this validation (all with regression
  tests): notional-sent-as-base-qty on first orders; missing step-size
  (`base_tick`) alignment (-1104); CANCELLED-unfilled orders surfaced as
  fabricated fills; `client_order_id` over the 36-char venue cap (-1005,
  including silently-failing TP/SL brackets).

## Re-run from scratch

```bash
source .op_env
EVM_PRIVATE_KEY=$(op read 'op://Olympus/XVN Wallet/private key') \
  python3 scripts/orderly_testnet_onboard.py   # needs eth-account, pynacl, base58, requests

# then the round-trip smoke through the real executor path:
scripts/orderly-testnet-smoke.sh target/debug/xvn
```

Key rotation: re-running the onboarding script announces a fresh ed25519
key (old keys stay valid until expiration, 30 days). Update the 1Password
item when rotating.

## Live-run env recipe

A `mode=live` run with `broker_creds_ref="orderly_testnet"` needs both
venues' env: Orderly executes, Alpaca supplies the market-data stream.

```bash
export ORDERLY_KEY=$(op read 'op://Olympus/xvision-orderly-testnet/key')
export ORDERLY_SECRET=$(op read 'op://Olympus/xvision-orderly-testnet/secret')
export ORDERLY_ACCOUNT_ID=$(op read 'op://Olympus/xvision-orderly-testnet/account_id')
export ORDERLY_BASE_URL=$(op read 'op://Olympus/xvision-orderly-testnet/base_url')
export APCA_API_KEY_ID=$(op read 'op://Olympus/Alpaca API Key/API KEY')
export APCA_API_SECRET_KEY=$(op read 'op://Olympus/Alpaca API Key/Secret')
```
