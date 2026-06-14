//! Real-money acknowledgement guard and global pause-gate for manual CLI tools
//! (`fire-trade`, `close-position`).
//!
//! Provides two guards:
//!
//! 1. [`require_real_money_ack`] — a pure, unit-testable helper that refuses to
//!    proceed when the venue is Byreal on mainnet and the operator has not
//!    passed `--i-understand-real-money`.
//!
//! 2. [`check_not_paused`] — reads the `safety_state` table from the operator's
//!    `xvn.db` and refuses to proceed when the global kill-switch is active.
//!    **Fail-closed for real money**: a missing or unopenable DB on mainnet is
//!    treated as an error (cannot verify the kill-switch). On testnet a missing
//!    DB issues a warning and proceeds (no real money at risk).
//!
//! The caller is responsible for reading `BYREAL_NETWORK` from the environment
//! and passing it in as `byreal_network` / `network_is_mainnet`. This keeps
//! the helpers free of env I/O so they can be tested without env manipulation.

use std::path::Path;

use anyhow::{bail, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use xvision_engine::safety::SafetyManager;

use super::venue::Venue;

// ---------------------------------------------------------------------------
// Safety migration — the one SQL block we need.
// ---------------------------------------------------------------------------

/// The SQL for migration 030. We re-embed it here rather than calling
/// `ApiContext::open` (which applies all 50+ migrations and is heavyweight for
/// a CLI pre-flight check). The table creation is idempotent because both
/// `CREATE TABLE IF NOT EXISTS` blocks guard against double-application.
const SAFETY_MIGRATION_SQL: &str =
    include_str!("../../../xvision-engine/migrations/030_safety_state_and_audit.sql");

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Open a minimal read-write SQLite pool on `xvn.db` inside `xvn_home`, apply
/// the safety migration (idempotent), and return the pool.
///
/// This intentionally does NOT run the full `ApiContext::open` migration stack —
/// only migration 030 is needed to read the pause state.
async fn open_safety_pool(xvn_home: &Path) -> anyhow::Result<SqlitePool> {
    let db_path = xvn_home.join("xvn.db");
    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(false) // we must NOT create the DB here; it must already exist
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5))
        .read_only(false); // SafetyManager bootstrap may write the initial row
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;

    // Apply the safety migration idempotently (CREATE TABLE IF NOT EXISTS).
    sqlx::query(SAFETY_MIGRATION_SQL).execute(&pool).await?;
    Ok(pool)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Return `Ok(())` if the venue / network combination is safe to proceed, or
/// `Err` if it is a real-money Byreal mainnet call and `ack` is false.
///
/// Decision table:
///
/// | venue  | byreal_network         | ack   | result |
/// |--------|------------------------|-------|--------|
/// | Byreal | None / "" / "mainnet"  | false | Err    |
/// | Byreal | None / "" / "mainnet"  | true  | Ok     |
/// | Byreal | contains "testnet"     | any   | Ok     |
/// | Alpaca | any                    | any   | Ok     |
/// | Orderly| any                    | any   | Ok     |
///
/// `byreal_network` is the value of `$BYREAL_NETWORK` (pass
/// `std::env::var("BYREAL_NETWORK").ok().as_deref()` at the call site).
pub fn require_real_money_ack(venue: Venue, byreal_network: Option<&str>, ack: bool) -> Result<()> {
    match venue {
        Venue::Alpaca | Venue::Orderly => Ok(()),
        Venue::Byreal => {
            // Fail-safe: anything that is not explicitly "testnet" is treated
            // as mainnet (None, empty string, or any unrecognised value).
            let is_testnet = byreal_network
                .map(|n| n.to_ascii_lowercase().contains("testnet"))
                .unwrap_or(false);

            if is_testnet {
                return Ok(());
            }

            // Mainnet path: require the explicit ack.
            if ack {
                return Ok(());
            }

            bail!(
                "BYREAL_NETWORK is mainnet — this command will move REAL funds. \
                 Re-run with --i-understand-real-money to proceed."
            );
        }
    }
}

/// Check the global kill-switch before submitting a manual trade.
///
/// Opens the SafetyManager from `xvn_home/xvn.db` (same DB as the dashboard
/// server) and reads the current pause state.
///
/// # Fail-closed policy
///
/// | DB reachable | paused | network_is_mainnet | result                        |
/// |-------------|--------|-------------------|-------------------------------|
/// | yes          | true   | any               | `Err` — system is paused      |
/// | yes          | false  | any               | `Ok`                          |
/// | no (missing) | —      | true              | `Err` — cannot verify         |
/// | no (missing) | —      | false             | `Ok` with warning             |
///
/// The fail-closed policy for mainnet prevents a missing DB from silently
/// bypassing the kill-switch on real-money venues. On testnet a missing DB is
/// treated as "no pause set" — the operator may not have run `xvn init` yet on
/// a fresh testnet box, and there is no real money at risk.
pub async fn check_not_paused(xvn_home: &Path, network_is_mainnet: bool) -> anyhow::Result<()> {
    match open_safety_pool(xvn_home).await {
        Err(open_err) => {
            if network_is_mainnet {
                bail!(
                    "could not open xvn.db at {} to verify the safety kill-switch \
                     (fail-closed on mainnet): {open_err}",
                    xvn_home.display()
                );
            } else {
                // Testnet: warn and proceed. No real money at risk; the operator
                // may not have run `xvn init` yet on a fresh testnet box.
                eprintln!(
                    "warning: xvn.db not found at {} — safety kill-switch not checked \
                     (testnet, proceeding): {open_err}",
                    xvn_home.display()
                );
                return Ok(());
            }
        }
        Ok(pool) => {
            let mgr = SafetyManager::new(pool);
            // bootstrap(false) seeds the state row if it doesn't exist yet.
            // We pass false (no live-venue detected here) so a fresh install
            // that has never had the dashboard running seeds paused=false and
            // does not block the operator.
            mgr.bootstrap(false).await?;
            if mgr.is_paused().await {
                let state = mgr.current().await;
                let by = state.paused_by.as_deref().unwrap_or("unknown");
                let reason = state.reason.as_deref().unwrap_or("no reason given");
                bail!(
                    "system is paused via the safety kill-switch (paused by {by}: {reason}); \
                     resume via the dashboard or `xvn safety resume` before trading"
                );
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests — written BEFORE implementation (TDD red → green).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. byreal + mainnet (None) + no ack ⇒ Err mentioning --i-understand-real-money
    #[test]
    fn byreal_mainnet_none_no_ack_is_err() {
        let err = require_real_money_ack(Venue::Byreal, None, false).unwrap_err();
        assert!(
            err.to_string().contains("--i-understand-real-money"),
            "error must mention --i-understand-real-money; got: {err}"
        );
    }

    // 2. byreal + explicit "mainnet" + no ack ⇒ Err
    #[test]
    fn byreal_explicit_mainnet_no_ack_is_err() {
        let err = require_real_money_ack(Venue::Byreal, Some("mainnet"), false).unwrap_err();
        assert!(
            err.to_string().contains("--i-understand-real-money"),
            "error must mention --i-understand-real-money; got: {err}"
        );
    }

    // 3. byreal + mainnet + ack ⇒ Ok
    #[test]
    fn byreal_mainnet_with_ack_is_ok() {
        require_real_money_ack(Venue::Byreal, None, true).expect("ack should be accepted");
        require_real_money_ack(Venue::Byreal, Some("mainnet"), true).expect("ack should be accepted");
    }

    // 4. byreal + testnet + no ack ⇒ Ok
    #[test]
    fn byreal_testnet_no_ack_is_ok() {
        require_real_money_ack(Venue::Byreal, Some("testnet"), false).expect("testnet must not require ack");
    }

    // 5. alpaca + no ack ⇒ Ok (paper trading, never real money)
    #[test]
    fn alpaca_no_ack_is_ok() {
        require_real_money_ack(Venue::Alpaca, None, false).expect("alpaca must not require ack");
    }

    // 6. orderly + no ack ⇒ Ok (testnet, never real money via this path)
    #[test]
    fn orderly_no_ack_is_ok() {
        require_real_money_ack(Venue::Orderly, None, false).expect("orderly must not require ack");
    }

    // 7. byreal + empty string (unset env) + no ack ⇒ Err (fail-safe mainnet)
    #[test]
    fn byreal_empty_network_no_ack_is_err() {
        let err = require_real_money_ack(Venue::Byreal, Some(""), false).unwrap_err();
        assert!(
            err.to_string().contains("--i-understand-real-money"),
            "empty BYREAL_NETWORK must be treated as mainnet; got: {err}"
        );
    }

    // 8. byreal + "TESTNET" (uppercase) + no ack ⇒ Ok (case-insensitive)
    #[test]
    fn byreal_testnet_uppercase_no_ack_is_ok() {
        require_real_money_ack(Venue::Byreal, Some("TESTNET"), false)
            .expect("uppercase testnet must be accepted");
    }

    // ---------------------------------------------------------------------------
    // check_not_paused tests — TDD (written first, then implementation added).
    // ---------------------------------------------------------------------------

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use tempfile::tempdir;
    use xvision_engine::safety::SafetyManager;

    /// Build an in-file-based SQLite pool with only the safety migration applied,
    /// at a temp path, for use in check_not_paused integration tests.
    async fn setup_test_db(dir: &std::path::Path) -> (std::path::PathBuf, SqlitePool) {
        let db_path = dir.join("xvn.db");
        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .expect("open test pool");
        sqlx::query(SAFETY_MIGRATION_SQL)
            .execute(&pool)
            .await
            .expect("apply safety migration");
        (db_path, pool)
    }

    // 9. check_not_paused: DB exists + unpaused ⇒ Ok (mainnet)
    #[tokio::test]
    async fn check_not_paused_unpaused_manager_is_ok() {
        let dir = tempdir().unwrap();
        let (_, pool) = setup_test_db(dir.path()).await;
        let mgr = SafetyManager::new(pool);
        mgr.bootstrap(false).await.unwrap();
        // Pool is now committed; check_not_paused must read the same file.
        let result = check_not_paused(dir.path(), true).await;
        assert!(
            result.is_ok(),
            "unpaused manager must allow trade; got: {result:?}"
        );
    }

    // 10. check_not_paused: DB exists + PAUSED ⇒ Err (mainnet)
    #[tokio::test]
    async fn check_not_paused_paused_manager_is_err() {
        use xvision_engine::safety::AuthContext;

        let dir = tempdir().unwrap();
        let (_, pool) = setup_test_db(dir.path()).await;
        let mgr = SafetyManager::new(pool);
        mgr.bootstrap(false).await.unwrap();
        let auth = AuthContext::api_anonymous();
        mgr.pause(Some("test pause".into()), &auth).await.unwrap();

        let result = check_not_paused(dir.path(), true).await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("paused via the safety kill-switch"),
            "error must mention kill-switch; got: {err}"
        );
    }

    // 11. check_not_paused: missing DB + mainnet ⇒ Err (fail-closed)
    #[tokio::test]
    async fn check_not_paused_missing_db_mainnet_is_err() {
        let dir = tempdir().unwrap();
        // No DB created — xvn.db does not exist.
        let result = check_not_paused(dir.path(), true).await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("fail-closed on mainnet"),
            "error must mention fail-closed; got: {err}"
        );
    }

    // 12. check_not_paused: missing DB + testnet ⇒ Ok (warn-and-proceed)
    #[tokio::test]
    async fn check_not_paused_missing_db_testnet_is_ok() {
        let dir = tempdir().unwrap();
        // No DB created.
        let result = check_not_paused(dir.path(), false).await;
        assert!(
            result.is_ok(),
            "missing DB on testnet must warn-and-proceed; got: {result:?}"
        );
    }
}
