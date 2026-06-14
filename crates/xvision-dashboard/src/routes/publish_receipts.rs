//! Publish-receipt store — the idempotency key for marketplace publishing
//! (bead xvision-4dn).
//!
//! `POST /api/marketplace/publish` mints an identity NFT and creates a listing.
//! Without a receipt store, re-publishing the same strategy (re-click, retry,
//! refresh) mints a DUPLICATE NFT + listing — a mainnet blocker. This module
//! persists a receipt keyed by `agent_id` (the strategy ULID, which becomes the
//! NFT token id post-mint) so the publish handler can short-circuit a
//! re-publish with 409 Conflict BEFORE any chain or IPFS work.
//!
//! ## Storage
//!
//! Receipts live in the `publish_receipts` table, created by plain idempotent
//! DDL in [`crate::state::AppState::run_dashboard_migrations`] (the same
//! pattern as `dashboard_sessions` / `auth_audit`; the engine owns
//! `_sqlx_migrations`, so the dashboard does not add a versioned sqlx
//! migration). The table is keyed `agent_id PRIMARY KEY`.
//!
//! ## Ordering / residual race
//!
//! The receipt is inserted by the handler AFTER the on-chain mint+list
//! succeeds, so two genuinely-concurrent first-publishes of the same agent_id
//! can both pass the lookup and both mint before either inserts — a duplicate
//! is still possible in that narrow race. The store collapses the dominant case
//! (sequential re-click / retry / refresh); it is not a hard mutex. The UNIQUE
//! PRIMARY KEY means a concurrent double-insert still leaves exactly one row
//! ([`insert_receipt`] maps the loser's UNIQUE violation to a `Conflict`).

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::DashboardError;

/// A persisted publish receipt: the proof an `agent_id` was already minted +
/// listed. Keyed by `agent_id` (the strategy ULID / NFT token id).
#[derive(Debug, Clone, Serialize)]
pub struct PublishReceipt {
    /// The strategy ULID — the pre-mint id that becomes the NFT token id.
    pub agent_id: String,
    /// The minted IdentityRegistry token id (decimal string).
    pub token_id: String,
    /// The created ListingRegistry listing id (decimal string).
    pub listing_id: String,
    /// keccak256 of the canonical strategy JSON at publish time (debugging).
    pub content_hash: String,
    /// When the publish completed (RFC 3339).
    pub published_at: String,
    /// Creator-chosen listing name captured at publish time (defaults to the
    /// strategy's display name). `None` for receipts written before the column
    /// existed; the marketplace name-enrichment then falls back to the
    /// strategy's display name.
    pub name: Option<String>,
}

/// Look up the publish receipt for an `agent_id`. Returns `None` when the
/// agent has not been published (the common first-publish case).
///
/// A missing `publish_receipts` table is treated as "no receipt" (`Ok(None)`)
/// rather than an error, so a state that somehow skipped
/// `run_dashboard_migrations` degrades to the pre-receipt behaviour (a publish
/// can proceed) instead of 500-ing every publish. Production always runs the
/// migrations at startup; this is a defensive belt for partially-initialised
/// test states.
pub async fn find_receipt(
    pool: &SqlitePool,
    agent_id: &str,
) -> Result<Option<PublishReceipt>, DashboardError> {
    use sqlx::Row as _;
    let row = sqlx::query(
        "SELECT agent_id, token_id, listing_id, content_hash, published_at, name
         FROM publish_receipts
         WHERE agent_id = ?1",
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await;

    let row = match row {
        Ok(r) => r,
        Err(e) if is_missing_table(&e) => return Ok(None),
        Err(e) => return Err(DashboardError::Internal(anyhow::anyhow!("find_receipt: {e}"))),
    };

    Ok(row.map(|r| PublishReceipt {
        agent_id: r.get("agent_id"),
        token_id: r.get("token_id"),
        listing_id: r.get("listing_id"),
        content_hash: r.get("content_hash"),
        published_at: r.get("published_at"),
        name: r.get("name"),
    }))
}

/// Insert a publish receipt for a freshly-minted `agent_id`.
///
/// A UNIQUE/PRIMARY-KEY violation (a receipt already exists for this agent_id —
/// the TOCTOU double-submit loser) is mapped to [`DashboardError::Conflict`]
/// rather than a 500, so the table always converges to exactly one row.
pub async fn insert_receipt(
    pool: &SqlitePool,
    agent_id: &str,
    token_id: &str,
    listing_id: &str,
    content_hash: &str,
    published_at: &str,
    name: Option<&str>,
) -> Result<(), DashboardError> {
    let result = sqlx::query(
        "INSERT INTO publish_receipts (agent_id, token_id, listing_id, content_hash, published_at, name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(agent_id)
    .bind(token_id)
    .bind(listing_id)
    .bind(content_hash)
    .bind(published_at)
    .bind(name)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) if is_unique_violation(&e) => Err(DashboardError::Conflict(format!(
            "agent_id {agent_id} already has a publish receipt"
        ))),
        Err(e) => Err(DashboardError::Internal(anyhow::anyhow!("insert_receipt: {e}"))),
    }
}

/// True when the sqlx error is a SQLite UNIQUE / PRIMARY KEY constraint
/// violation (code 2067 / 1555, message contains "UNIQUE").
fn is_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db) = e {
        // SQLite surfaces UNIQUE violations with the substring "UNIQUE
        // constraint failed"; matching the message is the portable check
        // (the numeric code accessor differs across sqlx/sqlite versions).
        return db.message().contains("UNIQUE constraint failed");
    }
    false
}

/// True when the sqlx error is "no such table: publish_receipts" — a state that
/// never ran the dashboard migrations. Treated as "no receipt" by
/// [`find_receipt`].
fn is_missing_table(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db) = e {
        return db.message().contains("no such table");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Open a tempdir-backed AppState pool with the dashboard migrations run
    /// (so the `publish_receipts` table exists).
    async fn migrated_pool() -> (SqlitePool, TempDir) {
        let tmp = TempDir::new().unwrap();
        let state = crate::state::AppState::new(tmp.path().to_path_buf())
            .await
            .expect("init dashboard state");
        state
            .run_dashboard_migrations()
            .await
            .expect("dashboard migrations");
        (state.pool.clone(), tmp)
    }

    #[tokio::test]
    async fn insert_then_find_roundtrip() {
        let (pool, _tmp) = migrated_pool().await;
        let agent_id = "01HZZZZZZZZZZZZZZZZZZZZZZZZ";
        insert_receipt(
            &pool,
            agent_id,
            "1234",
            "56",
            &"cd".repeat(32),
            "2026-06-13T12:00:00Z",
            Some("BTC Momentum"),
        )
        .await
        .unwrap();

        let found = find_receipt(&pool, agent_id).await.unwrap();
        let r = found.expect("receipt present after insert");
        assert_eq!(r.agent_id, agent_id);
        assert_eq!(r.token_id, "1234");
        assert_eq!(r.listing_id, "56");
        assert_eq!(r.content_hash, "cd".repeat(32));
        assert_eq!(r.published_at, "2026-06-13T12:00:00Z");
        assert_eq!(
            r.name.as_deref(),
            Some("BTC Momentum"),
            "the creator-chosen listing name round-trips through the receipt store"
        );
    }

    #[tokio::test]
    async fn find_receipt_absent_is_none() {
        let (pool, _tmp) = migrated_pool().await;
        let found = find_receipt(&pool, "01NOPENOPENOPENOPENOPENOPE").await.unwrap();
        assert!(found.is_none(), "fresh table returns None for an unknown id");
    }

    #[tokio::test]
    async fn duplicate_insert_is_conflict_and_leaves_one_row() {
        let (pool, _tmp) = migrated_pool().await;
        let agent_id = "01DUPDUPDUPDUPDUPDUPDUPDUP0";
        insert_receipt(&pool, agent_id, "1", "1", "00", "2026-06-13T00:00:00Z", None)
            .await
            .expect("first insert succeeds");

        // A second insert for the same agent_id is the TOCTOU loser → Conflict.
        let second = insert_receipt(&pool, agent_id, "2", "2", "11", "2026-06-13T00:01:00Z", None).await;
        assert!(
            matches!(second, Err(DashboardError::Conflict(_))),
            "second insert must map the UNIQUE violation to Conflict, got {second:?}"
        );

        // Exactly one row survived, and it is the FIRST one (the winner).
        use sqlx::Row as _;
        let row = sqlx::query(
            "SELECT COUNT(*) AS n, MIN(token_id) AS tok FROM publish_receipts WHERE agent_id = ?1",
        )
        .bind(agent_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let n: i64 = row.get("n");
        assert_eq!(n, 1, "exactly one receipt row survives the double-insert");
        let found = find_receipt(&pool, agent_id).await.unwrap().unwrap();
        assert_eq!(found.token_id, "1", "the first insert is the winner");
    }

    #[tokio::test]
    async fn find_receipt_missing_table_is_none_not_error() {
        // A state WITHOUT run_dashboard_migrations has no publish_receipts
        // table; find_receipt must degrade to Ok(None), not 500.
        let tmp = TempDir::new().unwrap();
        let state = crate::state::AppState::new(tmp.path().to_path_buf())
            .await
            .expect("init dashboard state");
        // NOTE: deliberately NOT running run_dashboard_migrations.
        let found = find_receipt(&state.pool, "01ANYANYANYANYANYANYANYANY").await;
        assert!(
            matches!(found, Ok(None)),
            "missing table degrades to Ok(None), got {found:?}"
        );
    }
}
