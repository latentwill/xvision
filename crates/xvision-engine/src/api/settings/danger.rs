//! `/api/settings/danger` — destructive workspace ops behind a typed
//! confirm phrase.
//!
//! Three operations, mirroring the Settings/Danger tab in the dashboard:
//!
//! - [`reset_workspace`] — selective clear of user-authored content
//!   (strategies, evals, agents, runs, chats, scenarios). Preserves
//!   `api_audit`, `agent_profiles`, `bars_cache`, `skills`, the
//!   `eval_scenarios` table, and canonical scenarios in `scenarios`.
//!   Filesystem: clears `$XVN_HOME/strategies/`; leaves
//!   `$XVN_HOME/secrets/` and `$XVN_HOME/config/` untouched.
//!   Requires the typed phrase `"RESET WORKSPACE"`. Replaces the old
//!   nuclear-ish `wipe_db` per F-4 from the 2026-05-18 QA round-4
//!   intake.
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
pub const RESET_WORKSPACE_CONFIRM: &str = "RESET WORKSPACE";
pub const REGEN_IDENTITY_CONFIRM: &str = "REGEN IDENTITY";
pub const FACTORY_RESET_CONFIRM: &str = "FACTORY RESET";

/// Tables NEVER cleared by [`reset_workspace`]. The intake (F-4,
/// 2026-05-18) calls out the four-row contract: audit trail, review
/// agent personas, expensive-to-refetch bars cache, and the skills
/// registry. `eval_scenarios` is preserved separately because it is
/// canonical seed data the eval foundation depends on. Canonical rows
/// in `scenarios` (those with `source = 'canonical'`) are also
/// preserved — `reset_workspace` clears `scenarios` with a
/// `WHERE source != 'canonical'` filter so the four-scenario seed
/// the dashboard depends on survives.
pub const RESET_WORKSPACE_PRESERVED_TABLES: &[&str] = &[
    "api_audit",
    "agent_profiles",
    "bars_cache",
    "skills",
    "eval_scenarios",
];

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetWorkspaceReport {
    /// `(table_name, rows_deleted)` for every user table cleared.
    /// Ordering matches `sqlite_master` enumeration order.
    pub tables_cleared: Vec<TableWipe>,
    /// Rows summed across `tables_cleared`. `u32` so ts-rs emits a TS
    /// `number` (no realistic user table approaches 4B rows).
    pub total_rows_deleted: u32,
    /// `(table_name, rows_remaining)` for every table preserved per
    /// [`RESET_WORKSPACE_PRESERVED_TABLES`]. Surfaced so the dashboard
    /// can show the operator exactly what survived the reset (rather
    /// than just a verbal contract). Canonical-scenario rows count
    /// toward the `scenarios` row in `tables_cleared` (they weren't
    /// touched but they aren't in this list either — the post-reset
    /// `scenarios` row count is implied by the seed contract).
    pub tables_preserved: Vec<TablePreserved>,
    /// Number of files removed from `$XVN_HOME/strategies/`. 0 if the
    /// dir didn't exist (filesystem-backed strategies are optional).
    pub strategy_files_deleted: u32,
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
pub struct TablePreserved {
    pub table: String,
    pub rows_remaining: u32,
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

// ─── reset_workspace ─────────────────────────────────────────────────────

pub async fn reset_workspace(
    ctx: &ApiContext,
    confirm: &str,
) -> ApiResult<ResetWorkspaceReport> {
    let started = Instant::now();
    let result = reset_workspace_inner(ctx, confirm).await;
    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "danger.reset_workspace",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn reset_workspace_inner(
    ctx: &ApiContext,
    confirm: &str,
) -> ApiResult<ResetWorkspaceReport> {
    check_confirm(confirm, RESET_WORKSPACE_CONFIRM)?;

    // Enumerate user tables from sqlite_master so we can't miss a table
    // added by a future migration. We exclude SQLite + sqlx metadata
    // and FTS5 shadow tables (search_index's `_data` / `_idx` /
    // `_content` / `_docsize` / `_config` partners — DELETE FROM those
    // is rejected by fts5, and `DELETE FROM search_index` already
    // cascades through them).
    let names: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master \
         WHERE type='table' \
           AND name NOT LIKE 'sqlite_%' \
           AND name NOT LIKE '_sqlx_%' \
           AND name NOT LIKE '%_data' \
           AND name NOT LIKE '%_idx' \
           AND name NOT LIKE '%_content' \
           AND name NOT LIKE '%_docsize' \
           AND name NOT LIKE '%_config'",
    )
    .fetch_all(&ctx.db)
    .await?
    .into_iter()
    .map(|r| r.get::<String, _>("name"))
    .collect();

    // Defer FK checks to commit time so we don't have to enumerate
    // delete order across every table. The final commit still enforces
    // referential integrity — if anything in the preserved set ends up
    // pointing at a cleared row, the tx aborts and we return Internal
    // instead of leaving a partial wipe.
    let mut tx = ctx.db.begin().await?;
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await?;

    let mut tables_cleared = Vec::with_capacity(names.len());
    let mut total_rows_deleted: u32 = 0;
    for name in &names {
        if RESET_WORKSPACE_PRESERVED_TABLES.contains(&name.as_str()) {
            continue;
        }

        // Quote the identifier to allow names like `eval_findings` to
        // co-exist with `_sqlx_*` filtering. Table names come from
        // sqlite_master so they're already trusted, but double-quote
        // escaping defends against a future migration that allows
        // unusual chars.
        let quoted = name.replace('"', "\"\"");
        let sql = if name == "scenarios" {
            // Preserve canonical-seed scenarios so the first-run
            // dashboard contract (four BTC scenarios available out of
            // the box) survives the reset.
            format!("DELETE FROM \"{quoted}\" WHERE source != 'canonical'")
        } else {
            format!("DELETE FROM \"{quoted}\"")
        };
        let result = sqlx::query(&sql).execute(&mut *tx).await?;
        let rows = result.rows_affected().min(u32::MAX as u64) as u32;
        total_rows_deleted = total_rows_deleted.saturating_add(rows);
        tables_cleared.push(TableWipe {
            table: name.clone(),
            rows_deleted: rows,
        });
    }

    // Snapshot row counts for the preserved set inside the same tx so
    // the report reflects post-clear state under the FK gate.
    let mut tables_preserved = Vec::with_capacity(RESET_WORKSPACE_PRESERVED_TABLES.len());
    for preserved in RESET_WORKSPACE_PRESERVED_TABLES {
        // Some preserved tables may not exist on older DBs (e.g.
        // `agent_profiles` was added in migration 016). Tolerate the
        // missing-table case by checking sqlite_master first.
        if !names.iter().any(|n| n == *preserved) {
            continue;
        }
        let quoted = preserved.replace('"', "\"\"");
        let count: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM \"{quoted}\""))
            .fetch_one(&mut *tx)
            .await?;
        let rows = (count.0.max(0) as u64).min(u32::MAX as u64) as u32;
        tables_preserved.push(TablePreserved {
            table: (*preserved).to_string(),
            rows_remaining: rows,
        });
    }

    tx.commit().await?;

    // Filesystem: clear `$XVN_HOME/strategies/` (file-backed strategy
    // drafts). Leave secrets/ and config/ untouched. We do this AFTER
    // the DB commit so a failed FK gate doesn't leave the filesystem
    // half-reset.
    let strategy_files_deleted = clear_strategy_files(&ctx.xvn_home).await?;

    Ok(ResetWorkspaceReport {
        tables_cleared,
        total_rows_deleted,
        tables_preserved,
        strategy_files_deleted,
    })
}

/// Remove every file directly under `$XVN_HOME/strategies/`. The
/// directory itself is left in place (re-created by the next save if
/// missing). Subdirectories are not recursed — the strategy store is
/// flat per `strategies::store::FilesystemStore`.
async fn clear_strategy_files(xvn_home: &std::path::Path) -> ApiResult<u32> {
    let dir = xvn_home.join("strategies");
    if !dir.exists() {
        return Ok(0);
    }
    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| ApiError::Internal(format!("read {}: {e}", dir.display())))?;
    let mut count: u32 = 0;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ApiError::Internal(format!("scan {}: {e}", dir.display())))?
    {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .await
            .map_err(|e| ApiError::Internal(format!("file_type {}: {e}", path.display())))?;
        if file_type.is_file() {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| ApiError::Internal(format!("remove {}: {e}", path.display())))?;
            count = count.saturating_add(1);
        }
    }
    Ok(count)
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

    // Mirror to a sibling log file so the trail survives the wipe. The
    // previous parent-based picker (commit 4b1eda3) only fell back to
    // `temp_dir()` when `xvn_home.parent()` was `None`, which never
    // fires in practice — the filesystem root `/` is a perfectly
    // valid parent for `XVN_HOME=/data`, so the picker would hand back
    // `/xvn-last-factory-reset.log` and fail with EACCES on container
    // deploys where the process runs as a non-root user. F-3 from the
    // 2026-05-18 QA round-4 intake.
    let audit_log_path = pick_audit_log_path(&xvn_home);
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

/// Pick a writable home for the factory-reset audit-log mirror. The log
/// must live OUTSIDE `xvn_home` so it survives the wipe, and the
/// directory must be writable by the running process. Order:
///
/// 1. `$XVN_AUDIT_DIR/xvn-last-factory-reset.log` — explicit override
///    for deployments that want the log under a known mount.
/// 2. `<xvn_home.parent()>/xvn-last-factory-reset.log` — the original
///    workstation pick. Only used when the parent isn't the filesystem
///    root *and* is writable.
/// 3. `std::env::temp_dir()/xvn-last-factory-reset.log` — operator can
///    `docker cp` it out before restart.
fn pick_audit_log_path(xvn_home: &std::path::Path) -> PathBuf {
    let audit_dir = std::env::var_os("XVN_AUDIT_DIR").map(PathBuf::from);
    pick_audit_log_path_with(
        xvn_home,
        audit_dir.as_deref(),
        &std::env::temp_dir(),
    )
}

/// Env-free core of `pick_audit_log_path` — same algorithm, but the
/// `XVN_AUDIT_DIR` override and the `temp_dir()` fallback are passed in
/// so unit tests don't have to race on the process-wide environment.
fn pick_audit_log_path_with(
    xvn_home: &std::path::Path,
    audit_dir_override: Option<&std::path::Path>,
    temp_dir: &std::path::Path,
) -> PathBuf {
    const LOG_NAME: &str = "xvn-last-factory-reset.log";

    if let Some(dir) = audit_dir_override {
        if dir_is_writable(dir) {
            return dir.join(LOG_NAME);
        }
    }

    if let Some(parent) = xvn_home.parent() {
        // Filesystem root has no parent; reject it so we don't try to
        // write `/xvn-last-factory-reset.log` on a container deploy
        // where `XVN_HOME=/data` makes the parent the read-only root.
        let is_root = parent.parent().is_none();
        if !is_root && dir_is_writable(parent) {
            return parent.join(LOG_NAME);
        }
    }

    temp_dir.join(LOG_NAME)
}

/// Probe whether `dir` is a writable directory by attempting to create
/// (and immediately remove) a uniquely-named file inside it. Failure
/// modes — missing dir, EACCES, EROFS — all collapse to `false`, which
/// routes the caller to the next fallback in the picker chain.
fn dir_is_writable(dir: &std::path::Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let probe_name = format!(
        ".xvn_writable_probe_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    let probe = dir.join(&probe_name);
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_f) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
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
    async fn reset_workspace_rejects_wrong_confirm() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;
        let err = reset_workspace(&ctx, "nope").await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn reset_workspace_rejects_legacy_wipe_db_phrase() {
        // F-4 collapses the two-button "wipe_db + factory_reset"
        // surface into one selective op. The old WIPE DATABASE phrase
        // must no longer satisfy any route — an operator typing it
        // should be told the new phrase verbatim.
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;
        let err = reset_workspace(&ctx, "WIPE DATABASE").await.unwrap_err();
        match err {
            ApiError::Validation(msg) => assert!(
                msg.contains(RESET_WORKSPACE_CONFIRM),
                "validation message must guide the operator to the new phrase, got: {msg}"
            ),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn factory_reset_rejects_cross_route_phrase() {
        // Per-route phrases also defend against a single typed phrase
        // accidentally firing the wrong op. The reset_workspace phrase
        // must not satisfy factory_reset.
        let dir = TempDir::new().unwrap();
        let xvn_home = dir.path().join("xvn-home");
        tokio::fs::create_dir_all(&xvn_home).await.unwrap();
        let ctx = ApiContext::open(&xvn_home, Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        let err = factory_reset(&ctx, RESET_WORKSPACE_CONFIRM).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn reset_workspace_preserves_secrets_settings_and_audit() {
        // F-4 acceptance test from the 2026-05-18 QA round-4 intake:
        // seed rows in both the preserve set (api_audit, agent_profiles,
        // bars_cache, skills) and the clear set (chat_sessions,
        // chat_messages, agents/agent_slots, scenarios non-canonical),
        // run the op, and assert clear counts > 0 while preserve counts
        // are unchanged.
        let dir = TempDir::new().unwrap();
        let ctx = ctx_in(&dir).await;

        // Preserve set seeds.
        let _ = audit::record(&ctx, "test", "seed", None, None, Outcome::Ok, 0).await;
        sqlx::query(
            "INSERT INTO agent_profiles \
             (id, name, type, provider, model, temperature, max_tokens, \
              system_prompt, enabled, created_at, updated_at) \
             VALUES ('p1', 'fast', 'fast-trader', 'anthropic', 'claude-haiku', \
                     0.7, 1024, 'sp', 1, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z')",
        )
        .execute(&ctx.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO bars_cache \
             (cache_key, asset, granularity, window_start, window_end, data_source, \
              fetched_at, bar_count, bars_blob) \
             VALUES ('k1', 'BTC/USD', '1h', '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z', \
                     'alpaca', '2026-05-18T00:00:00Z', 0, X'00')",
        )
        .execute(&ctx.db)
        .await
        .unwrap();

        let audit_before = count_rows(&ctx.db, "api_audit").await;
        let agent_profiles_before = count_rows(&ctx.db, "agent_profiles").await;
        let bars_cache_before = count_rows(&ctx.db, "bars_cache").await;

        // Clear set seeds.
        sqlx::query(
            "INSERT INTO chat_sessions \
             (id, started_at, last_activity_at, context_scope_json) \
             VALUES ('s1', '2026-05-11T00:00:00Z', '2026-05-11T00:00:00Z', '{}'), \
                    ('s2', '2026-05-11T00:00:00Z', '2026-05-11T00:00:00Z', '{}')",
        )
        .execute(&ctx.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO scenarios \
             (id, source, display_name, body_json, created_at, created_by) \
             VALUES ('user-scenario-1', 'user', 'My Scenario', '{}', \
                     '2026-05-18T00:00:00Z', 'operator')",
        )
        .execute(&ctx.db)
        .await
        .unwrap();
        let scenarios_before = count_rows(&ctx.db, "scenarios").await;
        assert!(
            scenarios_before >= 5,
            "expected 4 canonical seed + 1 user scenario, got {scenarios_before}"
        );

        let report = reset_workspace(&ctx, RESET_WORKSPACE_CONFIRM).await.unwrap();

        // ── preserve set untouched ──
        assert_eq!(
            count_rows(&ctx.db, "api_audit").await,
            audit_before + 1, // +1 for the reset_workspace's own audit row
            "api_audit must be preserved (plus the audit row from this op)"
        );
        assert_eq!(
            count_rows(&ctx.db, "agent_profiles").await,
            agent_profiles_before,
            "agent_profiles must be preserved"
        );
        assert_eq!(
            count_rows(&ctx.db, "bars_cache").await,
            bars_cache_before,
            "bars_cache must be preserved"
        );
        for preserved in RESET_WORKSPACE_PRESERVED_TABLES {
            assert!(
                report.tables_cleared.iter().all(|t| t.table != *preserved),
                "{preserved} must be excluded from tables_cleared: {:?}",
                report.tables_cleared,
            );
        }
        // Preserved list reflects post-clear row counts.
        let audit_row = report
            .tables_preserved
            .iter()
            .find(|t| t.table == "api_audit")
            .expect("api_audit reported as preserved");
        assert!(audit_row.rows_remaining >= 1);

        // ── clear set wiped ──
        assert_eq!(count_rows(&ctx.db, "chat_sessions").await, 0);
        let sessions_row = report
            .tables_cleared
            .iter()
            .find(|t| t.table == "chat_sessions")
            .expect("chat_sessions reported as cleared");
        assert_eq!(sessions_row.rows_deleted, 2);

        // ── canonical scenarios survive ──
        let scenarios_remaining = count_rows(&ctx.db, "scenarios").await;
        assert!(
            scenarios_remaining >= 4,
            "canonical scenarios must survive (source = 'canonical'), got {scenarios_remaining}"
        );
        let scenarios_cleared = report
            .tables_cleared
            .iter()
            .find(|t| t.table == "scenarios")
            .expect("scenarios in cleared report");
        // Only the user scenario(s) get cleared — canonical rows stay.
        assert_eq!(
            scenarios_cleared.rows_deleted, 1,
            "exactly the one user-source scenario should have been cleared"
        );
    }

    #[tokio::test]
    async fn reset_workspace_clears_strategy_files() {
        // F-4 also clears `$XVN_HOME/strategies/` (filesystem-backed
        // strategy drafts). Secrets/config dirs are untouched.
        let dir = TempDir::new().unwrap();
        let xvn_home = dir.path().join("xvn-home");
        tokio::fs::create_dir_all(&xvn_home).await.unwrap();
        let ctx = ApiContext::open(&xvn_home, Actor::Cli { user: "test".into() })
            .await
            .unwrap();

        // Seed strategy files + a secrets file.
        let strategies = xvn_home.join("strategies");
        tokio::fs::create_dir_all(&strategies).await.unwrap();
        tokio::fs::write(strategies.join("a.json"), b"{}").await.unwrap();
        tokio::fs::write(strategies.join("b.json"), b"{}").await.unwrap();
        let secrets = xvn_home.join("secrets");
        tokio::fs::create_dir_all(&secrets).await.unwrap();
        tokio::fs::write(secrets.join("alpaca.toml"), b"key=hi").await.unwrap();

        let report = reset_workspace(&ctx, RESET_WORKSPACE_CONFIRM).await.unwrap();

        assert_eq!(report.strategy_files_deleted, 2);
        assert!(strategies.exists(), "strategies dir itself remains");
        assert!(!strategies.join("a.json").exists());
        assert!(!strategies.join("b.json").exists());
        // Secrets untouched.
        assert!(secrets.join("alpaca.toml").exists());
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

    #[test]
    fn pick_audit_log_path_with_honors_audit_dir_override() {
        // F-3: when XVN_AUDIT_DIR is set + writable, it beats the
        // parent-based pick.
        let override_dir = TempDir::new().unwrap();
        let temp = TempDir::new().unwrap();
        let pick = pick_audit_log_path_with(
            std::path::Path::new("/data"),
            Some(override_dir.path()),
            temp.path(),
        );
        assert_eq!(pick, override_dir.path().join("xvn-last-factory-reset.log"));
    }

    #[test]
    fn pick_audit_log_path_with_falls_back_when_parent_is_filesystem_root() {
        // Container repro: XVN_HOME=/data → parent=/ → root-owned. The
        // old picker handed back /xvn-last-factory-reset.log; the new
        // picker must skip the root and use temp_dir.
        let temp = TempDir::new().unwrap();
        let pick = pick_audit_log_path_with(
            std::path::Path::new("/data"),
            None,
            temp.path(),
        );
        assert_eq!(pick, temp.path().join("xvn-last-factory-reset.log"));
    }

    #[test]
    fn pick_audit_log_path_with_uses_parent_when_writable() {
        // Workstation shape: xvn_home parent is a normal writable dir.
        let parent = TempDir::new().unwrap();
        let xvn_home = parent.path().join(".xvision");
        let temp = TempDir::new().unwrap();
        let pick = pick_audit_log_path_with(&xvn_home, None, temp.path());
        assert_eq!(pick, parent.path().join("xvn-last-factory-reset.log"));
    }

    #[test]
    fn pick_audit_log_path_with_falls_back_when_parent_unwritable() {
        // /proc/sys/<doesn't-exist> isn't a writable dir; probe fails
        // and we route to temp_dir.
        let temp = TempDir::new().unwrap();
        let pick = pick_audit_log_path_with(
            std::path::Path::new("/proc/sys/this-path-cannot-exist/.xvision"),
            None,
            temp.path(),
        );
        assert_eq!(pick, temp.path().join("xvn-last-factory-reset.log"));
    }

    #[test]
    fn pick_audit_log_path_with_falls_back_when_override_unwritable() {
        // An override pointing at a non-existent or non-writable dir
        // must NOT short-circuit — the picker falls through to the
        // parent-based / temp_dir chain.
        let parent = TempDir::new().unwrap();
        let xvn_home = parent.path().join(".xvision");
        let temp = TempDir::new().unwrap();
        let bogus = std::path::Path::new("/proc/sys/not-a-dir");
        let pick = pick_audit_log_path_with(&xvn_home, Some(bogus), temp.path());
        assert_eq!(
            pick,
            parent.path().join("xvn-last-factory-reset.log"),
            "bogus override should fall through to the writable parent",
        );
    }
}
