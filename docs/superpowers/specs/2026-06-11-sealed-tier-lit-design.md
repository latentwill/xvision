# Sealed-Tier Strategy Delivery — Lit Protocol (Chipotle v3) Design

Date: 2026-06-11
Status: design — pending operator review
Bead: xvision-cgz (supersedes the v1 address-assertion caveat in the import route)
Related: PR #920 (sealed-tier publish guard + Kubo backend), CE plan Phase 3.

## 1. Problem

Open-tier listings publish the full strategy manifest as plaintext on IPFS — fine,
"open" means public. **Sealed tier** must deliver the strategy ONLY to license
holders. Today sealed publishing is hard-blocked (PR #920) precisely because the
publish path would pin plaintext. This design unblocks it: encrypt the bundle so
the public ciphertext is useless, and gate decryption on `LicenseToken.balanceOf
(wallet, listingId) > 0` — verified without trusting our server with keys.

Threat tiers we must address (from the 2026-06-11 discussion):
1. **Non-buyers scraping IPFS** — MUST be cryptographically impossible. (Solved.)
2. **Our dashboard server compromised** — MUST NOT leak the catalog. So the server
   must never hold decryption keys. (Solved: keys live in Lit's TEE network.)
3. **A legitimate buyer leaks plaintext** — irreducible (analog hole). NOT solved by
   crypto; mitigated by product design (soulbound licenses, re-encrypt on update,
   value in the live attestation/perf stream). Documented, not engineered away.

## 2. Why Lit Chipotle (v3)

Verified 2026-06-11 (research spike, sources in the bead). Key facts that shaped this:

- **Network: Chipotle** (`api.chipotle.litprotocol.com`), Lit v3, GA since 2026-04-01.
  This is a ground-up rebuild — gating is **TEE-based (Phala) + on-chain KMS on Base**,
  NOT the old threshold-crypto / ACC model. The prior Datil and Naga networks (and the
  `@lit-protocol/*` SDKs targeting them) are **dead** — we write none of that.
- **No SDK required.** Chipotle is a REST API. Encryption/decryption run *inside a Lit
  Action* (sandboxed JS in the TEE) via `Lit.Actions.Encrypt/Decrypt`; the access gate
  is ordinary JS in that action, pinned to an IPFS CID for immutability.
- **Mantle Sepolia (5003) needs no allowlist.** The action's runtime injects ethers v5
  and allows outbound RPC — our gate is literally `new JsonRpcProvider(mantle_rpc)` +
  `balanceOf`. Chain-agnostic by construction.
- **Pricing: ~$0.01/decrypt, reads free, $5 min, credits never expire.** A platform
  cost on the 5% commission, never a per-buyer fee. (Threshold TACo is the fallback but
  is mid-relaunch until Q3 2026 — not viable now; noted in the bead.)
- **Nothing self-hosted.** Opposite of Kubo: Lit is the hosted network; Kubo is our box.

**Vendor-churn risk is real** (3 breaking network migrations in 6 months). Mitigation is
architectural — §5 wraps Lit behind our own trait with an operator escrow escape hatch,
so a future migration (or a switch to TACo) touches one module and re-encryption tooling,
not the marketplace.

## 3. Flow

### Publish (sealed tier) — operator/seller node
```
canonical_manifest_json (plaintext, never leaves the publishing node unencrypted)
  → embed plaintext_hash = keccak256(canonical) INSIDE the payload  (post-decrypt integrity)
  → POST Lit encrypt action {pkpId, message: payload}  → ciphertext
  → Kubo put(ciphertext)  → cid
  → content_uri = "ipfs://{cid}"
  → onchain contentHash = keccak256(ciphertext bytes)   // hash of what's actually pinned
  → mint + createListing as today
```
The onchain `contentHash` commits to the **ciphertext** (so the bundle route can verify
the pinned blob is untampered without decrypting). The **plaintext** hash is carried
*inside* the encrypted payload and checked only after a licensed decrypt — proving the
decrypted manifest is the one the seller signed.

### Import (sealed tier) — buyer browser
```
buyer connects wallet → SIWE-style message (nonce + listingId + timestamp) → personal_sign
  → POST Lit gate action {pkpId, ciphertext (or cid), address, message, signature}
      action: verifyMessage(sig)==address  &&  freshness/nonce ok
            && balanceOf(address, listingId) > 0 on Mantle Sepolia
            → Lit.Actions.Decrypt → plaintext manifest
  → browser verifies keccak256(plaintext) == embedded plaintext_hash
  → POST /api/marketplace/listings/:id/import-sealed { manifest }  (license re-checked server-side)
  → import_strategy → new local ULID
```
Decryption happens **in the buyer's browser**, not our server. The existing
`import_strategy` engine fn is reused; only the manifest source changes (decrypted
client-side vs fetched plaintext).

## 4. Components

| Unit | Location | Change |
|---|---|---|
| `SealedBundleCrypto` trait | `crates/xvision-marketplace/src/sealed.rs` (new) | `encrypt(plaintext) -> Ciphertext`, `gate_action_cid()`, abstracts Lit; a `NoopSealed` for tests + an operator-escrow fallback impl. Lit calls are HTTP (reqwest), no SDK. |
| Publish sealed path | `routes/marketplace.rs` post_publish | Replace the §PR920 hard-400 for `tier==sealed` with: encrypt → pin ciphertext → contentHash=ciphertext hash. Requires Lit config present (else keep 400 "sealed unavailable: Lit not configured"). |
| Lit config | `chain_config.rs` | New `lit: Option<LitConfig{ api_base, api_key, pkp_id, gate_action_cid }>` from env `XVN_LIT_*`. Resolved at startup like the rest. |
| Sealed import | `routes/marketplace.rs` | `POST /api/marketplace/listings/:id/import-sealed {manifest}` — server RE-CHECKS balanceOf (defense in depth; the browser already gated) → verify nothing (manifest came from a licensed decrypt) → import_strategy. The plaintext hash check is the browser's job; server trusts the license re-check. |
| Frontend sealed import | `features/marketplace/lib/sealed.ts` (new) + InstallSteps | SIWE sign + POST gate action + integrity check + POST import-sealed. The "Decrypt sealed bundle" step (removed in P3 as a fake) returns as real. |
| Gate action | `contracts/lit-actions/sealed-gate.js` (new, version-controlled, pinned to IPFS) | The §3 import action JS. Pinned CID is the immutable gate; its CID is `XVN_LIT_GATE_ACTION_CID`. |

## 5. Risk controls baked in
- **Server never decrypts** → tier-2 threat closed even if dashboard is popped.
- **`SealedBundleCrypto` trait + escrow fallback** → vendor migration = one module.
- **Replay/auth is DIY** (Lit no longer provides session-sig primitives): the SIWE
  message MUST carry a fresh nonce + listingId + short expiry; the gate action validates
  all three. This is the easiest thing to get wrong — it gets its own tests.
- **ciphertext contentHash** lets the existing bundle route integrity-check sealed blobs
  without decrypting (returns `{verified, encrypted: true}`, no manifest).
- **1 MB Lit Action payload limit** — strategy manifests are KBs; add a guard + chunking
  TODO only if a manifest ever approaches it.

## 6. Out of scope / explicit non-goals
- Stopping a legitimate buyer from leaking (analog hole — product mitigations only).
- Re-encryption-on-`updateListing` tooling (needed before sealed updates ship; follow-up).
- Self-hosting the Lit TEE stack (optional, verification-purist only).
- Mainnet capacity planning (testnet cost negligible; revisit at mainnet).

## 7. Operator setup checklist (Chipotle)
1. Account at `dashboard.chipotle.litprotocol.com`; save the account API key.
2. Fund min $5 (card or crypto; LITKEY on Base = 25% off). ~$0.01/decrypt thereafter.
3. Create a **vault PKP**; note `pkpId`.
4. Commit `contracts/lit-actions/sealed-gate.js`, pin it to IPFS (your Kubo), record the CID.
5. Create a **group** binding {PKP, gate-action CID, a tightly-scoped usage API key}.
6. Set dashboard env: `XVN_LIT_API_BASE`, `XVN_LIT_API_KEY`, `XVN_LIT_PKP_ID`,
   `XVN_LIT_GATE_ACTION_CID`. Sealed publishing unlocks; absent → stays 400.

## 8. Implementation phasing (separate plan after this design is approved)
1. `SealedBundleCrypto` trait + Lit HTTP client + config + tests (Noop + escrow).
2. Gate action JS + its own test harness (sign/replay/balance matrix).
3. Sealed publish path (encrypt+pin+ciphertext-hash) + bundle-route encrypted verify.
4. Frontend decrypt+import + the revived "Decrypt sealed bundle" install step.
5. Live verify on Mantle Sepolia (publish sealed → buy → browser decrypt → import).
