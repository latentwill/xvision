# Onboarding: live perps trading on Orderly (Mantle)

This guide takes a brand-new user from zero to a **live perpetual-futures trade
on Orderly Network, settled on Mantle.** It covers the wallet, the (free) Alpaca
market-data key, the gasless Orderly account onboarding, funding via WOOFi Pro /
Mantle, and where each value goes in xvision.

> **What this is.** Orderly Network is omnichain orderbook *perps* infrastructure.
> `woofi_pro` (WOOFi Pro) is a perps DEX powered by Orderly — that's the broker
> you register under. xvision executes orders on Orderly; **market-data bars come
> from Alpaca** (free), because Orderly is an execution venue, not a data feed.
> Funds are **non-custodial** — they live in the Orderly vault under your wallet.

---

## TL;DR — what you provide vs. what you don't

| You provide (yourself) | You do **NOT** need |
|---|---|
| An **EVM wallet** (a private key) — owns your Orderly account | Any xvision/operator 1Password secret |
| A **free Alpaca paper key** (market-data) | The platform deploy wallet / `XVN Wallet` |
| **USDC on Mantle** + a little **MNT** for gas | LLM API keys (those are server-side) |

There is **no shared "user credential"** to hand out. Each user onboards their own
wallet and generates their own Orderly trading key. The `op://` secrets in this
repo are developer/operator-only and are never needed by an end user.

---

## Prerequisites

- An **EVM wallet private key** (e.g. exported from MetaMask, or a fresh key
  reserved for trading). This wallet *owns* your Orderly account. Keep it safe.
- **Python 3** with `eth-account pynacl base58 requests` (for the one-time
  onboarding script):
  ```bash
  python3 -m venv ~/.cache/xvision-orderly-venv
  ~/.cache/xvision-orderly-venv/bin/pip install eth-account pynacl base58 requests
  ```
- The `xvn` binary (from `cargo build --bin xvn`, or your install).

---

## Step 1 — Free Alpaca market-data key

xvision streams 1-minute crypto bars from Alpaca for **every** live venue
(Orderly only executes the orders). The free tier is enough.

1. Sign up at <https://alpaca.markets> → switch to **Paper Trading**.
2. Generate an **API Key ID** + **Secret Key**.
3. Add it to xvision — **Settings → Brokers → Alpaca** (saved to
   `~/.xvn/secrets/brokers.toml`, owner-only), or export:
   ```bash
   export APCA_API_KEY_ID=...      # your Alpaca key id
   export APCA_API_SECRET_KEY=...  # your Alpaca secret
   ```
   (The same key is used only for the bar stream when you trade on Orderly.)

> The dashboard home page shows an **"Alpaca credentials not set"** nudge with a
> link to Settings → Brokers until this is done.

---

## Step 2 — Onboard your Orderly account (gasless, no signup page)

Orderly has **no website signup**. `https://api-evm.orderly.org` is a REST API,
not a portal — hitting it in a browser just returns a health response. You create
your account by **signing EIP-712 messages with your wallet** and posting them to
the API. **No funds move and no gas is spent** in this step.

Run the onboarding script with **mainnet** parameters:

```bash
cd /path/to/xvision
ORDERLY_TESTNET_BASE=https://api-evm.orderly.org \
ORDERLY_BROKER_ID=woofi_pro \
ORDERLY_CHAIN_ID=5000 \
EVM_PRIVATE_KEY=0x<your-wallet-private-key> \
  ~/.cache/xvision-orderly-venv/bin/python scripts/orderly_testnet_onboard.py
```

It prints JSON with three values you'll keep:

| Output field | What it is | Becomes env var |
|---|---|---|
| `account_id` | A `0x…` **hash** Orderly derives from your `address`+broker (NOT a username you pick) | `ORDERLY_ACCOUNT_ID` |
| `orderly_key` | `ed25519:<pubkey>` — your trading key's public half | `ORDERLY_KEY` |
| `orderly_secret` | base58 seed — the **private** trading key (keep secret) | `ORDERLY_SECRET` |

Then set the four env vars xvision reads:

```bash
export ORDERLY_KEY=ed25519:...        # from output
export ORDERLY_SECRET=...             # from output (secret!)
export ORDERLY_ACCOUNT_ID=0x...       # from output
export ORDERLY_BASE_URL=https://api-evm.orderly.org
```

Notes:
- The `EVM_PRIVATE_KEY` is used **only** to sign the one-time registration + key
  announcement. After that, xvision signs every trade with the **ed25519 trading
  key**, not your wallet key.
- The trading key is scoped `read,trading` with a **30-day expiry** — re-run the
  script to rotate.
- `account_id` is **chain-independent** (derived from address+broker). If the
  gateway ever rejects chain 5000 for registration, re-run with
  `ORDERLY_CHAIN_ID=42161` (Arbitrum One) — you get the same `account_id`.
- The script's `signature_b64_mode` may report `urlsafe`; that's expected and
  **not** a problem (it tries url-safe first; xvision's executor uses standard
  base64, which the gateway also accepts).

---

## Step 3 — Fund your account (USDC on Mantle → Orderly via WOOFi Pro)

Your Orderly account starts at $0. Collateral is **USDC on Mantle**, deposited
into the Orderly vault. Funds stay **non-custodial** under your wallet.

**3a. Get USDC.e onto Mantle (in your wallet):**
- Bridge from Ethereum/an L2 via the **Mantle bridge** (<https://bridge.mantle.xyz>), **or**
- Withdraw USDC from a CEX directly to the **Mantle** network.
- Mantle USDC.e token: `0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9`
- Also keep a little **MNT** in the wallet for the deposit gas.

**3b. Deposit into Orderly (easiest via WOOFi Pro):**
1. Go to **<https://pro.woofi.com>** and connect the **same wallet** you onboarded.
2. **Deposit** USDC — this credits your Orderly account (`account_id`). Settles in ~1–3 minutes.
3. (For reference, the Mantle-mainnet Orderly vault is `0x816f722424B49Cf1275cc86DA9840Fbd5a6167e9`.)

**How much?**
- Orderly's **minimum order is $10 notional** (SOL/ETH/BTC perps).
- The `xvn fire-trade` smoke caps an order at **20% of equity**, so to *smoke* a
  minimum order you need roughly **$55+** equity (20% × $55 ≈ $11 ≥ $10). For real
  strategy runs, fund for your intended position sizes. Funds are recoverable —
  withdraw on WOOFi Pro afterward.

---

## Step 4 — Verify and trade

**Confirm creds + signing (zero funds, no order):**
```bash
xvn portfolio --venue orderly      # signed read; prints your equity/positions
```

**Fire a tiny live round-trip (real money):**
```bash
xvn fire-trade --venue orderly --side buy --size-bps 2000 --asset SOL \
  --summary "first live orderly trade"
xvn close-position --venue orderly --asset SOL
xvn portfolio --venue orderly      # confirm flat
```
(The bundled `scripts/orderly-mainnet-smoke.sh` wraps this with a `DRY_RUN=1`
read-only mode and a real-money confirmation guard.)

---

## Credential reference

| Env var | Required | Secret? | Source |
|---|---|---|---|
| `ORDERLY_KEY` | yes | no (public key) | onboarding output |
| `ORDERLY_SECRET` | yes | **yes** | onboarding output |
| `ORDERLY_ACCOUNT_ID` | yes | no | onboarding output |
| `ORDERLY_BASE_URL` | yes (mainnet: `https://api-evm.orderly.org`) | no | you set |
| `APCA_API_KEY_ID` | yes (market data) | no | free Alpaca paper account |
| `APCA_API_SECRET_KEY` | yes (market data) | **yes** | free Alpaca paper account |
| `APCA_API_BASE_URL` | optional | no | defaults to Alpaca paper host |

---

## What the dashboard covers today (and what's still CLI/env)

| Surface | Alpaca | Byreal | Orderly |
|---|---|---|---|
| Settings → Brokers cred form | ✅ save/test/delete | ✅ save/delete | ⛔ **read-only status — set via env vars** |
| Live-run venue picker | ✅ `alpaca` | ✅ `byreal` (testnet) | ✅ `orderly_testnet` · ⛔ `orderly_mainnet` not yet in the picker |

**Known gaps / roadmap (tracked separately):**
1. **Orderly credential card in Settings → Brokers** — a save/test/delete form
   (parity with Alpaca/Byreal) so users don't hand-edit env vars. Backend needs a
   `POST /api/settings/brokers/orderly` route + an `orderly` field in
   `BrokersSecretsFile`; frontend needs an `OrderlyBrokerCard`. *Today Orderly is
   env-var-only.*
2. **`orderly_mainnet` in the live-run venue picker** (`eval-runs.tsx`), paired
   with a real-money confirmation, so the engine's mainnet arm is reachable from
   the dashboard (it currently runs via the `xvn` CLI).
3. **An in-app "connect a venue" onboarding step** — the first-run tour and the
   `/setup` wizard cover strategy authoring but not broker setup.

Until #1/#2 land, the **CLI + env-var path documented above is the supported
mainnet route** (it's the path proven end-to-end on 2026-06-14).

---

## Security model (why this is safe)

- Your **EVM private key never leaves your machine** and is used only to sign the
  one-time Orderly registration. xvision trades with the **ed25519 key** (scoped,
  30-day, revocable), not your wallet key.
- **Non-custodial:** collateral sits in the Orderly vault under your wallet; you
  withdraw on WOOFi Pro at any time.
- Stored creds live in `~/.xvn/secrets/brokers.toml` (owner-only) or your shell
  env — never committed.
