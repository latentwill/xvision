//! BLAKE3 content-hash type for the autoresearcher blob store (AR-1 Task 2).
//!
//! `ContentHash` is a newtype over a 32-byte BLAKE3 digest.
//! `canonicalize_json` sorts object keys so the hash of a JSON value is
//! independent of key insertion order.

use serde_json::Value;

/// A 32-byte BLAKE3 content hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Hash raw bytes.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        ContentHash(*blake3::hash(bytes).as_bytes())
    }

    /// Hash the canonical JSON form of `value` (object keys sorted recursively).
    pub fn of_json(value: &Value) -> Self {
        let canonical = canonicalize_json(value);
        let bytes = serde_json::to_vec(&canonical)
            .expect("serde_json::to_vec is infallible on a well-formed Value");
        Self::of_bytes(&bytes)
    }

    /// Return the lower-hex digest string (64 characters for a 32-byte hash).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Hash raw bytes — free-function alias used by `BlobStore::put`.
pub fn hash_bytes(bytes: &[u8]) -> ContentHash {
    ContentHash::of_bytes(bytes)
}

/// Return a deterministic canonical form of `value` with object keys sorted.
///
/// Arrays are left in their original order; only map key order is normalised.
pub fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            let sorted = keys
                .into_iter()
                .map(|k| (k.clone(), canonicalize_json(&map[k])))
                .collect();
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_json).collect()),
        other => other.clone(),
    }
}
