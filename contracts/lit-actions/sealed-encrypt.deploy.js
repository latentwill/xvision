// GENERATED — do not edit. Source: sealed-encrypt.js (run build-deploy.mjs).
// This is the plain-script form Lit's TEE runs; its IPFS CID is what you pin.
/**
 * sealed-encrypt.js — Lit Protocol v3 ("Chipotle") encrypt action.
 *
 * THE CID OF THIS FILE IS XVN_LIT_ENCRYPT_ACTION_CID. The operator pins this
 * exact file to IPFS and sets the resulting CID as `XVN_LIT_ENCRYPT_ACTION_CID`
 * (the server passes it as `ipfs_id` when calling `POST /core/v1/lit_action`).
 *
 * ## What it does
 * Runs SERVER-SIDE at publish time. It encrypts the SELLER's OWN plaintext
 * (the sealed-bundle manifest) under the PKP key, returning the ciphertext blob
 * the server stores on IPFS. The matching decrypt path is the gate action
 * (`sealed-gate.js`), which verifies a signature + ERC-1155 `balanceOf` before
 * decrypting.
 *
 * ## No gating needed (by design)
 * Unlike the gate action, this action does NOT check who is calling: it only
 * ever encrypts the caller-supplied plaintext and hands back the ciphertext.
 * Encrypting an attacker's own plaintext is harmless — they learn nothing they
 * didn't already supply, and the resulting ciphertext is still only decryptable
 * through the gate (which enforces license ownership). The encryption itself is
 * not a secret; only DECRYPTION is gated.
 *
 * ## js_params the action expects (Lit-runtime only)
 *   { pkpId, message }
 *
 * PARAM ACCESS — Naga ("Chipotle" v3) jsParams object, NOT bare globals.
 * Datil (V0) spread jsParams into global scope; Naga removed that, so params
 * arrive ONLY on the `jsParams` object. The invoke guard below destructures
 * `jsParams` and passes the values into `main({ pkpId, message })`.
 *
 * `main` is Lit-runtime-only (it touches the `Lit.Actions` globals). It is
 * guarded behind `typeof Lit` so importing this file in Node (for `node
 * --check` / the deploy-validity test, where `Lit`/jsParams are undefined)
 * does not execute the action.
 */

/* eslint-disable no-undef */
async function main({ pkpId, message }) {
  const ciphertext = await Lit.Actions.Encrypt({ pkpId, message });
  return Lit.Actions.setResponse({ response: JSON.stringify({ ciphertext }) });
}

// The Lit runtime invokes the top-level expression. Guarded so importing this
// file (in Node, where `Lit`/jsParams are undefined) does not execute it.
// Params arrive on the Naga `jsParams` object (Datil V0's bare-global spread
// was removed), so destructure them off jsParams and pass them into main.
if (typeof Lit !== "undefined" && typeof Lit.Actions !== "undefined") {
  const { pkpId, message } = jsParams;
  main({ pkpId, message });
}
