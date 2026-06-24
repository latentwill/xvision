//! Eval run watchdog — finalizes stuck `running` rows in `eval_runs`.
//!
//! Two entry points:
//!  - [`boot_sweep`] — one-shot scan at engine startup. Finalizes any
//!    `running` rows older than the configured threshold. The
//!    `xvision-dashboard` startup path also calls `fail_orphan_runs` which
//!    blanket-fails *all* leftover queued/running rows on restart; the
//!    boot sweep here is the narrower "started-but-stuck" sibling and is
//!    safe to run alongside (idempotent — already-terminal rows are
//!    skipped by the underlying `fail_active`).
//!  - [`spawn`] — long-running background task driven by
//!    `tokio::time::interval`. Catches runs that go silent *while the
//!    daemon is up* (the prototype case: `01KS0A5DP8KZVQJ03TCKGKYJVN`,
//!    started 14:27:45Z with no progress and no `completed_at`).
//!
//! Both paths reuse [`RunStore::fail_active`] — the same finalize code
//! path the provider-error executor branch uses today. That means the
//! row transitions to `status='failed'`, `error=<reason>`,
//! `completed_at=now()` with the same atomic guard against double-write
//! that protects the existing failure surface (the `WHERE … status IN
//! ('queued','running')` clause makes a second tick on the same row a
//! no-op).
//!
//! ## Per-scenario override
//!
//! `max_run_duration_secs` is a global default (1800s / 30min). A run
//! can override it via `eval_runs.params_override_json` —
//!
//! ```json
//! { "max_run_duration_secs": 600 }
//! ```
//!
//! …which the watchdog reads per row when deciding whether to finalize.
//! Missing / unparseable values fall back to the global default. No
//! migration needed — `params_override_json` is already a free-form
//! JSON column.

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::eval::store::RunStore;

/// Stable `eval_runs.error` value the watchdog writes when finalizing
/// a stuck row. Downstream consumers (UI banners, classifier) key off
/// this exact string — keep it stable.
pub const TIMEOUT_REASON: &str = "timeout";

/// Default per-run wall-clock budget. A run is considered stuck once
/// `now() - started_at > max_run_duration` and is finalized as
/// `failed` with `error = TIMEOUT_REASON`.
pub const DEFAULT_MAX_RUN_DURATION: Duration = Duration::from_secs(1800);

/// Default cadence at which [`spawn`] re-scans the table.
pub const DEFAULT_TICK_INTERVAL: Duration = Duration::from_secs(30);

/// JSON key on `eval_runs.params_override_json` that overrides the
/// global `max_run_duration_secs` for a single run.
pub const PER_RUN_OVERRIDE_KEY: &str = "max_run_duration_secs";

/// Configuration for [`spawn`] and [`boot_sweep`]. Defaults match the
/// production-safe values (30min budget, 30s tick).
#[derive(Debug, Clone, Copy)]
pub struct WatchdogConfig {
    /// Global default for the per-run wall-clock budget. Individual
    /// runs may override via `params_override_json.max_run_duration_secs`.
    pub max_run_duration: Duration,
    /// How often [`spawn`] wakes up and scans `eval_runs` for stuck rows.
    pub tick_interval: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            max_run_duration: DEFAULT_MAX_RUN_DURATION,
            tick_interval: DEFAULT_TICK_INTERVAL,
        }
    }
}

impl WatchdogConfig {
    /// Convenience constructor for tests / non-default deployments.
    pub fn new(max_run_duration: Duration, tick_interval: Duration) -> Self {
        Self {
            max_run_duration,
            tick_interval,
        }
    }
}

/// One pass over `eval_runs` looking for rows where
/// `status='running'` and `started_at` is older than the per-row
/// threshold (override or default). Finalizes each via
/// [`RunStore::fail_active`] which writes `status='failed'`,
/// `error='timeout'`, `completed_at=now()` atomically.
///
/// Returns the number of rows finalized.
///
/// Idempotent: a second pass over the same rows is a no-op because
/// `fail_active` short-circuits when the row is already terminal.
pub async fn sweep_once(
    pool: &SqlitePool,
    store: &RunStore,
    config: &WatchdogConfig,
    now: DateTime<Utc>,
) -> Result<u64> {
    // CT5 live-exemption (contract §9.2): live deployments are intentionally
    // long-running, so the 30-min default must NEVER finalize them. Exempt
    // `mode = 'live'` rows in the SELECT itself — only stale *backtests* are in
    // scope for the timeout sweep. (The legacy `'paper'` alias maps to Backtest
    // on read, so it is correctly NOT exempt here.)
    let rows = sqlx::query(
        "SELECT id, started_at, params_override_json \
         FROM eval_runs \
         WHERE status = 'running' AND mode != 'live'",
    )
    .fetch_all(pool)
    .await
    .context("select running eval_runs for watchdog sweep")?;

    let global_budget = config.max_run_duration;
    let mut finalized: u64 = 0;
    for row in rows {
        let id: String = match row.try_get("id") {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    target: "xvision::eval::watchdog",
                    error = %e,
                    "watchdog: skip row with unreadable id",
                );
                continue;
            }
        };
        let started_at_str: String = match row.try_get("started_at") {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    target: "xvision::eval::watchdog",
                    run_id = %id,
                    error = %e,
                    "watchdog: skip row with unreadable started_at",
                );
                continue;
            }
        };
        let started_at = match DateTime::parse_from_rfc3339(&started_at_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                tracing::warn!(
                    target: "xvision::eval::watchdog",
                    run_id = %id,
                    started_at = %started_at_str,
                    error = %e,
                    "watchdog: skip row with unparseable started_at",
                );
                continue;
            }
        };
        let params_override_json: Option<String> = row.try_get("params_override_json").ok();
        let budget = per_run_budget(params_override_json.as_deref(), global_budget);

        let age = match now.signed_duration_since(started_at).to_std() {
            Ok(d) => d,
            Err(_) => {
                // started_at is in the future relative to `now` — clock
                // skew or a test using a synthetic clock. Treat as "not
                // stuck".
                continue;
            }
        };
        if age <= budget {
            continue;
        }

        match store.fail_active(&id, TIMEOUT_REASON, None).await {
            Ok(true) => {
                finalized += 1;
                tracing::info!(
                    target: "xvision::eval::watchdog",
                    run_id = %id,
                    age_secs = age.as_secs(),
                    budget_secs = budget.as_secs(),
                    "watchdog finalized stuck eval_run as failed (timeout)",
                );
            }
            Ok(false) => {
                // Row already moved out of `running` between the SELECT
                // and the UPDATE. Idempotent path — nothing to do.
            }
            Err(e) => {
                tracing::warn!(
                    target: "xvision::eval::watchdog",
                    run_id = %id,
                    error = %e,
                    "watchdog: fail_active errored, will retry next tick",
                );
            }
        }
    }
    Ok(finalized)
}

/// One-shot sweep called at engine startup. Identical body to a single
/// [`spawn`] tick; provided as a named entry point so the startup
/// sequence is self-documenting and tests can invoke it directly.
pub async fn boot_sweep(pool: &SqlitePool, store: &RunStore, config: &WatchdogConfig) -> Result<u64> {
    let n = sweep_once(pool, store, config, Utc::now()).await?;
    if n > 0 {
        tracing::info!(
            target: "xvision::eval::watchdog",
            finalized = n,
            "watchdog boot sweep finalized stuck eval_runs",
        );
    }
    Ok(n)
}

/// Spawn the long-running watchdog task. The task runs until the
/// returned [`tokio::task::JoinHandle`] is dropped or cancelled. The
/// caller owns the handle so the eval subsystem can shut it down
/// cleanly on daemon stop.
///
/// On each tick the task calls [`sweep_once`]; errors from a single
/// pass are logged and swallowed — the watchdog never gives up just
/// because the DB blipped.
pub fn spawn(pool: SqlitePool, config: WatchdogConfig) -> tokio::task::JoinHandle<()> {
    let store = RunStore::new(pool.clone());
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.tick_interval);
        // The first tick fires immediately by default; skip it so the
        // boot_sweep (which the caller runs first) isn't duplicated.
        interval.tick().await;
        loop {
            interval.tick().await;
            match sweep_once(&pool, &store, &config, Utc::now()).await {
                Ok(0) => {}
                Ok(n) => tracing::debug!(
                    target: "xvision::eval::watchdog",
                    finalized = n,
                    "watchdog tick finalized stuck eval_runs",
                ),
                Err(e) => tracing::warn!(
                    target: "xvision::eval::watchdog",
                    error = %e,
                    "watchdog tick errored",
                ),
            }
        }
    })
}

/// Read the per-run `max_run_duration_secs` override from
/// `params_override_json`, falling back to the global default when the
/// field is missing, null, or unparseable.
fn per_run_budget(params_override_json: Option<&str>, default: Duration) -> Duration {
    let Some(raw) = params_override_json else {
        return default;
    };
    let parsed: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return default,
    };
    let secs = parsed.get(PER_RUN_OVERRIDE_KEY).and_then(|v| v.as_u64());
    match secs {
        Some(s) => Duration::from_secs(s),
        None => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_run_budget_falls_back_when_field_missing() {
        let raw = r#"{"other_key": 99}"#;
        assert_eq!(
            per_run_budget(Some(raw), Duration::from_secs(1800)),
            Duration::from_secs(1800),
        );
    }

    #[test]
    fn per_run_budget_falls_back_when_null() {
        assert_eq!(
            per_run_budget(None, Duration::from_secs(1800)),
            Duration::from_secs(1800),
        );
    }

    #[test]
    fn per_run_budget_falls_back_when_unparseable() {
        let raw = "not json";
        assert_eq!(
            per_run_budget(Some(raw), Duration::from_secs(1800)),
            Duration::from_secs(1800),
        );
    }

    #[test]
    fn per_run_budget_reads_override() {
        let raw = r#"{"max_run_duration_secs": 600}"#;
        assert_eq!(
            per_run_budget(Some(raw), Duration::from_secs(1800)),
            Duration::from_secs(600),
        );
    }

    // ── CT5 Wave 3a: live-run exemption ─────────────────────────────────────
    // (docs/superpowers/specs/2026-06-13-ct5-live-deployment-contract.md §9.2)
    // Live deployments are intentionally long-running; the 30-min default must
    // NOT kill them. The sweep must still fail a stale *backtest*.
    mod live_exemption {
        use super::*;
        use crate::eval::run::RunStatus;
        use chrono::Utc;
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::SqlitePool;

        async fn migrated_pool() -> SqlitePool {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite mem pool");
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .expect("apply migrations");
            pool
        }

        /// Insert a `running` row with a synthetic `started_at` and the given
        /// mode, bypassing `RunStore::create` (which stamps `started_at = now`
        /// and enforces live_config invariants). Under the SQL-only
        /// `sqlx::migrate!` schema `scenario_id` is still NOT NULL (only the
        /// runtime migrator — not applied here — relaxes it for live runs), so
        /// every row references the seeded fixture scenario. The watchdog never
        /// reads `scenario_id`; only `mode`/`started_at` drive its decision.
        async fn insert_running(pool: &SqlitePool, id: &str, mode: &str, started_at: DateTime<Utc>) {
            sqlx::query(
                "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at) \
                 VALUES (?, ?, 'fixture-scenario', ?, 'running', ?)",
            )
            .bind(id)
            .bind("agent-x")
            .bind(mode)
            .bind(started_at.to_rfc3339())
            .execute(pool)
            .await
            .expect("insert running run");
        }

        async fn seed_scenario(pool: &SqlitePool) {
            sqlx::query(
                "INSERT INTO scenarios (id, source, display_name, body_json, created_at, created_by) \
                 VALUES ('fixture-scenario', 'built', 'fixture', '{}', '2026-01-01T00:00:00Z', 'test')",
            )
            .execute(pool)
            .await
            .expect("seed scenarios row");
        }

        #[tokio::test]
        async fn sweep_does_not_fail_stale_live_run() {
            let pool = migrated_pool().await;
            seed_scenario(&pool).await;
            let store = RunStore::new(pool.clone());
            let config = WatchdogConfig::new(Duration::from_secs(60), Duration::from_millis(10));

            // 10 minutes old — far beyond the 60s budget — but mode=live.
            let live_id = "01LIVE000000000000000000000A";
            let started = Utc::now() - chrono::Duration::seconds(600);
            insert_running(&pool, live_id, "live", started).await;

            let n = sweep_once(&pool, &store, &config, Utc::now()).await.unwrap();
            assert_eq!(n, 0, "a stale live deployment must be exempt from the watchdog");

            let row = store.get(live_id).await.unwrap();
            assert_eq!(
                row.status,
                RunStatus::Running,
                "live run must stay running past the 30-min threshold"
            );
            assert!(row.completed_at.is_none());
            assert!(row.error.is_none());
        }

        #[tokio::test]
        async fn sweep_still_fails_stale_backtest_alongside_exempt_live() {
            let pool = migrated_pool().await;
            seed_scenario(&pool).await;
            let store = RunStore::new(pool.clone());
            let config = WatchdogConfig::new(Duration::from_secs(60), Duration::from_millis(10));

            let live_id = "01LIVE000000000000000000000B";
            let bt_id = "01BACKTEST00000000000000000B";
            let started = Utc::now() - chrono::Duration::seconds(600);
            insert_running(&pool, live_id, "live", started).await;
            insert_running(&pool, bt_id, "backtest", started).await;

            let n = sweep_once(&pool, &store, &config, Utc::now()).await.unwrap();
            assert_eq!(n, 1, "only the stale backtest should be finalized");

            assert_eq!(
                store.get(live_id).await.unwrap().status,
                RunStatus::Running,
                "live run stays running"
            );
            let bt = store.get(bt_id).await.unwrap();
            assert_eq!(bt.status, RunStatus::Failed, "stale backtest still fails");
            assert_eq!(bt.error.as_deref(), Some(TIMEOUT_REASON));
        }
    }
}
