use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::autoresearch::content_hash::ContentHash;

/// Filesystem-backed content-addressed store.
///
/// Each blob is written as `<dir>/<hash-hex>.json`. Writes are
/// idempotent: if the file already exists the hash is returned
/// without re-writing.
pub struct BlobStore {
    dir: PathBuf,
}

impl BlobStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Write `value` (as pretty-printed JSON) and return its content hash.
    pub fn put_json(&self, value: &serde_json::Value) -> Result<ContentHash> {
        let hash = ContentHash::of_json(value);
        let path = self.blob_path(&hash);
        if !path.exists() {
            std::fs::create_dir_all(&self.dir)
                .with_context(|| format!("create blob dir {}", self.dir.display()))?;
            let bytes = serde_json::to_vec_pretty(value).context("serialize blob")?;
            std::fs::write(&path, &bytes)
                .with_context(|| format!("write blob {}", hash.to_hex()))?;
        }
        Ok(hash)
    }

    /// Read and parse the blob for `hash`. Returns `None` when absent.
    pub fn get_json(&self, hash: &ContentHash) -> Result<Option<serde_json::Value>> {
        let path = self.blob_path(hash);
        if !path.exists() {
            return Ok(None);
        }
        let bytes =
            std::fs::read(&path).with_context(|| format!("read blob {}", hash.to_hex()))?;
        let v = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse blob {}", hash.to_hex()))?;
        Ok(Some(v))
    }

    pub fn exists(&self, hash: &ContentHash) -> bool {
        self.blob_path(hash).exists()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn blob_path(&self, hash: &ContentHash) -> PathBuf {
        self.dir.join(format!("{}.json", hash.to_hex()))
    }
}
