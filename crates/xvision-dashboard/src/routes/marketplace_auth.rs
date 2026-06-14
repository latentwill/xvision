//! Sealed-tier import proof-of-address verification (lane cgz, `xvision-cgz`).
//!
//! Replaces the v1 *address-assertion* caveat on the sealed-import route with a
//! real EIP-191 (`personal_sign`) proof: the caller signs a listing-bound,
//! time-bounded, nonce-carrying SIWE-style message; the server recovers the
//! signer with `alloy` and requires `signer == claimed address` before granting
//! import/decrypt. The signed-message grammar is byte-compatible with the Lit
//! gate action (`contracts/lit-actions/sealed-gate.js`'s `parseMessage` /
//! `validateMessage`), so the SAME message validates in both the in-TEE gate and
//! here on the server — the server is the actual single-use replay defense (the
//! gate is deliberately stateless; see its SECURITY NOTE).
//!
//! Why EIP-191 and not full RFC-4361 SIWE: the design doc
//! (`docs/superpowers/specs/2026-06-11-sealed-tier-lit-design.md` §3) and the
//! existing client (`frontend/web/src/features/marketplace/lib/sealed.ts`)
//! produce a "SIWE-*style*" `personal_sign` payload, not a strict 4361 message.
//! `alloy`'s `recover_address_from_msg` applies the
//! `"\x19Ethereum Signed Message:\n"` prefix internally, matching
//! viem/wagmi `walletClient.signMessage` and the gate's `ethers.verifyMessage`.

use alloy::primitives::Address;
use alloy::primitives::Signature;

use crate::error::DashboardError;

/// Minimum nonce length — short nonces give poor replay entropy. Mirrors
/// `MIN_NONCE_LEN` in `contracts/lit-actions/sealed-gate.js`.
pub const MIN_NONCE_LEN: usize = 8;

/// The header line every license message starts with. Field order after it is
/// not significant; the parser ignores any line without a `:`.
const MESSAGE_HEADER: &str = "xvision sealed-bundle license request";

/// The validated, listing-bound fields recovered from a signed challenge. The
/// `nonce` is returned so the caller can consume it single-use from the
/// `NonceStore`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedChallenge {
    pub listing_id: u64,
    pub nonce: String,
    pub expiry_unix: u64,
}

/// Verify an EIP-191 proof-of-address over a listing-bound challenge message.
///
/// Steps (in order, fail-closed):
///   1. Parse `signature_hex` into an `alloy` `Signature` (400 on malformed).
///   2. Recover the signer with `recover_address_from_msg` (the EIP-191 prefix
///      is applied internally — do NOT prefix the message yourself).
///   3. Require `recovered == address` (403 on mismatch — the caller is
///      identified but the proof does not entitle them, same semantics as the
///      license gate's "identified but not entitled").
///   4. Parse + validate the message fields against `expected_listing_id` and
///      `now_unix` (401 on a binding/freshness failure: listing mismatch,
///      short nonce, expired). The nonce single-use check is the caller's job
///      (via the `NonceStore`); this function only validates message shape.
///
/// `now_unix` is injected so tests can pin/advance the clock.
pub fn verify_address_proof(
    address: Address,
    message: &str,
    signature_hex: &str,
    expected_listing_id: u64,
    now_unix: u64,
) -> Result<ParsedChallenge, DashboardError> {
    // 1. Parse the signature. Accept an optional 0x prefix; require valid hex
    //    that decodes to a 65-byte signature.
    let sig: Signature = signature_hex.parse().map_err(|_| DashboardError::Validation {
        field: "signature".into(),
        msg: "must be a 0x-prefixed 65-byte (130-hex-char) personal_sign signature".into(),
    })?;

    // 2. Recover the signer. `recover_address_from_msg` hashes the EIP-191
    //    "\x19Ethereum Signed Message:\n<len><msg>" envelope internally.
    let recovered =
        sig.recover_address_from_msg(message.as_bytes())
            .map_err(|e| DashboardError::Validation {
                field: "signature".into(),
                msg: format!("could not recover signer from signature: {e}"),
            })?;

    // 3. The recovered signer must equal the claimed address. 403: caller is
    //    identified (we have an address) but does not prove control of the one
    //    they claim.
    if recovered != address {
        return Err(DashboardError::Forbidden(format!(
            "signature does not prove control of {address:#x}: recovered {recovered:#x}"
        )));
    }

    // 4. Parse + validate the message binding/freshness. 401 on any failure —
    //    a stale or wrong-listing proof is an auth failure, not a 400 input bug.
    let parsed = parse_challenge_message(message)
        .map_err(|e| DashboardError::Unauthorized(format!("invalid license message: {e}")))?;
    if parsed.listing_id != expected_listing_id {
        return Err(DashboardError::Unauthorized(format!(
            "listingId mismatch: signed {}, requested {expected_listing_id}",
            parsed.listing_id
        )));
    }
    if parsed.nonce.len() < MIN_NONCE_LEN {
        return Err(DashboardError::Unauthorized("nonce too short / missing".into()));
    }
    if now_unix > parsed.expiry_unix {
        return Err(DashboardError::Unauthorized("message expired".into()));
    }

    Ok(parsed)
}

/// Parse the SIWE-ish license message into `{ listing_id, nonce, expiry_unix }`.
/// Pure; ports `parseMessage` from `contracts/lit-actions/sealed-gate.js`:
/// newline-delimited, each non-empty line is `Key: value` (case-insensitive
/// key), lines without a `:` are ignored (the header line). Required keys:
/// `Listing` (decimal), `Nonce`, `Expiry` (decimal unix). Returns a short error
/// string on any shape failure.
fn parse_challenge_message(message: &str) -> Result<ParsedChallenge, String> {
    if message.trim().is_empty() {
        return Err("empty message".into());
    }

    let mut listing_raw: Option<String> = None;
    let mut nonce: Option<String> = None;
    let mut expiry_raw: Option<String> = None;

    for line in message.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(idx) = line.find(':') else {
            continue; // header / freeform line — ignored
        };
        let key = line[..idx].trim().to_ascii_lowercase();
        let value = line[idx + 1..].trim().to_string();
        match key.as_str() {
            "listing" => listing_raw = Some(value),
            "nonce" => nonce = Some(value),
            "expiry" => expiry_raw = Some(value),
            _ => {}
        }
    }

    let listing_raw = listing_raw.ok_or("missing Listing")?;
    let nonce = nonce.ok_or("missing Nonce")?;
    let expiry_raw = expiry_raw.ok_or("missing Expiry")?;

    let listing_id: u64 = listing_raw
        .parse()
        .map_err(|_| "Listing is not a decimal integer".to_string())?;
    let expiry_unix: u64 = expiry_raw
        .parse()
        .map_err(|_| "Expiry is not a unix timestamp".to_string())?;

    Ok(ParsedChallenge {
        listing_id,
        nonce,
        expiry_unix,
    })
}

/// Build the exact challenge message a client must `personal_sign`. Byte-for-byte
/// identical to `buildSealedMessage` in
/// `frontend/web/src/features/marketplace/lib/sealed.ts` so the same signature
/// validates here and in the Lit gate action. Returned by the import-challenge
/// route so the client signs the server's canonical string.
pub fn build_challenge_message(listing_id: u64, nonce: &str, expiry_unix: u64) -> String {
    format!("{MESSAGE_HEADER}\nListing: {listing_id}\nNonce: {nonce}\nExpiry: {expiry_unix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::signers::local::PrivateKeySigner;
    use alloy::signers::SignerSync;

    const NONCE: &str = "3f9a1c8e7b2d40563f9a1c8e7b2d4056";
    const FUTURE: u64 = 9_999_999_999;
    const NOW: u64 = 1_700_000_000;

    /// A deterministic dev signer (well-known Anvil key) and its address.
    fn signer() -> PrivateKeySigner {
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .unwrap()
    }

    /// Sign `message` with `s` and return the 0x-prefixed 65-byte hex
    /// signature, mirroring what viem/wagmi `signMessage` returns.
    fn sign(s: &PrivateKeySigner, message: &str) -> String {
        let sig = s.sign_message_sync(message.as_bytes()).unwrap();
        format!("0x{}", alloy::hex::encode(sig.as_bytes()))
    }

    #[test]
    fn build_challenge_message_is_byte_compatible_with_gate() {
        // Must match the JS buildSealedMessage / sealed-gate.js grammar exactly.
        let msg = build_challenge_message(42, "abc", 1_760_000_000);
        assert_eq!(
            msg,
            "xvision sealed-bundle license request\nListing: 42\nNonce: abc\nExpiry: 1760000000"
        );
    }

    #[test]
    fn verify_address_proof_accepts_valid_signature() {
        let s = signer();
        let msg = build_challenge_message(1, NONCE, FUTURE);
        let sig = sign(&s, &msg);
        let parsed = verify_address_proof(s.address(), &msg, &sig, 1, NOW)
            .expect("valid proof from claimed address should be accepted");
        assert_eq!(parsed.listing_id, 1);
        assert_eq!(parsed.nonce, NONCE);
        assert_eq!(parsed.expiry_unix, FUTURE);
    }

    #[test]
    fn verify_address_proof_rejects_wrong_signer() {
        // Signer A signs, but the body claims a DIFFERENT address.
        let a = signer();
        let b: PrivateKeySigner = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
            .parse()
            .unwrap();
        let msg = build_challenge_message(1, NONCE, FUTURE);
        let sig = sign(&a, &msg);
        let err = verify_address_proof(b.address(), &msg, &sig, 1, NOW).unwrap_err();
        match err {
            DashboardError::Forbidden(m) => {
                assert!(m.contains("does not prove control"), "{m}");
            }
            other => panic!("expected Forbidden (403) on signer mismatch, got {other:?}"),
        }
    }

    #[test]
    fn verify_address_proof_rejects_malformed_signature() {
        let s = signer();
        let msg = build_challenge_message(1, NONCE, FUTURE);
        let err = verify_address_proof(s.address(), &msg, "0xnothex", 1, NOW).unwrap_err();
        match err {
            DashboardError::Validation { field, .. } => assert_eq!(field, "signature"),
            other => panic!("expected Validation (400) on malformed signature, got {other:?}"),
        }
    }

    #[test]
    fn verify_address_proof_rejects_listing_mismatch() {
        // Message names Listing: 2 but the import is for listing 1.
        let s = signer();
        let msg = build_challenge_message(2, NONCE, FUTURE);
        let sig = sign(&s, &msg);
        let err = verify_address_proof(s.address(), &msg, &sig, 1, NOW).unwrap_err();
        match err {
            DashboardError::Unauthorized(m) => assert!(m.contains("listingId mismatch"), "{m}"),
            other => panic!("expected Unauthorized (401) on listing mismatch, got {other:?}"),
        }
    }

    #[test]
    fn verify_address_proof_rejects_expired_message() {
        let s = signer();
        let expiry = NOW; // now == expiry is valid; now+1 > expiry is expired.
        let msg = build_challenge_message(1, NONCE, expiry);
        let sig = sign(&s, &msg);
        // Boundary: now == expiry is still valid.
        assert!(verify_address_proof(s.address(), &msg, &sig, 1, expiry).is_ok());
        // Strictly-after expiry → 401 expired.
        let err = verify_address_proof(s.address(), &msg, &sig, 1, expiry + 1).unwrap_err();
        match err {
            DashboardError::Unauthorized(m) => assert!(m.contains("expired"), "{m}"),
            other => panic!("expected Unauthorized (401) on expiry, got {other:?}"),
        }
    }

    #[test]
    fn verify_address_proof_rejects_short_nonce() {
        let s = signer();
        let msg = build_challenge_message(1, "short", FUTURE); // 5 < MIN_NONCE_LEN
        let sig = sign(&s, &msg);
        let err = verify_address_proof(s.address(), &msg, &sig, 1, NOW).unwrap_err();
        match err {
            DashboardError::Unauthorized(m) => assert!(m.contains("nonce too short"), "{m}"),
            other => panic!("expected Unauthorized (401) on short nonce, got {other:?}"),
        }
    }

    #[test]
    fn verify_address_proof_rejects_missing_fields() {
        // A message that signs cleanly but is missing the Expiry line.
        let s = signer();
        let msg = format!("{MESSAGE_HEADER}\nListing: 1\nNonce: {NONCE}");
        let sig = sign(&s, &msg);
        let err = verify_address_proof(s.address(), &msg, &sig, 1, NOW).unwrap_err();
        match err {
            DashboardError::Unauthorized(m) => assert!(m.contains("missing Expiry"), "{m}"),
            other => panic!("expected Unauthorized (401) on missing field, got {other:?}"),
        }
    }

    #[test]
    fn parse_challenge_message_ignores_header_and_is_case_insensitive() {
        // Keys are case-insensitive; the header line (no colon) is ignored.
        let msg =
            "XVISION SEALED-BUNDLE LICENSE REQUEST\nlisting: 7\nNONCE: 0123456789abcdef\nExpiry: 1760000000";
        let parsed = parse_challenge_message(msg).unwrap();
        assert_eq!(parsed.listing_id, 7);
        assert_eq!(parsed.nonce, "0123456789abcdef");
        assert_eq!(parsed.expiry_unix, 1_760_000_000);
    }
}
