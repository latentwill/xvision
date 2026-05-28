use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        ContentHash::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl ContentHash {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub fn of_json(v: &serde_json::Value) -> Self {
        let canonical = canonical_json(v);
        let s = serde_json::to_string(&canonical).expect("serialize canonical json");
        Self::of_bytes(s.as_bytes())
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            bail!("expected 32 bytes, got {}", bytes.len());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Returns a semantically equivalent JSON value with all object keys sorted
/// lexicographically at every level of nesting.
pub fn canonical_json(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut pairs: Vec<(String, serde_json::Value)> = map
                .iter()
                .map(|(k, val)| (k.clone(), canonical_json(val)))
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(pairs.into_iter().collect())
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonical_json).collect())
        }
        other => other.clone(),
    }
}
