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
//!
//! `bars_content_hash` is produced by `eval-candle-integrity-and-manifest`
//! when that track lands. Until then, callers may pass a stub (e.g. the SHA-256
//! of the bars file path) — the receipt is still stable per `engine_version`
//! as long as the stub is deterministic for a given fixture.
//!
//! `manifest_canonical` is reserved but left `NULL` by this track;
//! `eval-candle-integrity-and-manifest` (migration 027) populates it once its
//! pinned-fixtures work lands.
//!
//! ## Persistence
//!
//! Receipts are stored in the `determinism_receipts` SQLite table (engine DB,
//! migration 026). The table is keyed by `run_id` (one receipt per run).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Inputs required to mint a determinism receipt.
#[derive(Debug, Clone)]
pub struct ReceiptInputs {
    /// Run ULID.
    pub run_id: String,
    /// Strategy content hash (e.g. blake3 or sha256 of the serialized strategy).
    pub strategy_hash: String,
    /// Scenario identifier.
    pub scenario_id: String,
    /// Content hash of the OHLCV bars fixture. May be a stub (sha256 of the
    /// file path) until `eval-candle-integrity-and-manifest` provides the
    /// canonical hash.
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
    /// Reserved for `eval-candle-integrity-and-manifest`. Always `None` from
    /// this track; the downstream track's migration populates it.
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
        let hash_bytes = hasher.finalize();
        let receipt_hash = hex::encode(hash_bytes);

        DeterminismReceipt {
            run_id: inputs.run_id.clone(),
            receipt_hash,
            engine_version: inputs.engine_version.clone(),
            schema_version: inputs.schema_version.clone(),
            created_at: Utc::now(),
            manifest_canonical: None,
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
    fn manifest_canonical_is_none_by_default() {
        let r = DeterminismReceipt::mint(&test_inputs());
        assert!(
            r.manifest_canonical.is_none(),
            "manifest_canonical must be None (reserved for candle-integrity track)"
        );
    }
}
