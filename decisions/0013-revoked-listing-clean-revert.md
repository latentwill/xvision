# ADR 0013 — Revoked listing mid-flight produces a clean revert (EIP-3009 nonce unspent)

## Status: Accepted (2026-05-27)

Relates to the marketplace contract surface
([`docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`](../docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md))
§4.5 and `Marketplace.buyWithAuthorization` (`contracts/src/Marketplace.sol`).
Supersedes the §4.5 prose, which was corrected in the same change.

## Context

The x402 buy path settles a sale in one transaction: a buyer signs an off-chain
EIP-3009 `TransferWithAuthorization`, and a facilitator submits it via
`Marketplace.buyWithAuthorization`, which calls
`USDC.transferWithAuthorization(...)` to pull funds, then splits and mints.

The surface spec §4.5 originally claimed that if a listing is **revoked between
the 402 issuance and settlement**, the tx "reverts cleanly" and "the EIP-3009
nonce is consumed but USDC isn't moved." That is mechanically impossible:

- EIP-3009 burns the authorization nonce **inside** `transferWithAuthorization`,
  atomically with the transfer. There is no path where USDC marks the nonce used
  without also moving the funds in the same call.
- Wrapping the call and reverting afterward would roll back the nonce write too
  (revert is all-or-nothing within a tx).

So the only ways to literally honor "nonce consumed, USDC not moved" would be to
hold a separate `cancelAuthorization` signature from the payer (we do not), or to
let the transfer execute and then revert — which lands back at "nonce unspent."

## Decision

`Marketplace.buyWithAuthorization` checks `Listing.revoked == false` **before**
calling `transferWithAuthorization`. Consequence:

1. A revoked-mid-flight purchase reverts cleanly (`ListingRevoked(listingId)`).
2. USDC never sees the call, so **the nonce stays unspent** and the signed auth
   remains replayable.
3. Replay is only possible against a **non-revoked** listing. Revocation is
   **monotonic** (one-way: `revokeListing` sets `revoked = true` and never
   unsets it). Therefore a replay of the same auth against the same listing will
   always re-hit the `revoked` check and revert. **Net replay risk: zero**, for
   as long as revocation state is monotonic — which it is, and which this ADR
   makes load-bearing.

This is the only correct mechanical interpretation and is kept as-is.

## Invariant (must hold for the safety argument)

- **Listing revocation is monotonic.** `revokeListing` is the only mutator of
  `Listing.revoked`; it sets it `true` and there is no "un-revoke". Any future
  change that reintroduces un-revocation MUST revisit this ADR — it would make a
  parked auth replayable against a re-activated listing.

## Consequences

- The §4.5 prose is corrected to match.
- `test/integration/SaleFlow.t.sol::test_revokedBetween402AndSettlement_revertsCleanly`
  asserts the nonce is untouched and no funds move.
- No contract change required — the implementation was already correct; this ADR
  records why and pins the monotonic-revocation invariant.
