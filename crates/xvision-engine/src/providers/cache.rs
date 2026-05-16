//! On-disk catalog cache at `$XVN_HOME/cache/models/<provider>.json`.
//!
//! One file per provider so a corrupt single-provider response doesn't
//! take the whole cache down. Files are atomically replaced via
//! write-to-tmp + rename so a partial write can't leave readers with a
//! half-truncated catalog.
//!
//! Concurrency:
//!
//! - **Different providers** are independent (different files).
//! - **Same provider, concurrent writes** each get a unique temp file
//!   (`.{provider}.{pid}.{nanos}.tmp`) before the rename, so two
//!   refreshes never race on the same path. Each rename overwrites
//!   the destination atomically (POSIX `rename(2)`); last writer wins,
//!   no torn reads, no spurious `NotFound` from a sibling stealing the
//!   temp file mid-flight.
//!
//! Defensive name validation:
//!
//! - The upstream `ProviderEntry` validator (`validate_provider_name`
//!   in `xvision_core::config`) already restricts names to a safe
//!   character set, but the cache file path is now reachable from two
//!   user-driven surfaces (CLI `--name` flag and the HTTP `:name`
//!   path segment). To keep this layer safe even if a caller forgets
//!   to validate, `catalog_path` re-checks the name and returns an
//!   error for empty, oversize, or filename-unsafe inputs.
//!
//! Cache shape mirrors `Catalog` exactly — no wrapper struct. The
//! `fetched_at` field on the catalog itself is the staleness ground
//! truth; this module just stamps it correctly on write.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
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

/// Defensive filename safety check on the provider name. Mirrors the
/// upstream `xvision_core::config::validate_provider_name` constraint
/// (1..=32 chars, restricted alphabet) so even if a caller forgets to
/// validate, this layer cannot produce a path that escapes the cache
/// directory or aliases another file.
///
/// Allowed: ASCII letters, digits, `-`, `_`. The upstream validator
/// also forbids a leading underscore (reserved namespace) — we keep
/// that here, but otherwise this is intentionally a strict subset of
/// what the config layer accepts so `cache.rs` is the last line of
/// defense, not the first.
fn validate_for_path(provider: &str) -> Result<()> {
    if provider.is_empty() {
        bail!("provider name is empty");
    }
    if provider.len() > 32 {
        bail!("provider name `{provider}` is longer than 32 chars");
    }
    if provider.starts_with('_') {
        bail!("provider name `{provider}` starts with reserved `_`");
    }
    for c in provider.chars() {
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            bail!(
                "provider name `{provider}` contains disallowed character `{c}` \
                 (only ASCII letters, digits, `-`, `_` allowed)"
            );
        }
    }
    Ok(())
}

pub fn catalog_path(xvn_home: &Path, provider: &str) -> Result<PathBuf> {
    validate_for_path(provider)?;
    Ok(catalog_cache_dir(xvn_home).join(format!("{provider}.json")))
}

/// Construct a unique temp filename for an in-progress write. Includes
/// the current pid and a nanosecond timestamp so two refreshes for the
/// same provider — possible when the dashboard and CLI race, or two
/// HTTP clients hammer `refresh-all` — each write to their own temp
/// file and rename independently. Without this, one writer can `rename`
/// the temp away before the other tries, causing a spurious `NotFound`
/// from the OS.
fn unique_tmp_path(dir: &Path, provider: &str) -> PathBuf {
    use std::time::SystemTime;
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.join(format!(".{provider}.{pid}.{nanos}.tmp"))
}

/// Read a cached catalog. `Ok(None)` when the file doesn't exist;
/// `Err` for corrupt JSON, IO errors, or unsafe provider names.
pub async fn load(xvn_home: &Path, provider: &str) -> Result<Option<Catalog>> {
    let path = catalog_path(xvn_home, provider)?;
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
/// Concurrency:
/// - Different providers → different files; fully independent.
/// - Same provider, two writers → each gets a unique temp file (see
///   `unique_tmp_path`) and renames it into place. Last `rename(2)`
///   wins; no torn reads, no spurious `NotFound` from a sibling
///   removing the temp file mid-flight.
pub async fn save(xvn_home: &Path, catalog: &Catalog) -> Result<()> {
    let final_path = catalog_path(xvn_home, &catalog.provider)?;
    let dir = catalog_cache_dir(xvn_home);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create cache dir {}", dir.display()))?;
    let tmp_path = unique_tmp_path(&dir, &catalog.provider);
    let bytes = serde_json::to_vec_pretty(catalog).context("serialize catalog")?;
    tokio::fs::write(&tmp_path, &bytes)
        .await
        .with_context(|| format!("write tmp cache file {}", tmp_path.display()))?;
    // On rename failure (rare — e.g., destination dir replaced under us
    // between create_dir_all and rename), best-effort remove the temp
    // file so we don't leave litter in cache/models/.
    if let Err(e) = tokio::fs::rename(&tmp_path, &final_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(e).with_context(|| format!("rename tmp to {}", final_path.display()));
    }
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

    #[tokio::test]
    async fn concurrent_same_provider_saves_do_not_race_on_temp_file() {
        // Regression for the review on PR #198: the previous impl used
        // a single fixed temp filename per provider, so two concurrent
        // refreshes for the same provider could collide — one rename
        // would succeed and the other would `ENOENT` because the
        // sibling had already moved the temp file into place.
        //
        // We now use a unique temp filename per write. Race a batch of
        // concurrent saves and assert every one succeeds, and the
        // final on-disk catalog parses cleanly.
        let tmp = TempDir::new().unwrap();
        let mut tasks = Vec::new();
        for i in 0..16u32 {
            let dir = tmp.path().to_path_buf();
            tasks.push(tokio::spawn(async move {
                let mut cat = sample_catalog("openrouter");
                // Tag each write with a distinguishable model so we can
                // verify a single coherent winner — no partial blends.
                cat.models[0].id = format!("writer-{i}");
                save(&dir, &cat).await
            }));
        }
        for t in tasks {
            t.await.unwrap().expect("save must not error under concurrency");
        }
        let loaded = load(tmp.path(), "openrouter").await.unwrap().unwrap();
        // The first model id must be one of the writers — meaning the
        // file is a coherent single-writer copy, not a concatenation
        // of two writers' bytes.
        let id = &loaded.models[0].id;
        assert!(id.starts_with("writer-"), "got blended id: {id}");
        // No `.openrouter.json.tmp`-style stragglers should be left
        // behind under cache/models/.
        let dir = catalog_cache_dir(tmp.path());
        let mut entries = tokio::fs::read_dir(&dir).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                name == "openrouter.json",
                "unexpected leftover after concurrent writes: {name}"
            );
        }
    }

    #[test]
    fn catalog_path_rejects_unsafe_provider_names() {
        let tmp = TempDir::new().unwrap();
        // The upstream validator already rejects these, but the cache
        // layer is reachable from two user-driven surfaces (CLI flag,
        // HTTP path segment) so the safety check stays here too.
        for bad in [
            "",
            "../escape",
            "with/slash",
            "with\\backslash",
            "_reserved",
            "way-too-long-name-exceeds-thirty-two-characters-limit",
            "has space",
            "has.dot",
            "unicode-ñ",
        ] {
            assert!(
                catalog_path(tmp.path(), bad).is_err(),
                "expected `{bad}` to be rejected, but path was built"
            );
        }
        // Sanity: a normal name still works.
        assert!(catalog_path(tmp.path(), "openrouter").is_ok());
        assert!(catalog_path(tmp.path(), "openai-compat-1").is_ok());
    }

    #[tokio::test]
    async fn load_rejects_unsafe_names_without_touching_disk() {
        let tmp = TempDir::new().unwrap();
        let err = load(tmp.path(), "../etc/passwd").await.unwrap_err();
        assert!(
            err.to_string().contains("disallowed character"),
            "got: {err}"
        );
    }
}
