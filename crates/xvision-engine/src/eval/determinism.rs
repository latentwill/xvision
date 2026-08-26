//! Determinism receipt minter for eval runs.
//!
//! A *receipt* is a stable hash that proves a given `(strategy, scenario,
//! bars_content, seed, engine_version)` tuple was evaluated under a specific
//! schema version. Receipts allow two operators to compare their runs of
//! the same scenario and verify they used identical inputs and the same
//! engine build.
//!
//! ## Receipt hash composition
//!
//! ```text
//! receipt_hash = sha256(
//!     strategy_hash || "\0" ||
//!     scenario_id   || "\0" ||
//!     bars_content_hash || "\0" ||
//!     seed (as decimal string) || "\0" ||
//!     engine_version
//! )
//! ```
//! `bars_content_hash` is computed from the loaded rows by
//! [`canonical_bars_content_hash`]. It must never be a path or cache-key hash:
//! those identify a location, not the bytes evaluated.
//!
//! The optional `manifest_canonical` receipt column stores the compact JSON
//! manifest built by [`ReceiptManifest::canonical_json`].
//!
//! The receipt table is intentionally independent of `eval_runs`: a run can be
//! re-read and verified even when its mutable summary fields change.
//!
//! ## Persistence
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use xvision_core::market::Ohlcv;
/// Canonical manifest persisted with a determinism receipt.
///
/// The fields that are not available at the receipt seam remain JSON `null`.
/// `bars_content_hash` and `engine_version` are required because a receipt
/// without either cannot identify the evaluated input or implementation.
///
/// Replay-provenance fields are audit metadata only and do not affect
/// `receipt_hash`, which remains the five-field input tuple documented above.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptManifest {
    pub bars_content_hash: String,
    pub bars_rows: usize,
    pub bars_start: Option<String>,
    pub bars_end: Option<String>,
    pub bars_source: String,
    pub scenario_id: String,
    pub strategy_hash: String,
    pub strategy_source_hash: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub prompt_version: Option<String>,
    pub system_prompt_hash: Option<String>,
    pub tool_cache_recording_id: Option<String>,
    pub engine_version: String,
    pub seed: u64,
    /// Original run whose cached tool responses were replayed, when known.
    #[serde(default)]
    pub replay_of_run_id: Option<String>,
    /// Whether replay inputs matched the original receipt; `None` means
    /// verification was unavailable.
    #[serde(default)]
    pub replay_inputs_match: Option<bool>,
    /// Reasons replay inputs differed, or why verification could not complete.
    #[serde(default)]
    pub replay_mismatches: Vec<String>,
}

impl ReceiptManifest {
    /// Serialise the manifest as compact, deterministic JSON.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("receipt manifest is JSON-safe")
    }
}

/// Hash loaded OHLCV rows, independent of source file encoding or row order.
///
/// Each row is sorted by timestamp and then by the raw IEEE-754 bits of its
/// OHLCV values. The hash input is fixed-width binary data:
/// `(timestamp_seconds, timestamp_nanoseconds, open_bits, high_bits,
/// low_bits, close_bits, volume_bits)`. Using `to_bits` preserves `-0.0` and
/// NaN payloads instead of introducing formatting or parser drift.
pub fn canonical_bars_content_hash(bars: &[Ohlcv]) -> String {
    let mut rows: Vec<&Ohlcv> = bars.iter().collect();
    rows.sort_unstable_by(|a, b| {
        (
            a.timestamp.timestamp(),
            a.timestamp.timestamp_subsec_nanos(),
            a.open.to_bits(),
            a.high.to_bits(),
            a.low.to_bits(),
            a.close.to_bits(),
            a.volume.to_bits(),
        )
            .cmp(&(
                b.timestamp.timestamp(),
                b.timestamp.timestamp_subsec_nanos(),
                b.open.to_bits(),
                b.high.to_bits(),
                b.low.to_bits(),
                b.close.to_bits(),
                b.volume.to_bits(),
            ))
    });

    let mut hasher = Sha256::new();
    for bar in rows {
        hasher.update(bar.timestamp.timestamp().to_be_bytes());
        hasher.update(bar.timestamp.timestamp_subsec_nanos().to_be_bytes());
        for bits in [
            bar.open.to_bits(),
            bar.high.to_bits(),
            bar.low.to_bits(),
            bar.close.to_bits(),
            bar.volume.to_bits(),
        ] {
            hasher.update(bits.to_be_bytes());
        }
    }
    hex::encode(hasher.finalize())
}


/// Inputs required to mint a determinism receipt.
#[derive(Debug, Clone)]
pub struct ReceiptInputs {
    /// Run ULID.
    pub run_id: String,
    /// Strategy content hash (e.g. blake3 or sha256 of the serialized strategy).
    pub strategy_hash: String,
    /// Scenario identifier.
    pub scenario_id: String,
    /// SHA-256 of the canonical loaded OHLCV rows. A path or cache-key hash
    /// is not a valid receipt input.
    pub bars_content_hash: String,
    /// Random seed used for this run.
    pub seed: u64,
    /// Engine version string (e.g. cargo package version).
    pub engine_version: String,
    /// Schema version of the decision/fill trace at receipt-mint time.
    pub schema_version: String,
}

/// A minted determinism receipt ready for persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct DeterminismReceipt {
    pub run_id: String,
    pub receipt_hash: String,
    pub engine_version: String,
    pub schema_version: String,
    pub created_at: DateTime<Utc>,
    /// Compact canonical JSON manifest of all known reproduction inputs.
    pub manifest_canonical: Option<String>,
}

impl DeterminismReceipt {
    /// Mint a receipt from the provided inputs. The hash is a hex-encoded
    /// SHA-256 digest of the canonical input string.
    ///
    /// # Determinism guarantee
    ///
    /// Two calls with identical `inputs` values produce the same
    /// `receipt_hash`. Any change to any input field changes the hash.
    pub fn mint(inputs: &ReceiptInputs) -> Self {
        Self::mint_with_manifest_json(inputs, None)
    }

    /// Mint a receipt and persist the supplied canonical manifest in the same
    /// row. The receipt hash remains the required five-field tuple.
    pub fn mint_with_manifest(inputs: &ReceiptInputs, manifest: &ReceiptManifest) -> Self {
        Self::mint_with_manifest_json(inputs, Some(manifest.canonical_json()))
    }

    fn mint_with_manifest_json(inputs: &ReceiptInputs, manifest: Option<String>) -> Self {
        let canonical = format!(
            "{}\0{}\0{}\0{}\0{}",
            inputs.strategy_hash,
            inputs.scenario_id,
            inputs.bars_content_hash,
            inputs.seed,
            inputs.engine_version,
        );
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let receipt_hash = hex::encode(hasher.finalize());

        DeterminismReceipt {
            run_id: inputs.run_id.clone(),
            receipt_hash,
            engine_version: inputs.engine_version.clone(),
            schema_version: inputs.schema_version.clone(),
            created_at: Utc::now(),
            manifest_canonical: manifest,
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

/// Persist a determinism receipt to the `determinism_receipts` table.
/// The table is created by migration 026. Callers must ensure migrations
/// have run before calling this.
///
/// Uses `INSERT OR REPLACE` so a re-run with identical inputs produces
/// an idempotent update (the hash will be identical, only `created_at`
/// changes).
pub async fn persist_receipt(pool: &SqlitePool, receipt: &DeterminismReceipt) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO determinism_receipts \
         (run_id, receipt_hash, engine_version, schema_version, manifest_canonical, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&receipt.run_id)
    .bind(&receipt.receipt_hash)
    .bind(&receipt.engine_version)
    .bind(&receipt.schema_version)
    .bind(&receipt.manifest_canonical)
    .bind(receipt.created_at.to_rfc3339())
    .execute(pool)
    .await
    .with_context(|| format!("insert determinism_receipt run_id={}", receipt.run_id))?;
    Ok(())
}

/// Read a determinism receipt by run_id. Returns `Ok(None)` when no row exists.
pub async fn read_receipt(pool: &SqlitePool, run_id: &str) -> Result<Option<DeterminismReceipt>> {
    let row = sqlx::query(
        "SELECT run_id, receipt_hash, engine_version, schema_version, \
                manifest_canonical, created_at \
         FROM determinism_receipts WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .context("read determinism_receipt")?;

    let Some(row) = row else {
        return Ok(None);
    };

    use sqlx::Row;
    let run_id: String = row.try_get("run_id").context("read receipt run_id")?;
    let receipt_hash: String = row.try_get("receipt_hash").context("read receipt receipt_hash")?;
    let engine_version: String = row
        .try_get("engine_version")
        .context("read receipt engine_version")?;
    let schema_version: String = row
        .try_get("schema_version")
        .context("read receipt schema_version")?;
    let manifest_canonical: Option<String> = row
        .try_get("manifest_canonical")
        .context("read receipt manifest_canonical")?;
    let created_at_str: String = row.try_get("created_at").context("read receipt created_at")?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .with_context(|| format!("parse receipt created_at {created_at_str:?}"))?
        .with_timezone(&Utc);

    Ok(Some(DeterminismReceipt {
        run_id,
        receipt_hash,
        engine_version,
        schema_version,
        created_at,
        manifest_canonical,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_inputs() -> ReceiptInputs {
        ReceiptInputs {
            run_id: "01TESTRUN00000000000000000".into(),
            strategy_hash: "abc123strategy".into(),
            scenario_id: "crypto-bull-q1-2025".into(),
            bars_content_hash: "deadbeefbarscontentshasum".into(),
            seed: 42,
            engine_version: "0.1.0".into(),
            schema_version: "2".into(),
        }
    }

    #[test]
    fn mint_produces_non_empty_hash() {
        let r = DeterminismReceipt::mint(&test_inputs());
        assert!(!r.receipt_hash.is_empty());
        // SHA-256 hex is always 64 characters.
        assert_eq!(r.receipt_hash.len(), 64);
    }

    #[test]
    fn mint_is_stable_across_identical_inputs() {
        let inputs = test_inputs();
        let r1 = DeterminismReceipt::mint(&inputs);
        let r2 = DeterminismReceipt::mint(&inputs);
        assert_eq!(
            r1.receipt_hash, r2.receipt_hash,
            "identical inputs must produce the same hash"
        );
    }

    #[test]
    fn mint_changes_on_any_input_change() {
        let base = test_inputs();
        let r_base = DeterminismReceipt::mint(&base);

        // Different strategy hash.
        let r_strat = DeterminismReceipt::mint(&ReceiptInputs {
            strategy_hash: "differentstrategy".into(),
            ..base.clone()
        });
        assert_ne!(r_base.receipt_hash, r_strat.receipt_hash, "strategy_hash change");

        // Different seed.
        let r_seed = DeterminismReceipt::mint(&ReceiptInputs {
            seed: 999,
            ..base.clone()
        });
        assert_ne!(r_base.receipt_hash, r_seed.receipt_hash, "seed change");

        // Different bars hash.
        let r_bars = DeterminismReceipt::mint(&ReceiptInputs {
            bars_content_hash: "differentbars".into(),
            ..base.clone()
        });
        assert_ne!(
            r_base.receipt_hash, r_bars.receipt_hash,
            "bars_content_hash change"
        );

        // Different engine version.
        let r_eng = DeterminismReceipt::mint(&ReceiptInputs {
            engine_version: "0.2.0".into(),
            ..base.clone()
        });
        assert_ne!(r_base.receipt_hash, r_eng.receipt_hash, "engine_version change");

        // Different scenario id.
        let r_scen = DeterminismReceipt::mint(&ReceiptInputs {
            scenario_id: "crypto-bear-q2-2025".into(),
            ..base.clone()
        });
        assert_ne!(r_base.receipt_hash, r_scen.receipt_hash, "scenario_id change");
    }

    #[test]
    fn bars_hash_is_order_independent_and_uses_float_bits() {
        use chrono::TimeZone;

        let first = Ohlcv {
            timestamp: Utc.timestamp_opt(2, 3).single().unwrap(),
            open: -0.0,
            high: 2.0,
            low: 1.0,
            close: 1.5,
            volume: 10.0,
        };
        let second = Ohlcv {
            timestamp: Utc.timestamp_opt(1, 4).single().unwrap(),
            open: 3.0,
            high: 4.0,
            low: 2.0,
            close: 3.5,
            volume: 20.0,
        };
        let forward = canonical_bars_content_hash(&[first.clone(), second.clone()]);
        let reverse = canonical_bars_content_hash(&[second, first.clone()]);
        assert_eq!(forward, reverse);
        assert_ne!(
            forward,
            canonical_bars_content_hash(&[Ohlcv {
                open: 0.0,
                ..first
            }])
        );
    }

    #[test]
    fn manifest_replay_fields_default_when_absent() {
        let old_manifest = r#"{
            "bars_content_hash":"bars",
            "bars_rows":1,
            "bars_start":null,
            "bars_end":null,
            "bars_source":"db_cache",
            "scenario_id":"scenario",
            "strategy_hash":"strategy",
            "strategy_source_hash":null,
            "provider":null,
            "model":null,
            "prompt_version":null,
            "system_prompt_hash":null,
            "tool_cache_recording_id":null,
            "engine_version":"engine",
            "seed":0
        }"#;
        let manifest: ReceiptManifest = serde_json::from_str(old_manifest).unwrap();
        assert_eq!(manifest.replay_of_run_id, None);
        assert_eq!(manifest.replay_inputs_match, None);
        assert!(manifest.replay_mismatches.is_empty());
    }

    #[test]
    fn manifest_canonical_is_none_by_default() {
        let r = DeterminismReceipt::mint(&test_inputs());
        assert!(
            r.manifest_canonical.is_none(),
            "manifest_canonical must be None (reserved for candle-integrity track)"
        );
    }
}
