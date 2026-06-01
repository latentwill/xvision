use crate::autooptimizer::{
    config::AutoOptimizerConfig,
    content_hash::{canonicalize_json, hash_canonical_json, ContentHash},
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use ulid::Ulid;

pub fn default_key_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("no home directory found")?;
    Ok(home.join(".xvn").join("keys").join("operator.ed25519"))
}

pub fn load_or_generate_key(path: &Path) -> Result<SigningKey> {
    if path.exists() {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading key from {}", path.display()))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("key file must be exactly 32 bytes"))?;
        return Ok(SigningKey::from_bytes(&arr));
    }
    let key = SigningKey::generate(&mut OsRng);
    write_secret_key_atomic(path, &key)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("key path has no filename")?;
    let pub_path = path.with_file_name(format!("{name}.pub"));
    std::fs::write(&pub_path, key.verifying_key().as_bytes()).context("writing public key")?;
    Ok(key)
}

#[cfg(unix)]
fn write_secret_key_atomic(path: &Path, key: &SigningKey) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path.parent().context("key path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("invalid key path")?;
    let tmp_path = parent.join(format!("{stem}.tmp"));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp_path)
        .with_context(|| format!("creating key file at {}", tmp_path.display()))?;
    f.write_all(&key.to_bytes())?;
    f.flush()?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secret_key_atomic(path: &Path, key: &SigningKey) -> Result<()> {
    let parent = path.parent().context("key path has no parent")?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(path, key.to_bytes()).context("writing key file")
}

fn signing_payload(
    session_id: &Ulid,
    created_at: &DateTime<Utc>,
    config_hash: &ContentHash,
    parents: &[ContentHash],
) -> Result<Vec<u8>> {
    let parent_hex: Vec<String> = parents.iter().map(ContentHash::to_hex).collect();
    let v = json!({
        "config_hash": config_hash.to_hex(),
        "created_at": created_at.to_rfc3339(),
        "parent_strategy_hashes": parent_hex,
        "session_id": session_id.to_string(),
    });
    let canonical = canonicalize_json(&v);
    Ok(serde_json::to_vec(&canonical)?)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCommitment {
    pub session_id: Ulid,
    pub created_at: DateTime<Utc>,
    pub config_hash: ContentHash,
    pub parent_strategy_hashes: Vec<ContentHash>,
    pub signature: String,
}

impl SessionCommitment {
    pub fn new_signed(
        session_id: Ulid,
        config: &AutoOptimizerConfig,
        parents: Vec<ContentHash>,
        key: &SigningKey,
    ) -> Result<SessionCommitment> {
        let created_at = Utc::now();
        let config_json = serde_json::to_value(config)?;
        let config_hash = hash_canonical_json(&config_json);
        let payload = signing_payload(&session_id, &created_at, &config_hash, &parents)?;
        let sig: Signature = key.sign(&payload);
        let signature = hex::encode(sig.to_bytes());
        debug_assert_eq!(signature.len(), 128, "ed25519 sig is 64 bytes = 128 hex chars");
        Ok(SessionCommitment {
            session_id,
            created_at,
            config_hash,
            parent_strategy_hashes: parents,
            signature,
        })
    }

    pub fn verify(&self, public_key: &VerifyingKey) -> Result<()> {
        let payload = signing_payload(
            &self.session_id,
            &self.created_at,
            &self.config_hash,
            &self.parent_strategy_hashes,
        )?;
        let sig_bytes = hex::decode(&self.signature).context("signature is not valid hex")?;
        let arr: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("signature must be exactly 64 bytes"))?;
        let sig = Signature::from_bytes(&arr);
        public_key
            .verify(&payload, &sig)
            .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))
    }

    pub fn write_to(&self, dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("session-{}.json", self.session_id));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json.as_bytes())?;
        Ok(path)
    }

    pub fn load_from(path: &Path) -> Result<SessionCommitment> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading session commitment from {}", path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing session commitment from {}", path.display()))
    }
}
