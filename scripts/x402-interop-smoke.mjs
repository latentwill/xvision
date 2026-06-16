// x402 interop smoke — proves the xvision marketplace endpoint is spec-compliant
// by paying it with an OFF-THE-SHELF x402 client (not our own Rust client).
// This is the distinct interop check from spec §9: self-consistency (the Rust
// e2e) is necessary but not sufficient; a foreign client paying successfully is
// what proves Shape B speaks the wire protocol.
//
// Deps (ephemeral — do not add to any workspace package.json):
//   npm i x402-fetch viem
//
// IMPORTANT (unproven against Mantle): the endpoint advertises network
// "eip155:5000" (x402 V2 / CAIP-2). This smoke only passes with an x402-fetch
// built on @x402/evm that (a) speaks V2 CAIP-2 network ids and (b) supports
// Mantle. Legacy x402-fetch validates `network` against a named-chain enum that
// does NOT include Mantle and will reject it. Pin a known-good version and
// record it here once this passes green; until then, third-party interop is
// spec-shaped but unverified (the first-party MCP client is the proven path).
//
// Run against a running testnet dashboard with a funded buyer key:
//   XVN_MARKETPLACE_API=http://127.0.0.1:8080 \
//   BUYER_PK=0x<buyer-key-with-test-USDC> \
//   LISTING_ID=<real-listing-id> \
//   node scripts/x402-interop-smoke.mjs
//
// PASS = the off-the-shelf client completes the 402 → X-PAYMENT → settle
// handshake and the response carries an X-PAYMENT-RESPONSE header.

import { wrapFetchWithPayment } from "x402-fetch";
import { privateKeyToAccount } from "viem/accounts";

const base = process.env.XVN_MARKETPLACE_API ?? "http://127.0.0.1:8080";
const pk = process.env.BUYER_PK;
const listingId = process.env.LISTING_ID ?? "1";

if (!pk) {
  console.error("BUYER_PK is required (0x-prefixed key holding test USDC)");
  process.exit(2);
}

const account = privateKeyToAccount(pk);
const fetchWithPay = wrapFetchWithPayment(fetch, account);

const url = `${base}/api/marketplace/listings/${listingId}/x402`;
const res = await fetchWithPay(url, { method: "GET" });

if (!res.ok) {
  console.error("interop FAIL", res.status, await res.text());
  process.exit(1);
}

const paymentResponse = res.headers.get("x-payment-response");
if (!paymentResponse) {
  console.error("interop FAIL: settled but no X-PAYMENT-RESPONSE header");
  process.exit(1);
}

console.log("interop PASS — X-PAYMENT-RESPONSE:", paymentResponse);
console.log(await res.json());
