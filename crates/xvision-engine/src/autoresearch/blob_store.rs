//! Filesystem-backed content-addressed blob store.
//!
//! Layout under `root`:
//!   <root>/<hh>/<hh>/<remaining-60-hex>.json   — for JSON blobs
//!   <root>/<hh>/<hh>/<remaining-60-hex>.bin    — for raw byte blobs
//!
//! Two-level fan-out keeps any single directory < a few thousand entries even
//! with millions of blobs. The default root is `~/.xvn/lineage/blobs`; tests
//! pass an explicit tempdir.

use std::path::{Path, PathBuf};

use crate::autoresearch::content_hash::ContentHash;

pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    /// Construct a store rooted at `root` without any I/O.
    ///
    /// Directories are created lazily on the first `put_*` call, so callers
    /// that only need `exists` or path resolution can use this.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Return the default root path (`~/.xvn/lineage/blobs`) without opening it.
    pub fn default_root() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("could not resolve home dir"))?;
        Ok(home.join(".xvn/lineage/blobs"))
    }

    /// Open a store at `root`, creating the directory if necessary.
    pub async fn open(root: PathBuf) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    /// Open the default store at `~/.xvn/lineage/blobs`.
    pub async fn open_default() -> anyhow::Result<Self> {
        Self::open(Self::default_root()?).await
    }

    pub async fn put_json(&self, value: &serde_json::Value) -> anyhow::Result<ContentHash> {
        let hash = ContentHash::of_json(value);
        let path = self.path_for(&hash, "json");
        if tokio::fs::try_exists(&path).await? {
            return Ok(hash);
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let canonical = crate::autoresearch::content_hash::canonicalize_json(value);
        let bytes = serde_json::to_vec_pretty(&canonical)?;
        atomic_write(&path, &bytes).await?;
        Ok(hash)
    }

    pub async fn get_json(&self, hash: &ContentHash) -> anyhow::Result<serde_json::Value> {
        let path = self.path_for(hash, "json");
        if !tokio::fs::try_exists(&path).await? {
            anyhow::bail!("blob not found: {hash}");
        }
        let bytes = tokio::fs::read(&path).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn put_bytes(&self, payload: &[u8]) -> anyhow::Result<ContentHash> {
        let hash = ContentHash::of_bytes(payload);
        let path = self.path_for(&hash, "bin");
        if tokio::fs::try_exists(&path).await? {
            return Ok(hash);
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        atomic_write(&path, payload).await?;
        Ok(hash)
    }

    pub async fn get_bytes(&self, hash: &ContentHash) -> anyhow::Result<Vec<u8>> {
        let path = self.path_for(hash, "bin");
        if !tokio::fs::try_exists(&path).await? {
            anyhow::bail!("blob not found: {hash}");
        }
        Ok(tokio::fs::read(&path).await?)
    }

    /// Check whether a blob with this hash exists in the store (sync).
    ///
    /// Checks both `.bin` and `.json` extensions because bytes and JSON blobs
    /// are stored under the same fan-out tree with different file extensions.
    pub fn exists(&self, hash: &ContentHash) -> bool {
        self.path_for(hash, "bin").exists() || self.path_for(hash, "json").exists()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, hash: &ContentHash, ext: &str) -> PathBuf {
        let hex = hash.to_hex();
        let (h1, rest) = hex.split_at(2);
        let (h2, tail) = rest.split_at(2);
        self.root.join(h1).join(h2).join(format!("{tail}.{ext}"))
    }
}

async fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}
