//! On-disk catalog cache at `$XVN_HOME/cache/models/<provider>.json`.
//!
//! One file per provider so concurrent refreshes don't race, and so a
//! corrupt single-provider response doesn't take the whole cache down.
//! Files are atomically replaced via write-to-tmp + rename so a partial
//! write can't leave readers with a half-truncated catalog.
//!
//! Cache shape mirrors `Catalog` exactly — no wrapper struct. The
//! `fetched_at` field on the catalog itself is the staleness ground
//! truth; this module just stamps it correctly on write.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Duration as ChronoDuration;
use xvision_core::providers::Catalog;

/// Default freshness window. A catalog older than this gets refreshed
/// on the next access; the stale copy is still served until the new
/// one lands so the UI doesn't blank out.
pub const DEFAULT_TTL: ChronoDuration = ChronoDuration::hours(24);

/// Resolve the cache root inside an `xvn_home`. The directory is
/// created lazily on the first write.
pub fn catalog_cache_dir(xvn_home: &Path) -> PathBuf {
    xvn_home.join("cache").join("models")
}

fn catalog_path(xvn_home: &Path, provider: &str) -> PathBuf {
    // Provider names already pass `validate_provider_name` (1..=32
    // chars, no leading underscore, restricted alphabet) before they
    // hit this layer, so `<name>.json` is always a safe filename.
    catalog_cache_dir(xvn_home).join(format!("{provider}.json"))
}

/// Read a cached catalog. `Ok(None)` when the file doesn't exist;
/// `Err` for corrupt JSON or IO errors. Callers decide whether a
/// missing-or-corrupt cache triggers a refresh or a hard fail.
pub async fn load(xvn_home: &Path, provider: &str) -> Result<Option<Catalog>> {
    let path = catalog_path(xvn_home, provider);
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let cat: Catalog = serde_json::from_slice(&bytes)
                .with_context(|| format!("parse cached catalog at {}", path.display()))?;
            Ok(Some(cat))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read cached catalog at {}", path.display())),
    }
}

/// Persist a catalog atomically. Creates `cache/models/` on first write.
///
/// Concurrent writes for *different* providers are safe (different
/// files). Concurrent writes for the *same* provider race on the rename
/// — last writer wins, no torn reads.
pub async fn save(xvn_home: &Path, catalog: &Catalog) -> Result<()> {
    let dir = catalog_cache_dir(xvn_home);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create cache dir {}", dir.display()))?;
    let final_path = catalog_path(xvn_home, &catalog.provider);
    let tmp_path = dir.join(format!(".{}.json.tmp", catalog.provider));
    let bytes = serde_json::to_vec_pretty(catalog).context("serialize catalog")?;
    tokio::fs::write(&tmp_path, &bytes)
        .await
        .with_context(|| format!("write tmp cache file {}", tmp_path.display()))?;
    tokio::fs::rename(&tmp_path, &final_path)
        .await
        .with_context(|| format!("rename tmp to {}", final_path.display()))?;
    Ok(())
}

/// True iff `now - fetched_at > ttl`. A catalog with a future
/// `fetched_at` (clock skew, manual edit) is treated as fresh.
pub fn is_stale(catalog: &Catalog, ttl: ChronoDuration, now: chrono::DateTime<chrono::Utc>) -> bool {
    let age = now.signed_duration_since(catalog.fetched_at);
    age > ttl
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;
    use xvision_core::providers::ModelEntry;

    fn sample_catalog(provider: &str) -> Catalog {
        Catalog::new(
            provider,
            "https://example.com/v1/models",
            vec![
                ModelEntry::minimal("foo"),
                ModelEntry {
                    id: "bar".into(),
                    display_name: Some("Bar".into()),
                    context_window: Some(128_000),
                    max_output_tokens: Some(8192),
                    supports_reasoning: Some(true),
                    supports_tools: Some(true),
                    pricing_per_million_input_usd: Some(3.0),
                    pricing_per_million_output_usd: Some(15.0),
                    raw: serde_json::json!({"id": "bar"}),
                },
            ],
        )
    }

    #[tokio::test]
    async fn save_then_load_roundtrips() {
        let tmp = TempDir::new().unwrap();
        let cat = sample_catalog("anthropic");
        save(tmp.path(), &cat).await.unwrap();
        let loaded = load(tmp.path(), "anthropic").await.unwrap().unwrap();
        assert_eq!(loaded.provider, "anthropic");
        assert_eq!(loaded.models.len(), 2);
        assert_eq!(loaded.models[1].max_output_tokens, Some(8192));
    }

    #[tokio::test]
    async fn load_missing_returns_none_not_err() {
        let tmp = TempDir::new().unwrap();
        let result = load(tmp.path(), "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn load_corrupt_returns_err() {
        let tmp = TempDir::new().unwrap();
        let dir = catalog_cache_dir(tmp.path());
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("bad.json"), b"not json")
            .await
            .unwrap();
        let err = load(tmp.path(), "bad").await.unwrap_err();
        assert!(err.to_string().contains("parse cached catalog"));
    }

    #[tokio::test]
    async fn save_replaces_existing_file_atomically() {
        let tmp = TempDir::new().unwrap();
        let mut a = sample_catalog("p");
        a.fetched_at = Utc::now() - chrono::Duration::hours(48);
        save(tmp.path(), &a).await.unwrap();
        let b = sample_catalog("p"); // newer fetched_at
        save(tmp.path(), &b).await.unwrap();
        let loaded = load(tmp.path(), "p").await.unwrap().unwrap();
        assert!(
            loaded.fetched_at > a.fetched_at,
            "second save should have replaced the first"
        );
    }

    #[test]
    fn is_stale_uses_fetched_at_against_now() {
        let now = Utc::now();
        let mut cat = sample_catalog("p");
        cat.fetched_at = now - chrono::Duration::hours(25);
        assert!(is_stale(&cat, DEFAULT_TTL, now));
        cat.fetched_at = now - chrono::Duration::hours(23);
        assert!(!is_stale(&cat, DEFAULT_TTL, now));
        // Clock-skew guard: future fetched_at is never stale.
        cat.fetched_at = now + chrono::Duration::hours(1);
        assert!(!is_stale(&cat, DEFAULT_TTL, now));
    }
}
