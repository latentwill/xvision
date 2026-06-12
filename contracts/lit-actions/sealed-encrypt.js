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
 * PARAM ACCESS — Chipotle (Lit v3) AUTO-INVOKES `main(js_params)`. The action
 * just DEFINES `main`; the runtime calls it with the request's `js_params` as
 * the single argument object. There must be NO top-level self-invocation:
 * Datil (V0) spread js_params into bare globals (gone — ReferenceError), and
 * Chipotle exposes no `jsParams` global either (also ReferenceError; both
 * were observed live against the Chipotle API on 2026-06-12).
 *
 * With no top-level call, importing this file in Node (for `node --check` /
 * the deploy-validity test, where `Lit` is undefined) only defines `main`
 * and never executes the action.
 */

/* eslint-disable no-undef */
// Chipotle uses main's RETURN VALUE as the response envelope (NOT
// Lit.Actions.setResponse, whose side-effect it ignores; observed live
// 2026-06-12: setResponse-only left `response: null`).
async function main({ pkpId, message }) {
  const ciphertext = await Lit.Actions.Encrypt({ pkpId, message });
  return { ciphertext };
}
