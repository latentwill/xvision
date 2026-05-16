//! Content-addressed blob store.
//!
//! Path: `$blob_root/<sha256-hex>`. Same content → same path, so writing
//! the same payload twice is a no-op + dedup; deleting a row that
//! referenced a payload still in use by another row would orphan the
//! other row, so deletion is the janitor's job (tracks blob refs per
//! row) — not this layer's.
//!
//! The blob root is normally `$XVN_HOME/agent_runs/blobs/`. We don't
//! reach for `XVN_HOME` here so the store can be constructed from a
//! test temp dir or any explicit path.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlobStoreError {
    #[error("blob store io: {0}")]
    Io(#[from] io::Error),
    #[error("blob not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRef(pub String);

impl BlobRef {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stateless blob store rooted at `root_dir`. The dir is created lazily on
/// first write; reads against a missing dir return `NotFound`.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, sha_hex: &str) -> PathBuf {
        self.root.join(sha_hex)
    }

    /// Hash, write (if absent), return the `BlobRef`. Writes are atomic
    /// via a temp-file-then-rename so a partial write can't leave a
    /// readable but truncated blob.
    pub fn write(&self, payload: &[u8]) -> Result<BlobRef, BlobStoreError> {
        let sha = hex::encode(Sha256::digest(payload));
        let final_path = self.path_for(&sha);
        if final_path.exists() {
            return Ok(BlobRef(sha));
        }
        fs::create_dir_all(&self.root)?;
        let tmp_path = self.root.join(format!(".tmp-{sha}"));
        // Best-effort cleanup if a previous attempt died between
        // create_dir_all and write.
        let _ = fs::remove_file(&tmp_path);
        fs::write(&tmp_path, payload)?;
        // rename is atomic on the same filesystem.
        fs::rename(&tmp_path, &final_path)?;
        Ok(BlobRef(sha))
    }

    pub fn read(&self, blob_ref: &BlobRef) -> Result<Vec<u8>, BlobStoreError> {
        let path = self.path_for(&blob_ref.0);
        match fs::read(&path) {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(BlobStoreError::NotFound(blob_ref.0.clone()))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn exists(&self, blob_ref: &BlobRef) -> bool {
        self.path_for(&blob_ref.0).exists()
    }

    /// Delete the blob if present. No-op if absent. Used by the janitor
    /// in the retention-cli leaf — this crate exposes the primitive but
    /// does not run a janitor itself.
    pub fn delete(&self, blob_ref: &BlobRef) -> Result<(), BlobStoreError> {
        let path = self.path_for(&blob_ref.0);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_then_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = BlobStore::new(tmp.path());
        let payload = b"hello world";
        let r = store.write(payload).unwrap();
        assert!(store.exists(&r));
        let got = store.read(&r).unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn same_content_dedupes() {
        let tmp = TempDir::new().unwrap();
        let store = BlobStore::new(tmp.path());
        let r1 = store.write(b"abc").unwrap();
        let r2 = store.write(b"abc").unwrap();
        assert_eq!(r1, r2);
        // Only one file on disk.
        let entries: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(entries.len(), 1, "expected 1 blob, got {}", entries.len());
    }

    #[test]
    fn read_missing_blob_returns_notfound() {
        let tmp = TempDir::new().unwrap();
        let store = BlobStore::new(tmp.path());
        let r = BlobRef("0".repeat(64));
        match store.read(&r) {
            Err(BlobStoreError::NotFound(_)) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn delete_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let store = BlobStore::new(tmp.path());
        let r = store.write(b"x").unwrap();
        store.delete(&r).unwrap();
        store.delete(&r).unwrap(); // second delete is a no-op
        assert!(!store.exists(&r));
    }

    #[test]
    fn sha256_matches_expected() {
        let tmp = TempDir::new().unwrap();
        let store = BlobStore::new(tmp.path());
        // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let r = store.write(b"hello").unwrap();
        assert_eq!(
            r.0,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
