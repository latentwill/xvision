# Runbook: marketplace mainnet end-to-end test (on the xvn host)

The marketplace contracts are **live on Mantle mainnet** (chain 5000) and the
browser buy path now supports mainnet. This runbook takes the running dashboard
on the **xvn host** from "deployed contracts" to a verified
`publish → buy → license` round-trip.

Contracts are in `config/mantle.toml`; the live addresses are baked into the
env block below.

---

## 0. Prerequisites

- A **funded relayer key** for `XVN_PUBLISHER_PK` — the server signs publish txs
  and relays the gasless EIP-3009 buy, so it pays MNT gas. The deploy wallet
  (`XVN Wallet`, `0xb5d2…E553`) already holds MNT and works.
- (Buyer side, for a **paid** listing) the buyer wallet needs **USDC.e on Mantle**.
  For the simplest proof, publish a **free** listing (`price = 0`) — then buy
  needs no USDC and still mints the license.
- (Optional) IPFS pinning for the bundle: set `XVN_IPFS_API_URL` (Kubo) or
  `PINATA_JWT`. Without it, publish falls back to a local `xvn://` content URI
  (fine for a non-sealed test). **Use a public (non-sealed) listing** so the Lit
  `XVN_LIT_*` config isn't required.

---

## 1. Build the image for mainnet

The browser bundle's target chain is **baked at build time**. Build with the
mainnet flag (and an empty subgraph URL so the SPA reads the live **backend
indexer**, not the testnet Goldsky subgraph):

```bash
VITE_MARKETPLACE_NETWORK=mainnet \
VITE_MARKETPLACE_SUBGRAPH_URL="" \
  scripts/deploy-image.sh --with-identity --push root@<xvn-host>
```

`--with-identity` includes the on-chain stack; `VITE_MARKETPLACE_NETWORK=mainnet`
makes `lib/chain.ts` use chain 5000 + the real USDC.e EIP-712 domain
(`"USD Coin"`/v2). After the image lands, recreate the dashboard container so it
picks up the new image **and** the env below.

---

## 2. Runtime env on the xvn host (wakes the indexer)

Set these on the dashboard container/service, then restart. Without them the
marketplace routes return 503 and the UI shows demo fixtures.

```bash
# Chain core + relayer (required)
export XVN_RPC_URL=https://rpc.mantle.xyz
export XVN_CHAIN_ID=5000
export XVN_PUBLISHER_PK=<funded relayer private key>   # pays gas; needs MNT

# Deployed addresses (chain 5000)
export XVN_IDENTITY_REGISTRY=0xa0c9A5a00cbD5bcC7DBc92acE170356471352f2E
export XVN_REPUTATION_REGISTRY=0xF0Eeb3412cbaeaD5E56fB35e1b2EAb7766f84Fce
export XVN_LISTING_REGISTRY=0xF491b6102F5c50Db46AeEc7fFb3D520aaF2f0151
export XVN_MARKETPLACE_CONTRACT=0xd02Cc76515021D2a140e7EfDadc561dA4ae57BFB
export XVN_MARKETPLACE_USDC=0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9
export XVN_LICENSE_TOKEN=0x060642CbFae3F3a058aD8793b71E7540d8a71007
export XVN_EVAL_ATTESTATION=0xB9cb44FD3c8F4e4025eBf8f640C56402546b91Fc
export XVN_PLATFORM_AGENT_TOKEN_ID=0

# Indexer start block (skip a full-chain scan from genesis)
export XVN_MARKETPLACE_DEPLOY_BLOCK=96658559
```

---

## 3. Verify the indexer woke

```bash
curl -s http://<xvn-host>/api/marketplace/status | jq
```

Expect `active: true` and a `contracts` block with the marketplace + USDC
addresses above (not null). If routes 503, an `XVN_*` var is missing/unparseable
(check `XVN_PUBLISHER_PK` is a valid key) — the log prints which.

---

## 4. End-to-end test (publish → buy → license)

In the dashboard, connected wallet on **Mantle mainnet (5000)**:

1. **Publish** a strategy listing (start with `price = 0`, public/non-sealed).
   The server pins the bundle + calls `ListingRegistry.createListing`. Confirm a
   listing id + tx on [Mantlescan](https://explorer.mantle.xyz).
2. **Buy** it from a second wallet (or the same). For a free listing the buyer
   signs nothing on-chain beyond the relayed call; for a paid one, the browser
   signs an **EIP-3009 `transferWithAuthorization`** (now using the mainnet USDC
   domain) and `POST /api/marketplace/buy` relays it.
3. **License** — confirm the buyer received an ERC-1155 license token
   (`LicenseToken.balanceOf(buyer, listingId) == 1`) and the receipt page shows
   the on-chain tx. Spot-check:
   ```bash
   cast call 0x060642CbFae3F3a058aD8793b71E7540d8a71007 \
     'balanceOf(address,uint256)(uint256)' <buyer> <listingId> --rpc-url https://rpc.mantle.xyz
   ```

---

## Notes / known limitations

- **Sealed (encrypted) bundles** additionally need the `XVN_LIT_*` Chipotle
  config; the sealed-decrypt RPC now follows the active chain. Use a **public**
  listing for the first mainnet test to avoid that dependency.
- **USDC.e** `0x09Bc…0dF9` is verified on-chain as real EIP-3009 "USD Coin"/v2;
  if a different mainnet USDC is preferred, repoint via `Marketplace.setUsdc`
  (owner = the deploy wallet) and update `XVN_MARKETPLACE_USDC` + the build.
- This is the **hackathon-grade** deploy: the operator EOA is proxy admin + fee
  recipient (no audit/timelock/multisig). Not production-secured.
