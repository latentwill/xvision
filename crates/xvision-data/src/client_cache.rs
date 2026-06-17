//! Process-global Nansen/Elfa client cache (xvision-im2r.8).
//!
//! `build_tool_registry` creates a fresh client on every eval run, so the
//! in-process rate limiter (governor `Quota`) resets each time and the
//! account-level per-minute quota cannot be tracked across rapid optimizer
//! backtest cycles.
//!
//! This module memoizes client construction keyed by the identity tuple
//! `(kind, base_url, api_key_value, rpm)`. Identical config reuses the same
//! `Arc<NansenClient>` / `Arc<ElfaClient>` (and thus the same in-memory rate
//! limiter) across runs. A config change (different URL, rotated key, or new
//! RPM) yields a fresh client.
//!
//! Thread safety: the map is behind a `std::sync::Mutex`; contention on this
//! path (at most a few times per run start) is negligible.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::elfa::ElfaClient;
use crate::nansen::NansenClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ClientKind {
    Nansen,
    Elfa,
}

/// Cache key: kind + config identity.
type CacheKey = (ClientKind, String, String, u32);

struct ClientStore {
    nansen: HashMap<CacheKey, Arc<NansenClient>>,
    elfa: HashMap<CacheKey, Arc<ElfaClient>>,
}

static CLIENTS: LazyLock<Mutex<ClientStore>> = LazyLock::new(|| {
    Mutex::new(ClientStore {
        nansen: HashMap::new(),
        elfa: HashMap::new(),
    })
});

/// Return a shared `Arc<NansenClient>` for the given config, constructing and
/// caching one on first use.
pub fn get_or_create_nansen(base_url: &str, api_key: &str, rpm: u32) -> Arc<NansenClient> {
    let key = (ClientKind::Nansen, base_url.to_string(), api_key.to_string(), rpm);
    let mut store = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = store.nansen.get(&key) {
        return existing.clone();
    }
    let client = Arc::new(NansenClient::new(base_url.to_string(), api_key.to_string(), rpm));
    store.nansen.insert(key, client.clone());
    client
}

/// Return a shared `Arc<ElfaClient>` for the given config, constructing and
/// caching one on first use.
pub fn get_or_create_elfa(base_url: &str, api_key: &str, rpm: u32) -> Arc<ElfaClient> {
    let key = (ClientKind::Elfa, base_url.to_string(), api_key.to_string(), rpm);
    let mut store = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = store.elfa.get(&key) {
        return existing.clone();
    }
    let client = Arc::new(ElfaClient::new(base_url.to_string(), api_key.to_string(), rpm));
    store.elfa.insert(key, client.clone());
    client
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_nansen_config_returns_same_arc() {
        let a = get_or_create_nansen("http://nansen.example", "key1", 300);
        let b = get_or_create_nansen("http://nansen.example", "key1", 300);
        assert!(
            Arc::ptr_eq(&a, &b),
            "identical config must return the same Arc (shared rate limiter)"
        );
    }

    #[test]
    fn different_nansen_base_url_returns_different_arc() {
        let a = get_or_create_nansen("http://nansen-a.example", "key1", 300);
        let b = get_or_create_nansen("http://nansen-b.example", "key1", 300);
        assert!(
            !Arc::ptr_eq(&a, &b),
            "different base_url must return distinct clients"
        );
    }

    #[test]
    fn same_elfa_config_returns_same_arc() {
        let a = get_or_create_elfa("http://elfa.example", "ekey1", 60);
        let b = get_or_create_elfa("http://elfa.example", "ekey1", 60);
        assert!(
            Arc::ptr_eq(&a, &b),
            "identical elfa config must return the same Arc"
        );
    }

    #[test]
    fn different_elfa_api_key_returns_different_arc() {
        let a = get_or_create_elfa("http://elfa.example", "ekey-x", 60);
        let b = get_or_create_elfa("http://elfa.example", "ekey-y", 60);
        assert!(
            !Arc::ptr_eq(&a, &b),
            "different api_key must return distinct clients"
        );
    }
}
