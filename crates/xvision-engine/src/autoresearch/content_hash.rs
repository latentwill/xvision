use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHash(pub [u8; 32]);

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(self.0))
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let hex_str = String::deserialize(d)?;
        let bytes = hex::decode(&hex_str).map_err(serde::de::Error::custom)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 hex bytes (64 chars)"))?;
        Ok(ContentHash(arr))
    }
}

pub fn canonicalize_json(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .into_iter()
                .map(|(k, val)| (k, canonicalize_json(val)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(canonicalize_json).collect()),
        other => other,
    }
}

pub fn hash_canonical_json<T: Serialize>(value: &T) -> Result<ContentHash> {
    let v = serde_json::to_value(value)?;
    let canonical = canonicalize_json(v);
    let bytes = serde_json::to_vec(&canonical)?;
    let hash = blake3::hash(&bytes);
    Ok(ContentHash(*hash.as_bytes()))
}
