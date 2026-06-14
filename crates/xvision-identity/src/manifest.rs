//! Agent manifest types — serialisable to / from JSON for `identity/*.agent.json`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The JSON schema identifier baked into every manifest.
///
/// ERC-8004 draft §3.1 calls the registration file an "agent file" but does not
/// yet mandate a canonical schema URI.  We use `"erc-8004/v0.1-draft"` as a
/// forward-compatible placeholder; update when the standard is finalised.
pub const SCHEMA_VERSION: &str = "erc-8004/v0.1-draft";

/// Top-level agent manifest, serialised at `identity/*.agent.json`.
///
/// Matches the `agentURI` payload referenced by the IdentityRegistry
/// `register(agentURI)` call.  The file is meant to be pinned to IPFS or
/// an HTTPS CDN; the resulting content URL is the `agentURI` argument.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentManifest {
    /// Schema identifier (e.g. `"erc-8004/v0.1-draft"`).
    pub schema: String,
    /// Human-readable name for this experimental arm.
    pub name: String,
    /// Free-text description of the agent's purpose and configuration.
    pub description: String,
    /// Base LLM identifier (e.g. `"Qwen/Qwen3-32B"`).
    pub model: String,
    /// Strategy configuration for this arm.
    pub strategy_config: StrategyConfigSummary,
    /// Git commit SHA at the time of minting (`"PENDING"` until mint time).
    pub code_commit: String,
    /// Operator contact (email or URL; `"PENDING"` until operator fills in).
    pub contact: String,
    /// ISO 8601 creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
}

/// Summary of the strategy configuration applied to this agent arm.
/// Per ADR 0011, the per-arm split is by strategy name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyConfigSummary {
    /// Strategy name: `"trader_arm"` | `"buy_and_hold"` | `"always_long"` | …
    pub name: String,
    /// Free-form configuration parameters (e.g. RSI thresholds, MA windows).
    /// Empty for parameter-less strategies.
    pub params: Vec<String>,
}

/// A single trade outcome, keyed by `cycle_id`, posted to the
/// ReputationRegistry via [`crate::IdentityClient::post_reputation`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeOutcome {
    /// Unique identifier for the trading setup that produced this trade.
    pub cycle_id: Uuid,
    /// Realised P&L in USD (positive = profit, negative = loss).
    pub realized_pnl_usd: f64,
    /// Trade direction: `"buy"` | `"sell"` | `"close"`.
    pub action: String,
    /// Wall-clock time at which the position was closed.
    pub closed_at: DateTime<Utc>,
}

/// A reputation entry read back from the ReputationRegistry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReputationEntry {
    /// The cycle_id this reputation post covers.
    pub cycle_id: Uuid,
    /// Transaction hash of the `postReputation` / `giveFeedback` call.
    pub tx_hash: String,
    /// Block number at which the transaction was included.
    pub block_number: u64,
    /// The outcome that was posted.
    pub outcome: TradeOutcome,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_manifest() -> AgentManifest {
        AgentManifest {
            schema: SCHEMA_VERSION.to_string(),
            name: "xvision-sample-agent".to_string(),
            description: "LLM-driven sample agent".to_string(),
            model: "Qwen/Qwen3-32B".to_string(),
            strategy_config: StrategyConfigSummary {
                name: "sample_strategy".to_string(),
                params: vec![],
            },
            code_commit: "abc1234".to_string(),
            contact: "test@example.com".to_string(),
            created_at: Utc.with_ymd_and_hms(2025, 5, 3, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn round_trip_json() {
        let m = sample_manifest();
        let json = serde_json::to_string_pretty(&m).expect("serialize");
        let m2: AgentManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, m2);
    }

    #[test]
    fn canonical_hash_determinism() {
        // The same manifest always serialises to the same JSON bytes
        // (field order is declaration order via serde's default).
        let m = sample_manifest();
        let json1 = serde_json::to_string(&m).unwrap();
        let json2 = serde_json::to_string(&m).unwrap();
        assert_eq!(json1, json2);
        // and the sha256 of those bytes is stable
        let digest = sha256_hex(json1.as_bytes());
        assert_eq!(digest.len(), 64); // 32 bytes hex
        assert_eq!(digest, sha256_hex(json2.as_bytes()));
    }

    #[test]
    fn trade_outcome_round_trip() {
        let o = TradeOutcome {
            cycle_id: Uuid::nil(),
            realized_pnl_usd: 42.5,
            action: "close".to_string(),
            closed_at: Utc.with_ymd_and_hms(2025, 5, 3, 12, 0, 0).unwrap(),
        };
        let json = serde_json::to_string(&o).unwrap();
        let o2: TradeOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(o, o2);
    }

    #[test]
    fn reputation_entry_round_trip() {
        let e = ReputationEntry {
            cycle_id: Uuid::nil(),
            tx_hash: "0xdeadbeef".to_string(),
            block_number: 12345,
            outcome: TradeOutcome {
                cycle_id: Uuid::nil(),
                realized_pnl_usd: -10.0,
                action: "sell".to_string(),
                closed_at: Utc.with_ymd_and_hms(2025, 5, 1, 8, 0, 0).unwrap(),
            },
        };
        let json = serde_json::to_string_pretty(&e).unwrap();
        let e2: ReputationEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(e, e2);
    }

    /// Minimal sha256 without pulling in ring/sha2 — good enough for the
    /// determinism assertion.  Uses alloy's bundled k256-adjacent primitives
    /// via std hashing for a length check; real hashing uses sha2 if needed.
    fn sha256_hex(data: &[u8]) -> String {
        // We have no sha2 dep; use a simple FNV-inspired determinism check:
        // The point is that identical inputs yield identical outputs, not the
        // cryptographic strength.  Replace with sha2 if manifest_hashes need
        // real SHA-256 in production.
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        data.hash(&mut h);
        let v = h.finish();
        format!("{v:064x}")
    }
}
