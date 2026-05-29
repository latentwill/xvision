use std::path::PathBuf;

use tempfile::tempdir;
use xvision_engine::autoresearch::blob_store::BlobStore;
use xvision_engine::autoresearch::content_hash::ContentHash;

// ── JSON round-trips ───────────────────────────────────────────────────────

#[tokio::test]
async fn put_then_get_round_trips_json() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let value = serde_json::json!({"hello": "world", "n": 42});
    let hash = store.put_json(&value).await.unwrap();
    let loaded = store.get_json(&hash).await.unwrap();
    assert_eq!(loaded, value);
}

#[tokio::test]
async fn put_json_is_idempotent() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let value = serde_json::json!({"k": "v"});
    let h1 = store.put_json(&value).await.unwrap();
    let h2 = store.put_json(&value).await.unwrap();
    assert_eq!(h1, h2);
}

#[tokio::test]
async fn get_json_missing_returns_not_found_error() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let bogus = ContentHash::of_bytes(b"no such blob");
    let err = store.get_json(&bogus).await.unwrap_err();
    assert!(err.to_string().contains("not found"), "got: {err}");
}

// ── Bytes round-trips ──────────────────────────────────────────────────────

#[tokio::test]
async fn put_bytes_then_get_bytes_round_trip() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let payload = b"raw bytes can also be stored";
    let hash = store.put_bytes(payload).await.unwrap();
    let loaded = store.get_bytes(&hash).await.unwrap();
    assert_eq!(loaded.as_slice(), payload);
}

#[tokio::test]
async fn put_bytes_is_idempotent() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let payload = b"same bytes, same hash";
    let h1 = store.put_bytes(payload).await.unwrap();
    let h2 = store.put_bytes(payload).await.unwrap();
    assert_eq!(h1, h2);
}

#[tokio::test]
async fn get_bytes_missing_returns_not_found_error() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let bogus = ContentHash::of_bytes(b"never written");
    let err = store.get_bytes(&bogus).await.unwrap_err();
    assert!(err.to_string().contains("not found"), "got: {err}");
}

// ── On-disk filename ───────────────────────────────────────────────────────

#[tokio::test]
async fn on_disk_path_for_bytes_matches_hash_hex() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let payload = b"path check";
    let hash = store.put_bytes(payload).await.unwrap();
    let hex = hash.to_hex();
    assert_eq!(hex.len(), 64, "BLAKE3 hex must be 64 chars");
    let (h1, rest) = hex.split_at(2);
    let (h2, tail) = rest.split_at(2);
    let expected = dir.path().join(h1).join(h2).join(format!("{tail}.bin"));
    assert!(expected.exists(), "expected file at {}", expected.display());
}

#[tokio::test]
async fn on_disk_path_for_json_matches_hash_hex() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let value = serde_json::json!({"check": true});
    let hash = store.put_json(&value).await.unwrap();
    let hex = hash.to_hex();
    let (h1, rest) = hex.split_at(2);
    let (h2, tail) = rest.split_at(2);
    let expected = dir.path().join(h1).join(h2).join(format!("{tail}.json"));
    assert!(expected.exists(), "expected file at {}", expected.display());
}

// ── exists ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn exists_returns_true_after_put_bytes() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let hash = store.put_bytes(b"existence check").await.unwrap();
    assert!(store.exists(&hash));
}

#[tokio::test]
async fn exists_returns_true_after_put_json() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let hash = store.put_json(&serde_json::json!({"e": 1})).await.unwrap();
    assert!(store.exists(&hash));
}

#[tokio::test]
async fn exists_returns_false_for_unknown_hash() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let hash = ContentHash::of_bytes(b"never put");
    assert!(!store.exists(&hash));
}

// ── Sync constructor and default_root ──────────────────────────────────────

#[test]
fn new_constructs_without_io() {
    let root = PathBuf::from("/tmp/xvision-test-nonexistent-new");
    let store = BlobStore::new(root.clone());
    assert_eq!(store.root(), root.as_path());
}

#[test]
fn default_root_contains_xvn_lineage_blobs() {
    let root = BlobStore::default_root().expect("home dir available in test env");
    assert!(
        root.ends_with(".xvn/lineage/blobs"),
        "expected path ending in .xvn/lineage/blobs, got: {}",
        root.display()
    );
}
