//! Single-use, time-bounded nonce store for the sealed-tier import challenge.
//!
//! Lane cgz (`xvision-cgz`): the sealed-import route requires a real
//! proof-of-address (EIP-191 `personal_sign` over a listing-bound SIWE-style
//! message). To close the replay hole that the stateless Lit gate action
//! (`contracts/lit-actions/sealed-gate.js`) deliberately leaves open, the
//! *server* owns the nonce: it issues a fresh, random, short-TTL nonce per
//! challenge ([`NonceStore::issue`]) and consumes it exactly once on a
//! successful import ([`NonceStore::consume`]). A replayed nonce (second
//! consume) or an expired nonce is rejected.
//!
//! The store is process-local and in-memory — consistent with the other
//! transient `AppState` maps (`autooptimizer_cancels`/`autooptimizer_pauses`,
//! `marketplace_snapshot`), none of which are persisted. It is `Arc<Mutex<…>>`
//! so the single-use guarantee holds across the per-request `AppState` clones
//! (the `Arc` is shared, not deep-copied).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// How long an issued nonce stays valid, in seconds. Short by design: a
/// captured challenge is only useful inside this window, and the signed
/// message carries its own `Expiry` that the verifier also enforces.
pub const NONCE_TTL_SECS: u64 = 600;

/// Number of random bytes in an issued nonce (rendered as lowercase hex, so
/// the wire nonce is `2 * NONCE_BYTES` characters — comfortably above the
/// gate's `MIN_NONCE_LEN` of 8).
const NONCE_BYTES: usize = 32;

/// One issued-but-not-yet-consumed challenge.
#[derive(Clone, Debug)]
struct NonceEntry {
    listing_id: u64,
    expiry_unix: u64,
}

/// Reason a `consume` failed. Carried so the route can map to the right HTTP
/// status / message without string-matching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonceError {
    /// No such nonce was ever issued, or it was already consumed (replay).
    UnknownOrConsumed,
    /// The nonce exists but is bound to a different listing than the one
    /// being imported.
    ListingMismatch,
    /// The nonce was issued but its TTL has elapsed.
    Expired,
}

/// A server-issued, single-use, time-bounded nonce store.
#[derive(Clone, Default)]
pub struct NonceStore {
    inner: Arc<Mutex<HashMap<String, NonceEntry>>>,
}

impl NonceStore {
    /// Fresh, empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Issue a fresh nonce bound to `listing_id`, valid for [`NONCE_TTL_SECS`]
    /// from `now_unix`. Returns `(nonce, expiry_unix)`. Prunes any expired
    /// entries first so the map cannot grow without bound.
    pub fn issue(&self, listing_id: u64, now_unix: u64) -> (String, u64) {
        let nonce = random_nonce_hex();
        let expiry_unix = now_unix.saturating_add(NONCE_TTL_SECS);
        let mut map = self.inner.lock().expect("nonce store mutex poisoned");
        map.retain(|_, e| e.expiry_unix >= now_unix);
        map.insert(
            nonce.clone(),
            NonceEntry {
                listing_id,
                expiry_unix,
            },
        );
        (nonce, expiry_unix)
    }

    /// Atomically validate-and-remove a nonce. Succeeds only if the nonce was
    /// issued, is bound to `listing_id`, and has not expired as of `now_unix`.
    /// A second `consume` of the same nonce fails with
    /// [`NonceError::UnknownOrConsumed`] (replay defense).
    pub fn consume(&self, nonce: &str, listing_id: u64, now_unix: u64) -> Result<(), NonceError> {
        let mut map = self.inner.lock().expect("nonce store mutex poisoned");
        let entry = map.get(nonce).cloned().ok_or(NonceError::UnknownOrConsumed)?;
        // Remove first so even a mismatched/expired consume burns the nonce —
        // no second chance to probe with the same value.
        map.remove(nonce);
        if entry.listing_id != listing_id {
            return Err(NonceError::ListingMismatch);
        }
        if now_unix > entry.expiry_unix {
            return Err(NonceError::Expired);
        }
        Ok(())
    }
}

/// 32 cryptographically-random bytes rendered as lowercase hex.
fn random_nonce_hex() -> String {
    let mut bytes = [0u8; NONCE_BYTES];
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    let mut s = String::with_capacity(NONCE_BYTES * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_700_000_000;

    #[test]
    fn issue_returns_long_hex_and_future_expiry() {
        let store = NonceStore::new();
        let (nonce, expiry) = store.issue(1, T0);
        assert_eq!(nonce.len(), NONCE_BYTES * 2, "32 bytes → 64 hex chars");
        assert!(nonce.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(expiry, T0 + NONCE_TTL_SECS);
    }

    #[test]
    fn issue_returns_unique_nonces() {
        let store = NonceStore::new();
        let (a, _) = store.issue(1, T0);
        let (b, _) = store.issue(1, T0);
        assert_ne!(a, b, "two issues must not collide");
    }

    #[test]
    fn nonce_single_use_then_replay_rejected() {
        let store = NonceStore::new();
        let (nonce, _) = store.issue(1, T0);
        assert_eq!(store.consume(&nonce, 1, T0 + 1), Ok(()));
        // Second consume of the same nonce → replay rejected.
        assert_eq!(
            store.consume(&nonce, 1, T0 + 1),
            Err(NonceError::UnknownOrConsumed)
        );
    }

    #[test]
    fn expired_nonce_rejected_and_pruned() {
        let store = NonceStore::new();
        let (nonce, expiry) = store.issue(1, T0);
        // Consume strictly after expiry → Expired.
        assert_eq!(store.consume(&nonce, 1, expiry + 1), Err(NonceError::Expired));
        // A fresh issue at a much later time prunes nothing of ours (already
        // consumed) and the original cannot be reused.
        assert_eq!(
            store.consume(&nonce, 1, expiry + 1),
            Err(NonceError::UnknownOrConsumed)
        );
    }

    #[test]
    fn expired_entry_pruned_on_next_issue() {
        let store = NonceStore::new();
        let (stale, stale_expiry) = store.issue(1, T0);
        // A later issue past the stale entry's expiry prunes it.
        let _ = store.issue(2, stale_expiry + 1);
        // The stale nonce is gone (pruned), so consuming it is UnknownOrConsumed,
        // not Expired.
        assert_eq!(
            store.consume(&stale, 1, stale_expiry + 1),
            Err(NonceError::UnknownOrConsumed)
        );
    }

    #[test]
    fn nonce_bound_to_listing() {
        let store = NonceStore::new();
        let (nonce, _) = store.issue(1, T0);
        // Consuming under the wrong listing id → ListingMismatch (and burns it).
        assert_eq!(store.consume(&nonce, 2, T0 + 1), Err(NonceError::ListingMismatch));
    }

    #[test]
    fn store_shares_state_across_clones() {
        // AppState is cloned per request; the Arc-backed store must share the
        // map so single-use holds across clones.
        let store = NonceStore::new();
        let clone = store.clone();
        let (nonce, _) = store.issue(7, T0);
        assert_eq!(clone.consume(&nonce, 7, T0 + 1), Ok(()));
        assert_eq!(
            store.consume(&nonce, 7, T0 + 1),
            Err(NonceError::UnknownOrConsumed)
        );
    }
}
