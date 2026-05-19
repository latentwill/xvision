//! `CatalogService` — process-wide owner of model catalogs.
//!
//! Three responsibilities:
//!
//! - **In-memory map** keyed by `ProviderEntry.name`, served read-only
//!   to callers. Cheap to clone since `Catalog` is a thin struct of
//!   string ids + numbers; readers can hold the resulting `Catalog`
//!   without blocking writers.
//!
//! - **Disk cache** at `$XVN_HOME/cache/models/<provider>.json`. The
//!   service reads from disk on first access per provider and writes
//!   back after a successful refresh.
//!
//! - **Refresh orchestration**. Dispatches to the right fetcher
//!   (Anthropic / OpenRouter / OpenAI-compat / etc.), handles auth-
//!   header construction, and stores both in-memory and on-disk.
//!
//! Concurrency model: a single `RwLock<HashMap>` for the in-memory map.
//! Refresh is sequential per provider (held write lock during fetch
//! would block readers for ~10s of seconds — we don't do that); instead
//! the lock is dropped during the network call, and only retaken to
//! commit the result. Two callers racing a refresh on the same
//! provider will both fire requests; the last write wins. That's
//! fine — catalogs are idempotent and the rare extra fetch is cheaper
//! than a serialization point on a slow path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use xvision_core::config::ProviderEntry;
use xvision_core::providers::Catalog;

use super::cache;
use super::fetcher;

pub struct CatalogService {
    xvn_home: PathBuf,
    http: reqwest::Client,
    inner: RwLock<HashMap<String, Arc<Catalog>>>,
}

impl CatalogService {
    /// Construct from an `xvn_home`. Lazy — the disk cache isn't read
    /// until the first `get_or_load` / `refresh` call for a provider.
    pub fn new(xvn_home: PathBuf) -> Result<Self> {
        Ok(Self {
            xvn_home,
            http: fetcher::build_http_client()?,
            inner: RwLock::new(HashMap::new()),
        })
    }

    /// Construct with a pre-built HTTP client (for tests that want to
    /// inject a `localhost` client without TLS overhead).
    pub fn with_client(xvn_home: PathBuf, http: reqwest::Client) -> Self {
        Self {
            xvn_home,
            http,
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Try the in-memory map first, then the disk cache, then return
    /// `None`. Does **not** trigger a network fetch — callers who want
    /// to ensure freshness should call `refresh`.
    pub async fn get_or_load(&self, provider: &str) -> Result<Option<Arc<Catalog>>> {
        {
            let guard = self.inner.read().await;
            if let Some(c) = guard.get(provider) {
                return Ok(Some(Arc::clone(c)));
            }
        }
        match cache::load(&self.xvn_home, provider).await? {
            Some(cat) => {
                let arc = Arc::new(cat);
                self.inner
                    .write()
                    .await
                    .insert(provider.to_string(), Arc::clone(&arc));
                Ok(Some(arc))
            }
            None => Ok(None),
        }
    }

    /// Force a fresh fetch + cache write. Returns the new catalog.
    ///
    /// Auth keys are read from the env at call time, not stored — so
    /// rotating an API key takes effect on the next refresh without
    /// a service restart.
    pub async fn refresh(&self, provider: &ProviderEntry) -> Result<Arc<Catalog>> {
        let api_key = fetcher::resolve_api_key(provider)?;
        let f = fetcher::fetcher_for(provider, api_key)?;
        let catalog = f.fetch(&self.http).await?;
        let arc = Arc::new(catalog);
        // Disk first — if the write fails, fail the whole refresh
        // rather than leave the in-memory and on-disk views diverging.
        cache::save(&self.xvn_home, &arc).await?;
        self.inner
            .write()
            .await
            .insert(provider.name.clone(), Arc::clone(&arc));
        Ok(arc)
    }

    /// Refresh every registered provider in parallel. Returns a vec of
    /// (provider_name, result) pairs so partial failures (one provider
    /// unreachable) don't fail the whole batch — the CLI / API can
    /// surface which ones succeeded.
    pub async fn refresh_all(&self, providers: &[ProviderEntry]) -> Vec<(String, Result<Arc<Catalog>>)> {
        let mut joins = Vec::with_capacity(providers.len());
        for p in providers {
            // local-candle has no remote catalog; surface as a soft
            // error in the result list rather than skipping silently,
            // so the CLI output is auditable.
            let p = p.clone();
            joins.push(async move {
                let name = p.name.clone();
                let result = self.refresh(&p).await;
                (name, result)
            });
        }
        // Sequential — we deliberately don't fan out concurrent
        // refreshes because shared HTTP clients across mismatched TLS
        // backends have surprised us before, and the typical N=2-5
        // providers makes the wall-clock cost trivial. Revisit if N
        // grows past ~10.
        let mut out = Vec::with_capacity(joins.len());
        for j in joins {
            out.push(j.await);
        }
        out
    }

    /// Drop a single provider's cache entry (in-memory only). Useful
    /// when the provider is deleted from config — disk file is left
    /// behind until the next refresh-all so a re-add can warm-start.
    pub async fn forget(&self, provider: &str) {
        self.inner.write().await.remove(provider);
    }

    /// Snapshot of provider names currently in the in-memory map.
    /// Useful for diagnostics and the health probe.
    pub async fn providers_in_memory(&self) -> Vec<String> {
        let guard = self.inner.read().await;
        let mut out: Vec<String> = guard.keys().cloned().collect();
        out.sort();
        out
    }

    pub fn xvn_home(&self) -> &Path {
        &self.xvn_home
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;
    use xvision_core::config::ProviderKind;
    use xvision_core::providers::{Catalog, ModelEntry};

    async fn put_disk_cache(xvn_home: &Path, provider: &str, models: Vec<ModelEntry>) {
        let cat = Catalog {
            provider: provider.to_string(),
            fetched_at: Utc::now(),
            source_url: "test".into(),
            models,
        };
        cache::save(xvn_home, &cat).await.unwrap();
    }

    #[tokio::test]
    async fn get_or_load_returns_none_when_no_cache() {
        let tmp = TempDir::new().unwrap();
        let svc = CatalogService::new(tmp.path().to_path_buf()).unwrap();
        assert!(svc.get_or_load("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_or_load_promotes_disk_to_memory_on_first_access() {
        let tmp = TempDir::new().unwrap();
        put_disk_cache(
            tmp.path(),
            "anthropic",
            vec![ModelEntry::minimal("claude-opus-4-7")],
        )
        .await;
        let svc = CatalogService::new(tmp.path().to_path_buf()).unwrap();
        // Cold cache: in-memory map should be empty.
        assert!(svc.providers_in_memory().await.is_empty());
        let loaded = svc.get_or_load("anthropic").await.unwrap().unwrap();
        assert_eq!(loaded.models.len(), 1);
        // After first access, in-memory should hold it for subsequent
        // reads — otherwise we'd hit disk on every settings page render.
        assert_eq!(svc.providers_in_memory().await, vec!["anthropic"]);
    }

    #[tokio::test]
    async fn forget_removes_from_memory_but_not_disk() {
        let tmp = TempDir::new().unwrap();
        put_disk_cache(tmp.path(), "p", vec![ModelEntry::minimal("m")]).await;
        let svc = CatalogService::new(tmp.path().to_path_buf()).unwrap();
        svc.get_or_load("p").await.unwrap();
        assert!(!svc.providers_in_memory().await.is_empty());
        svc.forget("p").await;
        assert!(svc.providers_in_memory().await.is_empty());
        // Disk still has it — verify by reloading.
        assert!(svc.get_or_load("p").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn refresh_local_candle_returns_err() {
        let tmp = TempDir::new().unwrap();
        let svc = CatalogService::new(tmp.path().to_path_buf()).unwrap();
        let p = ProviderEntry {
            name: "candle".into(),
            kind: ProviderKind::LocalCandle,
            base_url: String::new(),
            api_key_env: String::new(),
            enabled_models: Vec::new(),
        };
        let err = svc.refresh(&p).await.unwrap_err();
        assert!(err.to_string().contains("local-candle"));
    }
}
