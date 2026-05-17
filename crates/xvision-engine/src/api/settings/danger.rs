//! `/api/settings/danger` — destructive workspace ops behind a typed
//! confirm phrase.
//!
//! Three operations, mirroring the Settings/Danger tab in the dashboard:
//!
//! - [`wipe_db`] — `DELETE FROM` every user table in `xvn.db` except
//!   `api_audit`. Requires the typed phrase `"WIPE DATABASE"`.
//! - [`regen_identity`] — overwrite the on-disk Ed25519 signing key.
//!   v1 ships without the `xvision-identity` feature; this op returns
//!   a `Conflict` until the wallet plan ships. Requires
//!   `"REGEN IDENTITY"`.
//! - [`factory_reset`] — delete and recreate `$XVN_HOME`. Audit row is
//!   mirrored to a sibling log file outside the wiped dir. Requires
//!   `"FACTORY RESET"`.
//!
//! Per QA 2026-05-17 finding #4 (`qa-dashboard-auth-hardening`): the
//! prior single `CONFIRM_TOKEN = "yes-i-am-sure"` was a static constant
//! shipped in the frontend bundle, so it added no real intent gate. Now
//! each op has a distinct phrase that the operator must type verbatim;
//! the typed text is what travels on the wire and is what the engine
//! checks. The frontend may render the expected phrase to operators
//! for discoverability — it just can't auto-fill it on submit.

use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tokio::task;

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiError, ApiResult,
};

/// Per-route confirm phrases (qa-dashboard-auth-hardening, 2026-05-17).
/// Distinct phrases per op so an operator's typed string demonstrates
/// intent specifically for THAT op — not a generic "yes-i-am-sure"
/// that could fall through from one route to another.
pub const WIPE_DB_CONFIRM: &str = "WIPE DATABASE";
pub const REGEN_IDENTITY_CONFIRM: &str = "REGEN IDENTITY";
pub const FACTORY_RESET_CONFIRM: &str = "FACTORY RESET";

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WipeDbReport {
    /// `(table_name, rows_deleted)` for every user table touched.
    /// Ordering matches `sqlite_master` enumeration order.
    pub tables: Vec<TableWipe>,
    /// Rows summed across `tables`. Useful for the UI's confirmation
    /// toast without re-summing.
    /// Rows summed across `tables`. `u32` rather than `u64` so ts-rs
    /// emits a TS `number` instead of `bigint` (4B rows is plenty for
    /// any user table).
    pub total_rows_deleted: u32,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableWipe {
    pub table: String,
    pub rows_deleted: u32,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegenIdentityReport {
    /// New public key in hex-encoded form. Operator copies this if they
    /// need to publish it elsewhere.
    pub pubkey_hex: String,
    /// Filesystem path the new key was written to.
    pub key_path: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactoryResetReport {
    pub xvn_home: String,
    /// Path of the audit log we mirrored to so the operator can find it
    /// after the wipe.
    pub audit_log_path: String,
}

// ─── wipe_db ──────────────────────────────────────────────────────────────

pub async fn wipe_db(ctx: &ApiContext, confirm: &str) -> ApiResult<WipeDbReport> {
    let started = Instant::now();
    let result = wipe_db_inner(ctx, confirm).await;
    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "danger.wipe_db",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn wipe_db_inner(ctx: &ApiContext, confirm: &str) -> ApiResult<WipeDbReport> {
    check_confirm(confirm, WIPE_DB_CONFIRM)?;

    let names: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master \
         WHERE type='table' \
           AND name NOT LIKE 'sqlite_%' \
           AND name NOT LIKE '_sqlx_%' \
           AND name != 'api_audit'",
    )
    .fetch_all(&ctx.db)
    .await?
    .into_iter()
    .map(|r| r.get::<String, _>("name"))
    .collect();

    let mut tables = Vec::with_capacity(names.len());
    let mut total_rows_deleted: u32 = 0;
    for name in &names {
        // We can't bind the table name as a parameter; SQLite parameters
        // are values, not identifiers. The names come from sqlite_master
        // (already trusted) and we only got here past the confirm string,
        // so quoting via `"name"` is safe enough.
        let sql = format!("DELETE FROM \"{}\"", name.replace('"', "\"\""));
        let result = sqlx::query(&sql).execute(&ctx.db).await?;
        // sqlx returns u64; clamp to u32 since the wire shape promises
        // `number` (no realistic user table approaches 4B rows).
        let rows = result.rows_affected().min(u32::MAX as u64) as u32;
        total_rows_deleted = total_rows_deleted.saturating_add(rows);
        tables.push(TableWipe {
            table: name.clone(),
            rows_deleted: rows,
        });
    }

    Ok(WipeDbReport {
        tables,
        total_rows_deleted,
    })
}

// ─── regen_identity ──────────────────────────────────────────────────────

pub async fn regen_identity(ctx: &ApiContext, confirm: &str) -> ApiResult<RegenIdentityReport> {
    let started = Instant::now();
    let result = regen_identity_inner(ctx, confirm).await;
    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "danger.regen_identity",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn regen_identity_inner(_ctx: &ApiContext, confirm: &str) -> ApiResult<RegenIdentityReport> {
    check_confirm(confirm, REGEN_IDENTITY_CONFIRM)?;

    // v1 ships without the `xvision-identity` member. Regen is intentionally
    // refused; the audit row still records the intent. The wallet plan
    // replaces this branch with the real keygen.
    Err(ApiError::Conflict(
        "regen_identity is gated behind the xvision-identity feature, \
         which is not compiled into this build. See the wallet plan."
            .into(),
    ))
}

// ─── factory_reset ───────────────────────────────────────────────────────

pub async fn factory_reset(ctx: &ApiContext, confirm: &str) -> ApiResult<FactoryResetReport> {
    let started = Instant::now();
    // For factory_reset the audit trail can't survive in `xvn.db` (we're
    // about to delete the file). We record to the in-DB audit row FIRST
    // for the optimistic case, AND we mirror to a sibling log file
    // outside `xvn_home` so the trail outlives the wipe.
    let _ = audit::record(
        ctx,
        "settings",
        "danger.factory_reset",
        None,
        None,
        Outcome::Ok,
        0,
    )
    .await;

    let result = factory_reset_inner(ctx, confirm).await;
    // Best-effort: a second audit row may not land because the DB is
    // already gone, but try anyway in case the reset failed early.
    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "danger.factory_reset.finalize",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn factory_reset_inner(ctx: &ApiContext, confirm: &str) -> ApiResult<FactoryResetReport> {
    check_confirm(confirm, FACTORY_RESET_CONFIRM)?;
    let xvn_home = ctx.xvn_home.clone();

    // Mirror to a sibling log file so the trail survives the wipe.
    let audit_log_path: PathBuf = match xvn_home.parent() {
        Some(parent) => parent.join("xvn-last-factory-reset.log"),
        // `xvn_home` is the filesystem root — shouldn't happen, but fall
        // back to a tempfile so we still get a record somewhere.
        None => std::env::temp_dir().join("xvn-last-factory-reset.log"),
    };
    let log_line = format!(
        "{ts} factory_reset xvn_home={path}\n",
        ts = chrono::Utc::now().to_rfc3339(),
        path = xvn_home.display(),
    );
    let log_path_for_blocking = audit_log_path.clone();
    let line_for_blocking = log_line.clone();
    task::spawn_blocking(move || -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path_for_blocking)?;
        f.write_all(line_for_blocking.as_bytes())?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))?
    .map_err(|e| ApiError::Internal(format!("write audit log: {e}")))?;

    // The actual wipe. `remove_dir_all` is idempotent enough here —
    // if the path is missing we treat it as already-clean.
    if xvn_home.exists() {
        tokio::fs::remove_dir_all(&xvn_home)
            .await
            .map_err(|e| ApiError::Internal(format!("remove {}: {e}", xvn_home.display())))?;
    }
    tokio::fs::create_dir_all(&xvn_home)
        .await
        .map_err(|e| ApiError::Internal(format!("recreate {}: {e}", xvn_home.display())))?;

    Ok(FactoryResetReport {
        xvn_home: xvn_home.display().to_string(),
        audit_log_path: audit_log_path.display().to_string(),
    })
}

// ─── helpers ─────────────────────────────────────────────────────────────

fn check_confirm(confirm: &str, expected: &str) -> ApiResult<()> {
    if confirm != expected {
        return Err(ApiError::Validation(format!(
            "confirm must be the literal string \"{expected}\""
        )));
    }
    Ok(())
}

fn audit_outcome<T>(result: &ApiResult<T>) -> Outcome {
    match result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    async fn ctx_in(dir: &TempDir) -> ApiContext {
        ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
            .await
            .unwrap()
    }

    async fn count_rows(pool: &SqlitePool, table: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) AS n FROM \"{table}\"");
        sqlx::query(&sql)
            .fetch_one(pool)
            .await
            .map(|r| r.get::<i64, _>("n"))
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn wipe_db_rejects_wrong_confirm() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;
        let err = wipe_db(&ctx, "nope").await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn wipe_db_rejects_legacy_yes_i_am_sure() {
        // qa-dashboard-auth-hardening (2026-05-17): the prior single
        // `yes-i-am-sure` confirm string is intentionally no longer
        // accepted. An operator typing it must now get a Validation
        // error pointing at the per-route phrase.
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;
        let err = wipe_db(&ctx, "yes-i-am-sure").await.unwrap_err();
        match err {
            ApiError::Validation(msg) => assert!(
                msg.contains(WIPE_DB_CONFIRM),
                "validation message must guide the operator to the new phrase, got: {msg}"
            ),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn factory_reset_rejects_cross_route_phrase() {
        // Per-route phrases also defend against a single typed phrase
        // accidentally firing the wrong op. The WIPE_DB phrase must
        // not satisfy factory_reset.
        let dir = TempDir::new().unwrap();
        let xvn_home = dir.path().join("xvn-home");
        tokio::fs::create_dir_all(&xvn_home).await.unwrap();
        let ctx = ApiContext::open(&xvn_home, Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        let err = factory_reset(&ctx, WIPE_DB_CONFIRM).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn wipe_db_clears_user_tables_and_preserves_audit() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;
        // Seed two rows in chat_sessions (a real table from migration 003).
        sqlx::query(
            "INSERT INTO chat_sessions \
             (id, started_at, last_activity_at, context_scope_json) \
             VALUES ('s1', '2026-05-11T00:00:00Z', '2026-05-11T00:00:00Z', '{}'), \
                    ('s2', '2026-05-11T00:00:00Z', '2026-05-11T00:00:00Z', '{}')",
        )
        .execute(&ctx.db)
        .await
        .unwrap();
        assert_eq!(count_rows(&ctx.db, "chat_sessions").await, 2);

        // Also fire an audit row so the post-wipe count is verifiable.
        let _ = audit::record(&ctx, "test", "seed", None, None, Outcome::Ok, 0).await;

        let report = wipe_db(&ctx, WIPE_DB_CONFIRM).await.unwrap();

        assert_eq!(count_rows(&ctx.db, "chat_sessions").await, 0);
        // api_audit must NOT be in the report and must still have rows
        // (the seed row + the wipe's own audit row).
        assert!(
            report.tables.iter().all(|t| t.table != "api_audit"),
            "api_audit must be excluded from wipe: {:?}",
            report.tables
        );
        assert!(
            count_rows(&ctx.db, "api_audit").await >= 2,
            "audit trail preserved"
        );
        // The chat_sessions DELETE recorded its row count.
        let sessions_row = report
            .tables
            .iter()
            .find(|t| t.table == "chat_sessions")
            .expect("chat_sessions reported");
        assert_eq!(sessions_row.rows_deleted, 2);
        assert!(report.total_rows_deleted >= 2);
    }

    #[tokio::test]
    async fn regen_identity_returns_conflict_in_v1() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;
        let err = regen_identity(&ctx, REGEN_IDENTITY_CONFIRM).await.unwrap_err();
        match err {
            ApiError::Conflict(msg) => {
                assert!(
                    msg.contains("xvision-identity"),
                    "expected feature-gate message, got: {msg}"
                );
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn regen_identity_rejects_wrong_confirm_before_feature_check() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;
        let err = regen_identity(&ctx, "").await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn factory_reset_rejects_wrong_confirm() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;
        let err = factory_reset(&ctx, "").await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn factory_reset_clears_xvn_home_and_writes_sibling_log() {
        let dir = TempDir::new().unwrap();
        // Put a marker file under xvn_home so we can confirm it disappeared.
        let xvn_home = dir.path().join("xvn-home");
        tokio::fs::create_dir_all(&xvn_home).await.unwrap();
        tokio::fs::write(xvn_home.join("marker"), b"hi").await.unwrap();

        let ctx = ApiContext::open(&xvn_home, Actor::Cli { user: "test".into() })
            .await
            .unwrap();

        let report = factory_reset(&ctx, FACTORY_RESET_CONFIRM).await.unwrap();

        // Marker gone, dir re-created empty (ApiContext::open didn't
        // re-run since we don't re-open after the reset — the dir is
        // empty but exists).
        assert!(xvn_home.exists(), "xvn_home re-created at {}", xvn_home.display());
        assert!(!xvn_home.join("marker").exists(), "marker should have been wiped");

        // Sibling log got our line.
        let log_path = PathBuf::from(&report.audit_log_path);
        assert!(log_path.exists(), "sibling log written");
        let contents = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert!(contents.contains("factory_reset"), "log line written");
        assert!(contents.contains(&xvn_home.display().to_string()));
    }
}
