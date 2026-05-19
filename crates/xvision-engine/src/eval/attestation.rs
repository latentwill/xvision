//! Phase 3.C signed eval attestation. Ed25519 signature over canonical-JSON
//! of the run + scenario + metrics + token-usage tuple, suitable for
//! marketplace publishing.
//!
//! On-chain push lands in Plan 5 (blockchain). This module produces and
//! verifies attestations and persists them via `RunStore::record_attestation`
//! to the local `eval_attestations` table from migration 002.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::eval::run::{MetricsSummary, Run};
use crate::eval::scenario::Scenario;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvalAttestation {
    pub agent_id: String,
    pub scenario_id: String,
    pub metrics: MetricsSummary,
    pub tokens_used: TokensUsed,
    pub ran_at: DateTime<Utc>,
    pub signing_pubkey_hex: String,
    /// Signature is over canonical(JSON({everything-except-this-field-and-pubkey})).
    pub signature_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokensUsed {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

/// Build the canonical JSON payload that gets signed. Same shape used by
/// `sign` and `verify` so the bytes are identical when the attestation is
/// untampered. Object keys are sorted by `canonicalize_json` so insertion
/// order doesn't affect the signature.
fn signable_payload(
    agent_id: &str,
    scenario_id: &str,
    metrics: &MetricsSummary,
    tokens_used: &TokensUsed,
    ran_at: &DateTime<Utc>,
) -> Result<Vec<u8>> {
    let unsigned = serde_json::json!({
        "agent_id": agent_id,
        "scenario_id": scenario_id,
        "metrics": metrics,
        "tokens_used": tokens_used,
        "ran_at": ran_at,
    });
    let canonical = canonicalize_json(&unsigned);
    Ok(serde_json::to_vec(&canonical)?)
}

pub fn sign(run: &Run, scenario: &Scenario, signing_key: &SigningKey) -> Result<EvalAttestation> {
    let metrics = run
        .metrics
        .clone()
        .ok_or_else(|| anyhow!("run {} has no metrics; finalize first", run.id))?;
    let tokens_used = TokensUsed {
        input: run.actual_input_tokens.unwrap_or(0),
        output: run.actual_output_tokens.unwrap_or(0),
        total: run.actual_input_tokens.unwrap_or(0) + run.actual_output_tokens.unwrap_or(0),
    };
    let ran_at = run.completed_at.unwrap_or_else(Utc::now);

    let bytes = signable_payload(&run.agent_id, &scenario.id, &metrics, &tokens_used, &ran_at)?;
    let signature: Signature = signing_key.sign(&bytes);
    let pubkey: VerifyingKey = signing_key.verifying_key();

    Ok(EvalAttestation {
        agent_id: run.agent_id.clone(),
        scenario_id: scenario.id.clone(),
        metrics,
        tokens_used,
        ran_at,
        signing_pubkey_hex: hex::encode(pubkey.as_bytes()),
        signature_hex: hex::encode(signature.to_bytes()),
    })
}

pub fn verify(att: &EvalAttestation) -> Result<()> {
    let pubkey_bytes = hex::decode(&att.signing_pubkey_hex).map_err(|e| anyhow!("decode pubkey hex: {e}"))?;
    let pubkey_arr: [u8; 32] = pubkey_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("pubkey must be 32 bytes"))?;
    let pubkey = VerifyingKey::from_bytes(&pubkey_arr).map_err(|e| anyhow!("decode pubkey: {e}"))?;

    let sig_bytes = hex::decode(&att.signature_hex).map_err(|e| anyhow!("decode signature hex: {e}"))?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("signature must be 64 bytes"))?;
    let signature = Signature::from_bytes(&sig_arr);

    let bytes = signable_payload(
        &att.agent_id,
        &att.scenario_id,
        &att.metrics,
        &att.tokens_used,
        &att.ran_at,
    )?;
    pubkey
        .verify(&bytes, &signature)
        .map_err(|e| anyhow!("signature verification failed: {e}"))?;
    Ok(())
}

/// Recursively sort all object keys so the JSON serialization is canonical.
/// Same shape as the `xvision-marketplace::content_hash::canonicalize` we
/// converge on for on-chain hashing.
fn canonicalize_json(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), canonicalize_json(&map[k]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value};

    use super::canonicalize_json;

    #[test]
    fn canonicalize_json_is_key_order_stable() {
        let mut ordered = Map::new();
        ordered.insert("agent_id".into(), Value::String("agent-a".into()));
        ordered.insert(
            "metrics".into(),
            serde_json::json!({ "sharpe": 1.42, "n_trades": 17 }),
        );
        ordered.insert("ran_at".into(), Value::String("2025-04-01T12:00:00Z".into()));
        ordered.insert("scenario_id".into(), Value::String("crypto-bull-q1-2025".into()));

        let mut reversed = Map::new();
        reversed.insert("scenario_id".into(), Value::String("crypto-bull-q1-2025".into()));
        reversed.insert("ran_at".into(), Value::String("2025-04-01T12:00:00Z".into()));
        reversed.insert(
            "metrics".into(),
            serde_json::json!({ "n_trades": 17, "sharpe": 1.42 }),
        );
        reversed.insert("agent_id".into(), Value::String("agent-a".into()));

        let canonical_ordered = serde_json::to_vec(&canonicalize_json(&Value::Object(ordered))).unwrap();
        let canonical_reversed = serde_json::to_vec(&canonicalize_json(&Value::Object(reversed))).unwrap();

        assert_eq!(
            canonical_ordered, canonical_reversed,
            "canonical JSON bytes must be stable across object insertion orders",
        );
    }
}
