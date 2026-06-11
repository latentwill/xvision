# Marketplace Phase 2 — In-UI Purchase (plan)

> Executes Phase 2 of `2026-06-11-marketplace-real-loop-ce-plan.md`. Branch
> `feat/marketplace-p2-purchase` (stacked on `feat/marketplace-reads-real`, PR #912).
> Subagent-driven; TDD; isolated `CARGO_TARGET_DIR=~/.cargo-target/xvision-mreads`.

**Goal:** a human buys a listing from the UI with their own wallet: connect →
ensure Mantle Sepolia → USDC balance (+testnet faucet) → buy. Primary path is
**gasless x402**: sign EIP-3009 `TransferWithAuthorization` in the browser
(`eth_signTypedData_v4`), backend relays `buyWithAuthorization` (server pays
gas). Fallback path when relay is unavailable: **approve+buy** from the user's
wallet (two txs). Receipts become real (decoded `Sold` events).

**Ground truths (from 2026-06-11 investigation):**
- `Marketplace.buy(listingId, recipient)`; `buyWithAuthorization(listingId, recipient, auth)` with M-2 guard `recipient == auth.from`; uses `transferWithAuthorization`.
- USDC EIP-712 domain: name `"USD Coin (xvn test)"`, version `"1"`, chainId 5003, verifyingContract = USDC addr. Typehash string: `TransferWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce)`. `faucet(uint256)` open mint, cap 10_000e6. decimals 6.
- Rust `BuyRequest { listing_id, recipient, authorization: Option<TransferAuthorization{from,to,value,valid_after,valid_before,nonce,v,r,s}> }` → `Erc8004MantleDriver::buy_listing` handles both paths and pre-checks M-2.
- Frontend has NO viem/ethers (add **viem**); `useWallet` exposes address only; the seam is `MarketplaceData.purchaseIntent(listingId)`; Buy CTAs: BrowseRoute `handleBuy`, LineageRoute `buyMutation` (→ `/marketplace/receipts/{txHash}`), ListingCard `onBuy`.
- `Sold(listingId idx, agentNftId idx, buyer idx, priceUSDC, sellerProceeds, protocolProceeds, licenseTokenId, payerKind, purchasePath)`.

### Task 1 (Rust): contracts in status + relay route + receipt route
- `GET /api/marketplace/status` gains `contracts: {marketplace, usdc, license_token, listing_registry, identity_registry}` (from env; nulls when unset) — frontend discovers addresses here, nothing hardcoded in the bundle.
- `POST /api/marketplace/buy` body `{listing_id: u64, recipient: "0x…", authorization: {from,to,value,valid_after,valid_before,nonce,v,r,s}}` (all hex-string encoded except v) → validate recipient==from (400), env gate (503, needs XVN_PUBLISHER_PK as relayer + XVN_MARKETPLACE_CONTRACT etc.) → `driver.buy_listing` → 200 `{tx_hash, license_token_id}`. Relay pays gas; signature is the buyer's authority — server never holds buyer funds.
- `GET /api/marketplace/receipts/:tx_hash` → fetch tx receipt via RPC (read-only provider), find+decode `Sold` log (alloy event decoding on the IMarketplace binding), join listing from snapshot → `{tx_hash, listing_id, agent_id, gen_art_seed, name, buyer, price_usdc, seller_proceeds_usdc, protocol_proceeds_usdc, license_token_id, purchase_path, block_time_unix}`; 404 unknown tx / no Sold log.
- TDD: unit tests for body validation/M-2/env-gate; receipt route 404 path; live paths in Task 4.

### Task 2 (frontend): chain lib + wallet extensions
- Add `viem` dep. New `lib/chain.ts`: mantle-sepolia chain object (5003, rpc https://rpc.sepolia.mantle.xyz), `getContracts()` (cached from /api/marketplace/status), `walletClient()`/`publicClient()` via `custom(window.ethereum)` / `http(rpc)`, `ensureMantleSepolia()` (`wallet_switchEthereumChain` 0x138b + `wallet_addEthereumChain` fallback), `usdcBalance(addr)`, `usdcAllowance(owner)`, `faucet(amount)` (sendTransaction via wallet), `approveUsdc(amount)`, `buyDirect(listingId, recipient)`, `signTransferAuthorization({from, to, valueUsdc6, validSecs})` → EIP-712 sign via viem `signTypedData` (random 32-byte nonce via crypto.getRandomValues; validAfter=0, validBefore=now+validSecs) returning the relay body. All amounts integer 6dp (bigint), converted at the edge.
- TDD: pure helpers (typed-data struct shape vs the normative typehash fields, usdc6 conversions, hex packing of v/r/s from a 65-byte signature) with viem mocked at the transport boundary.

### Task 3 (frontend): purchase flow + real receipts
- `ApiMarketplaceData.purchaseIntent(listingId)`: connected+chain ensured → balance check (insufficient → throw typed error the UI turns into an inline "Get test USDC" faucet action on testnet) → try relay (sign EIP-3009 → POST /api/marketplace/buy) → fallback to approve+buy when relay 503s → resolve `TxRef{txHash: real tx hash}`. Buy CTAs gain inline pending/error states (NO popups); LineageRoute already navigates to receipts on success.
- `ApiMarketplaceData.getReceipt(tx)`: GET /api/marketplace/receipts/:tx → map into the rich `Receipt` type (real fields from the route; install/share sections keep honest defaults); fixture fallback for fixture hashes.
- WalletRoute gains a USDC balance line + faucet button (inline).
- TDD: purchaseIntent paths (relay happy, relay 503→direct fallback, insufficient balance), receipt mapping, component states.

### Task 4: live verification + PR
- Start dashboard with full env; faucet 5 USDC to the op wallet if needed; build relay body by signing the typed data with `cast wallet sign --data` (op key) — POST /api/marketplace/buy → real gasless purchase on Mantle Sepolia; GET receipts/:tx returns decoded Sold; license balance increments. Frontend suites green. PR stacked on #912. Browser/MetaMask manual pass left to operator (documented in PR).

## Outcome (2026-06-11)

Live-verified on Mantle Sepolia: cast-signed EIP-3009 authorization relayed through
`POST /api/marketplace/buy` → gasless purchase tx `0x372cc61d…` (purchase_path=1),
receipt route decoded the Sold event (1.00 USDC, 0.95/0.05 split, full listing join
with real art seed), license balance 1→2. Adversarial review passed: M-2 enforced at
route/driver/contract layers; relayer exposure = bounded gas griefing (documented);
EIP-712 typed data byte-exact vs MockUSDC3009. Browser/MetaMask manual pass left to
operator. Nits deferred: localStorage address validation, transport-errors→400, 1970
timestamp on degraded block lookup, faucet mints price not deficit.
