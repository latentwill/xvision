//! CycleSeal — signed manifest of one evening cycle.
//! Operator-surface display name: "Evening summary" (terminology lock 2026-05-27).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use super::content_hash::{canonical_json, ContentHash};

pub const OPERATOR_DISPLAY_LABEL: &str = "Evening summary";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleSeal {
    pub seal_id: ulid::Ulid,
    pub cycle_id: String,
    pub merkle_root: ContentHash,
    pub node_count: usize,
    /// Raw Ed25519 signature as 128-char hex.
    pub operator_signature: String,
    pub sealed_at: DateTime<Utc>,
    pub session_id: String,
}

fn signing_payload(
    cycle_id: &str,
    session_id: &str,
    merkle_root: &ContentHash,
    node_count: usize,
    sealed_at: &DateTime<Utc>,
) -> Vec<u8> {
    let v = serde_json::json!({
        "cycle_id": cycle_id,
        "merkle_root": merkle_root.to_hex(),
        "node_count": node_count,
        "sealed_at": sealed_at.to_rfc3339(),
        "session_id": session_id,
    });
    serde_json::to_string(&canonical_json(&v))
        .expect("canonical JSON serialization is infallible")
        .into_bytes()
}

pub fn build_and_sign(
    cycle_id: &str,
    session_id: &str,
    merkle_root: ContentHash,
    node_count: usize,
    key: &SigningKey,
) -> Result<CycleSeal> {
    let seal_id = ulid::Ulid::new();
    let sealed_at = Utc::now();
    let payload = signing_payload(cycle_id, session_id, &merkle_root, node_count, &sealed_at);
    let sig: Signature = key.sign(&payload);
    let operator_signature = hex::encode(sig.to_bytes());
    Ok(CycleSeal {
        seal_id,
        cycle_id: cycle_id.to_owned(),
        merkle_root,
        node_count,
        operator_signature,
        sealed_at,
        session_id: session_id.to_owned(),
    })
}

impl CycleSeal {
    pub fn verify(&self, public_key: &VerifyingKey) -> Result<()> {
        let payload = signing_payload(
            &self.cycle_id,
            &self.session_id,
            &self.merkle_root,
            self.node_count,
            &self.sealed_at,
        );
        let sig_bytes = hex::decode(&self.operator_signature).context("decode operator_signature hex")?;
        anyhow::ensure!(
            sig_bytes.len() == 64,
            "expected 64-byte signature, got {}",
            sig_bytes.len()
        );
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&sig_bytes);
        let sig = Signature::from_bytes(&arr);
        public_key
            .verify(&payload, &sig)
            .context("Ed25519 signature verification failed")?;
        Ok(())
    }

    // The operator_signature DB column stores: {session_id}:{node_count}:{sig_hex}
    // because the cycle_seals schema has no dedicated session_id / node_count columns.
    pub async fn persist(&self, pool: &SqlitePool) -> Result<()> {
        let stored_sig = format!(
            "{}:{}:{}",
            self.session_id, self.node_count, self.operator_signature
        );
        sqlx::query(
            "INSERT INTO cycle_seals \
             (seal_id, cycle_id, merkle_root, operator_signature, sealed_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(self.seal_id.to_string())
        .bind(&self.cycle_id)
        .bind(self.merkle_root.to_hex())
        .bind(&stored_sig)
        .bind(self.sealed_at.to_rfc3339())
        .execute(pool)
        .await
        .context("persist cycle_seal")?;
        Ok(())
    }

    pub async fn load(pool: &SqlitePool, seal_id: &str) -> Result<Option<Self>> {
        let row = sqlx::query(
            "SELECT seal_id, cycle_id, merkle_root, operator_signature, sealed_at \
             FROM cycle_seals WHERE seal_id = ?",
        )
        .bind(seal_id)
        .fetch_optional(pool)
        .await
        .context("load cycle_seal")?;

        let row = match row {
            None => return Ok(None),
            Some(r) => r,
        };

        let seal_id_str: String = row.try_get("seal_id").context("seal_id")?;
        let cycle_id: String = row.try_get("cycle_id").context("cycle_id")?;
        let merkle_hex: String = row.try_get("merkle_root").context("merkle_root")?;
        let stored_sig: String = row.try_get("operator_signature").context("operator_signature")?;
        let sealed_str: String = row.try_get("sealed_at").context("sealed_at")?;

        let seal_id = seal_id_str.parse::<ulid::Ulid>().context("parse seal_id")?;
        let merkle_root = ContentHash::from_hex(&merkle_hex).context("parse merkle_root")?;
        let sealed_at = DateTime::parse_from_rfc3339(&sealed_str)
            .context("parse sealed_at")?
            .with_timezone(&Utc);
        let (session_id, node_count, operator_signature) = parse_stored_sig(&stored_sig)?;

        Ok(Some(CycleSeal {
            seal_id,
            cycle_id,
            merkle_root,
            node_count,
            operator_signature,
            sealed_at,
            session_id,
        }))
    }
}

fn parse_stored_sig(s: &str) -> Result<(String, usize, String)> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    anyhow::ensure!(
        parts.len() == 3,
        "malformed stored signature: expected session_id:node_count:sig_hex, got {:?}",
        s
    );
    let session_id = parts[0].to_owned();
    let node_count: usize = parts[1].parse().context("parse node_count")?;
    let sig_hex = parts[2].to_owned();
    Ok((session_id, node_count, sig_hex))
}
