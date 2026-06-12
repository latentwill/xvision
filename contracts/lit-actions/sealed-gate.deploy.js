// GENERATED — do not edit. Source: sealed-gate.js (run build-deploy.mjs).
// This is the plain-script form Lit's TEE runs; its IPFS CID is the gate.
/**
 * sealed-gate.js — Lit Protocol v3 ("Chipotle") decrypt gate action.
 *
 * THE CID OF THIS FILE IS THE IMMUTABLE GATE. The operator pins this exact
 * file to IPFS and sets the resulting CID as `XVN_LIT_GATE_ACTION_CID`
 * (mirrored on-chain / in listing metadata). The PKP's auth method trusts
 * only this CID, so the *only* way to decrypt a sealed bundle is to satisfy
 * the checks below. Changing one byte changes the CID and breaks the gate —
 * which is the point.
 *
 * ## What the gate enforces (in order)
 *   1. SIGNATURE: ethers.utils.verifyMessage(message, signature) === address
 *      (compared lowercased). Proves the caller controls `address`.
 *   2. MESSAGE BINDING / FRESHNESS: `message` is a structured SIWE-ish string
 *      that embeds the listingId, a nonce, and an expiry unix timestamp. The
 *      gate rejects if the message is expired, or if its listingId does not
 *      match the listingId the caller is trying to unlock. This binds the
 *      signature to one listing and a short time window so a captured
 *      signature cannot be used for a different listing or after expiry.
 *   3. LICENSE: an ERC-1155 `balanceOf(address, listingId)` RPC call must
 *      return > 0 — the caller must actually hold the license NFT.
 *   4. DECRYPT: only then does it call Lit.Actions.Decrypt({pkpId,ciphertext})
 *      and return { plaintext }. Any failure returns { error } instead.
 *
 * ## Expected message format (validated by `validateMessage`)
 * A newline-delimited string. Field order is not significant; each non-empty
 * line is `Key: value`. Required keys:
 *
 *     xvision sealed-bundle license request
 *     Listing: <listingId>            // decimal, must equal expectedListingId
 *     Nonce: <hex-or-alphanumeric>    // >= 8 chars, length-checked only (see SECURITY NOTE below)
 *     Expiry: <unixSeconds>           // integer; rejected once nowSec > Expiry
 *
 * Example:
 *     xvision sealed-bundle license request
 *     Listing: 42
 *     Nonce: 3f9a1c8e7b2d4056
 *     Expiry: 1760000000
 *
 * ## js_params the action expects (Lit-runtime only)
 *   { pkpId, ciphertext, address, message, signature,
 *     listingId, nftAddress, rpcUrl }
 *
 * PARAM ACCESS — Chipotle (Lit v3) AUTO-INVOKES `main(js_params)`. The action
 * just DEFINES `main`; the runtime calls it with the request's `js_params` as
 * the single argument object. There must be NO top-level self-invocation:
 * Datil (V0) spread js_params into bare globals (gone — ReferenceError), and
 * Chipotle exposes no `jsParams` global either (also ReferenceError; both
 * were observed live against the Chipotle API on 2026-06-12).
 *
 * The pure validators (`parseMessage`, `validateMessage`) are exported so they
 * can be unit-tested without the Lit runtime (see sealed-gate.test.mjs). The
 * `main` entrypoint is Lit-runtime-only (it touches `ethers`, the RPC
 * provider, and the `Lit.Actions` globals).
 *
 * ## SECURITY NOTE — nonce semantics
 * Nonce is length-checked only (the action is stateless — no consumed-nonce
 * store). Replay within the expiry window is possible but harmless: the gate
 * recovers the signer address from the signature and checks balanceOf for THAT
 * address, so a replayed message only ever re-grants decryption to a current
 * license holder. Keep the expiry window short. A consumed-nonce store is a
 * later hardening if needed.
 *
 * ## SECURITY NOTE — js_params trust (route-wiring phase)
 * nftAddress, rpcUrl, and pkpId arrive as js_params in this Phase-1 action.
 * At route-wiring time these MUST be sourced from gate-pinned constants or
 * on-chain listing data, NOT free caller params — otherwise a caller could
 * point balanceOf at a contract/RPC that returns a fake non-zero balance.
 * Pin them before live use.
 */

/** Minimum nonce length — short nonces give poor replay entropy. */
const MIN_NONCE_LEN = 8;

/**
 * Parse the SIWE-ish license message into a structured object.
 * Returns { ok: true, fields: { listingId, nonce, expiry } } or
 * { ok: false, error }. Pure — no I/O, no globals.
 *
 * @param {string} message
 */
function parseMessage(message) {
  if (typeof message !== "string" || message.trim() === "") {
    return { ok: false, error: "empty message" };
  }
  const lines = message
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0);

  const fields = {};
  for (const line of lines) {
    const idx = line.indexOf(":");
    if (idx === -1) continue; // header / freeform line — ignored
    const key = line.slice(0, idx).trim().toLowerCase();
    const value = line.slice(idx + 1).trim();
    fields[key] = value;
  }

  const listingRaw = fields["listing"];
  const nonce = fields["nonce"];
  const expiryRaw = fields["expiry"];

  if (listingRaw === undefined) return { ok: false, error: "missing Listing" };
  if (nonce === undefined) return { ok: false, error: "missing Nonce" };
  if (expiryRaw === undefined) return { ok: false, error: "missing Expiry" };

  if (!/^\d+$/.test(listingRaw)) {
    return { ok: false, error: "Listing is not a decimal integer" };
  }
  if (!/^\d+$/.test(expiryRaw)) {
    return { ok: false, error: "Expiry is not a unix timestamp" };
  }

  return {
    ok: true,
    fields: {
      listingId: listingRaw,
      nonce,
      expiry: Number(expiryRaw),
    },
  };
}

/**
 * Validate a parsed license message against the expected listing and the
 * current time. This is the replay/binding-protection core. Pure.
 *
 * @param {string} message  the raw signed message
 * @param {{ expectedListingId: string|number, nowSec: number }} opts
 * @returns {{ ok: true, fields: object } | { ok: false, error: string }}
 */
function validateMessage(message, { expectedListingId, nowSec }) {
  const parsed = parseMessage(message);
  if (!parsed.ok) return parsed;
  const { listingId, nonce, expiry } = parsed.fields;

  // Listing binding: the signed message must name the listing being unlocked.
  if (String(listingId) !== String(expectedListingId)) {
    return {
      ok: false,
      error: `listingId mismatch: signed ${listingId}, requested ${expectedListingId}`,
    };
  }

  // Nonce: length-checked only (the action is stateless — no consumed-nonce
  // store). See the SECURITY NOTE in the header for replay semantics.
  if (typeof nonce !== "string" || nonce.length < MIN_NONCE_LEN) {
    return { ok: false, error: "nonce too short / missing" };
  }

  // Expiry: reject stale signatures. `nowSec === expiry` is still valid;
  // strictly-after is expired.
  if (!Number.isFinite(expiry) || nowSec > expiry) {
    return { ok: false, error: "message expired" };
  }

  return { ok: true, fields: parsed.fields };
}

/**
 * Compare a signature-recovered address against the claimed caller address,
 * case-insensitively. Pure (the `ethers.utils.verifyMessage` recovery itself
 * is runtime-only, but the comparison logic is unit-testable). Returns
 * { ok: true } or { ok: false, error }.
 *
 * @param {string} recovered  address recovered from verifyMessage
 * @param {string} claimed    the `address` js_param the caller asserts
 */
function checkSigner(recovered, claimed) {
  if (typeof recovered !== "string" || typeof claimed !== "string") {
    return { ok: false, error: "signature does not match address" };
  }
  if (recovered.toLowerCase() !== claimed.toLowerCase()) {
    return { ok: false, error: "signature does not match address" };
  }
  return { ok: true };
}

/* eslint-disable no-undef */
/**
 * Lit Action entrypoint. RUNTIME-ONLY: depends on the `ethers` and `Lit`
 * globals provided by the Lit execution environment. The Chipotle runtime
 * auto-invokes `main(js_params)` — params arrive as the single destructured
 * argument object — and uses main's RETURN VALUE as the response envelope
 * (NOT Lit.Actions.setResponse, whose side-effect Chipotle ignores; observed
 * live 2026-06-12: setResponse-only left `response: null`). Return plain
 * objects: `{ plaintext }` on success, `{ error }` on any failure.
 * (See PARAM ACCESS header note; no self-invocation here.)
 * Not invoked by the unit tests (which exercise the pure validators above).
 */
async function main({ message, signature, address, listingId, pkpId, ciphertext, nftAddress, rpcUrl }) {
  try {
    // 1. Verify the signature recovers `address`.
    const recovered = ethers.utils.verifyMessage(message, signature);
    const sig = checkSigner(recovered, String(address));
    if (!sig.ok) {
      return { error: sig.error };
    }

    // 2. Validate message binding + freshness (expiry is the only temporal
    //    bound — see SECURITY NOTE in header for nonce/replay semantics).
    const nowSec = Math.floor(Date.now() / 1000);
    const v = validateMessage(message, { expectedListingId: listingId, nowSec });
    if (!v.ok) {
      return { error: v.error };
    }

    // 3. License check: ERC-1155 balanceOf(address, listingId) > 0.
    const provider = new ethers.providers.JsonRpcProvider(rpcUrl);
    const erc1155 = new ethers.Contract(
      nftAddress,
      ["function balanceOf(address account, uint256 id) view returns (uint256)"],
      provider,
    );
    const balance = await erc1155.balanceOf(address, listingId);
    if (balance.lte(0)) {
      return { error: "caller does not hold the license NFT" };
    }

    // 4. All gates passed — decrypt.
    const plaintext = await Lit.Actions.Decrypt({ pkpId, ciphertext });
    return { plaintext };
  } catch (e) {
    return { error: `gate error: ${e && e.message ? e.message : e}` };
  }
}

// NO top-level invocation: the Chipotle runtime auto-invokes `main(js_params)`
// itself. (Importing this file in Node for unit tests only defines `main`.)
