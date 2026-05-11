//! `xvn eod` — end-of-day operator report. Closes v1 success criterion §168
//! item 5 ("Reset: `xvn eod` produces a sensible markdown report from the
//! test-session data").
//!
//! v1 cut: queries the actually-existing `api_audit` + `eval_*` tables.
//! Wallet/live sections (positions, halt status, reservations) render a
//! placeholder line so the report layout stays stable when Plan 2c +
//! the wallet plan ship.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use clap::Args;
use sqlx::SqlitePool;
use xvision_engine::api::{Actor, ApiContext};

#[derive(Args, Debug)]
pub struct EodArgs {
    /// Window length in hours (default: 24).
    #[arg(long, default_value_t = 24)]
    pub hours: u64,

    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

pub async fn run(args: EodArgs) -> Result<()> {
    let xvn_home = resolve_xvn_home(args.xvn_home.clone())?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    let ctx = ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))?;

    let report = render_report(&ctx.db, args.hours).await?;
    println!("{report}");
    Ok(())
}

fn resolve_xvn_home(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    if let Ok(p) = std::env::var("XVN_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().context("HOME not set; pass --xvn-home")?;
    Ok(home.join(".xvn"))
}

/// Pure render fn — takes a pool + window and returns the full markdown
/// report. Split out from `run` so tests can drive it against an in-memory
/// pool without going through the CLI binary.
pub async fn render_report(pool: &SqlitePool, hours: u64) -> Result<String> {
    let now = Utc::now();
    let since = now - Duration::hours(hours as i64);
    let since_rfc = since.to_rfc3339();
    let mut out = String::new();

    out.push_str(&format!(
        "# xvision EOD report — {}\n\n",
        now.format("%Y-%m-%d %H:%M UTC")
    ));
    out.push_str(&format!(
        "**Window:** last {hours} hour(s) (since {}).\n\n",
        since.format("%Y-%m-%d %H:%M UTC"),
    ));

    out.push_str(&render_eval_runs(pool, &since_rfc).await?);
    out.push_str(&render_per_strategy(pool, &since_rfc).await?);
    out.push_str(&render_audit_activity(pool, &since_rfc).await?);
    out.push_str(&render_errors(pool, &since_rfc).await?);
    out.push_str(&render_deferred_stubs());

    Ok(out)
}

async fn render_eval_runs(pool: &SqlitePool, since_rfc: &str) -> Result<String> {
    let mut s = String::from("## Eval runs\n\n");

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eval_runs WHERE started_at >= ?1")
        .bind(since_rfc)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    if total == 0 {
        s.push_str("No eval runs in the window.\n\n");
        return Ok(s);
    }

    let by_status: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM eval_runs WHERE started_at >= ?1 \
         GROUP BY status ORDER BY 2 DESC",
    )
    .bind(since_rfc)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    s.push_str(&format!("- Total: {total}\n"));
    for (status, n) in &by_status {
        s.push_str(&format!("- {status}: {n}\n"));
    }

    let n_decisions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM eval_decisions d \
         JOIN eval_runs r ON d.run_id = r.id \
         WHERE r.started_at >= ?1",
    )
    .bind(since_rfc)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let n_trades: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM eval_decisions d \
         JOIN eval_runs r ON d.run_id = r.id \
         WHERE r.started_at >= ?1 AND d.fill_price IS NOT NULL",
    )
    .bind(since_rfc)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    s.push_str(&format!("- Total decisions: {n_decisions}\n"));
    s.push_str(&format!("- Total fills: {n_trades}\n\n"));
    Ok(s)
}

async fn render_per_strategy(pool: &SqlitePool, since_rfc: &str) -> Result<String> {
    let mut s = String::from("## Per-strategy summary\n\n");

    let rows: Vec<(String, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT strategy_bundle_hash, \
                COUNT(*), \
                SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), \
                MAX(metrics_json) \
         FROM eval_runs \
         WHERE started_at >= ?1 \
         GROUP BY strategy_bundle_hash \
         ORDER BY 2 DESC",
    )
    .bind(since_rfc)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if rows.is_empty() {
        s.push_str("No strategies exercised in the window.\n\n");
        return Ok(s);
    }

    s.push_str("| Strategy | Runs | Completed | Best Sharpe | Best return % |\n");
    s.push_str("|---|---|---|---|---|\n");
    for (bundle, runs, completed, sample_metrics) in rows {
        let (best_sharpe, best_return) =
            best_metrics_for_bundle(pool, &bundle, since_rfc, sample_metrics).await;
        s.push_str(&format!(
            "| `{bundle}` | {runs} | {completed} | {best_sharpe} | {best_return} |\n",
        ));
    }
    s.push('\n');
    Ok(s)
}

async fn best_metrics_for_bundle(
    pool: &SqlitePool,
    bundle: &str,
    since_rfc: &str,
    _sample: Option<String>,
) -> (String, String) {
    let metrics_rows: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT metrics_json FROM eval_runs \
         WHERE strategy_bundle_hash = ?1 AND started_at >= ?2 \
           AND metrics_json IS NOT NULL",
    )
    .bind(bundle)
    .bind(since_rfc)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut best_sharpe: Option<f64> = None;
    let mut best_return: Option<f64> = None;
    for (raw,) in metrics_rows {
        let Some(json) = raw else { continue };
        let parsed: serde_json::Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(s) = parsed.get("sharpe").and_then(|v| v.as_f64()) {
            if s.is_finite() {
                best_sharpe = Some(best_sharpe.map_or(s, |cur| cur.max(s)));
            }
        }
        if let Some(r) = parsed.get("total_return_pct").and_then(|v| v.as_f64()) {
            if r.is_finite() {
                best_return = Some(best_return.map_or(r, |cur| cur.max(r)));
            }
        }
    }

    let sharpe_s = best_sharpe
        .map(|v| format!("{v:.3}"))
        .unwrap_or_else(|| "—".into());
    let return_s = best_return
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "—".into());
    (sharpe_s, return_s)
}

async fn render_audit_activity(pool: &SqlitePool, since_rfc: &str) -> Result<String> {
    let mut s = String::from("## Audit activity\n\n");

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_audit WHERE occurred_at >= ?1")
        .bind(since_rfc)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    if total == 0 {
        s.push_str("No engine API calls in the window.\n\n");
        return Ok(s);
    }

    s.push_str(&format!("- Total calls: {total}\n\n"));
    s.push_str("| Domain | Operation | Calls |\n");
    s.push_str("|---|---|---|\n");
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT domain, operation, COUNT(*) FROM api_audit \
         WHERE occurred_at >= ?1 \
         GROUP BY domain, operation \
         ORDER BY 3 DESC \
         LIMIT 10",
    )
    .bind(since_rfc)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (domain, op, n) in rows {
        s.push_str(&format!("| {domain} | {op} | {n} |\n"));
    }
    s.push('\n');
    Ok(s)
}

async fn render_errors(pool: &SqlitePool, since_rfc: &str) -> Result<String> {
    let mut s = String::from("## Errors\n\n");

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM api_audit WHERE occurred_at >= ?1 AND outcome = 'error'")
            .bind(since_rfc)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    if total == 0 {
        s.push_str("Zero errors — clean window.\n\n");
        return Ok(s);
    }

    s.push_str(&format!("- Total errors: {total}\n\n"));
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT domain, operation, COALESCE(error, '<no message>'), COUNT(*) FROM api_audit \
         WHERE occurred_at >= ?1 AND outcome = 'error' \
         GROUP BY domain, operation, error \
         ORDER BY 4 DESC \
         LIMIT 5",
    )
    .bind(since_rfc)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    s.push_str("| Domain | Operation | Error | Count |\n");
    s.push_str("|---|---|---|---|\n");
    for (domain, op, err, n) in rows {
        // Truncate long error messages to keep the table readable.
        let trimmed: String = err.chars().take(120).collect();
        let short = if err.len() > trimmed.len() {
            format!("{trimmed}…")
        } else {
            trimmed
        };
        s.push_str(&format!("| {domain} | {op} | {short} | {n} |\n"));
    }
    s.push('\n');
    Ok(s)
}

fn render_deferred_stubs() -> String {
    let mut s = String::new();
    s.push_str("## Halt status\n\n");
    s.push_str("(Available once `xvn live` and the global-halt switch ship.)\n\n");
    s.push_str("## Positions\n\n");
    s.push_str("(Available once the wallet plan's positions table ships.)\n\n");
    s.push_str("## Reservation hygiene\n\n");
    s.push_str("(Available once Plan 2c's reservation reaper ships.)\n\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn empty_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(include_str!(
            "../../../xvision-engine/migrations/001_api_audit.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(include_str!("../../../xvision-engine/migrations/002_eval.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn empty_state_renders_all_sections() {
        let pool = empty_pool().await;
        let report = render_report(&pool, 24).await.unwrap();
        assert!(report.contains("# xvision EOD report"));
        assert!(report.contains("## Eval runs"));
        assert!(report.contains("No eval runs in the window."));
        assert!(report.contains("## Per-strategy summary"));
        assert!(report.contains("No strategies exercised in the window."));
        assert!(report.contains("## Audit activity"));
        assert!(report.contains("No engine API calls in the window."));
        assert!(report.contains("## Errors"));
        assert!(report.contains("Zero errors"));
        assert!(report.contains("## Halt status"));
        assert!(report.contains("## Positions"));
        assert!(report.contains("## Reservation hygiene"));
    }

    #[tokio::test]
    async fn populated_state_surfaces_runs_and_audit() {
        let pool = empty_pool().await;
        let now = Utc::now();
        // Insert one completed eval run with metrics.
        let metrics = serde_json::json!({
            "total_return_pct": 12.5,
            "sharpe": 1.7,
            "max_drawdown_pct": 4.2,
            "win_rate": 0.6,
            "n_trades": 3,
            "n_decisions": 10,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO eval_runs (id, strategy_bundle_hash, scenario_id, mode, status, \
             started_at, completed_at, metrics_json) \
             VALUES (?1, 'bundle-abc', 'flash-crash-2024-08', 'backtest', 'completed', ?2, ?2, ?3)",
        )
        .bind("run-1")
        .bind(now.to_rfc3339())
        .bind(&metrics)
        .execute(&pool)
        .await
        .unwrap();
        // Insert one decision with a fill.
        sqlx::query(
            "INSERT INTO eval_decisions (run_id, decision_index, timestamp, asset, action, fill_price) \
             VALUES ('run-1', 0, ?1, 'BTC/USD', 'long_open', 60000.0)",
        )
        .bind(now.to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
        // Insert one audit row (ok) and one error.
        sqlx::query(
            "INSERT INTO api_audit (id, occurred_at, actor, domain, operation, outcome, duration_ms) \
             VALUES ('a1', ?1, 'cli', 'eval', 'run', 'ok', 100), \
                    ('a2', ?1, 'cli', 'eval', 'list', 'error', 5)",
        )
        .bind(now.to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE api_audit SET error = 'boom' WHERE id = 'a2'")
            .execute(&pool)
            .await
            .unwrap();

        let report = render_report(&pool, 24).await.unwrap();
        assert!(report.contains("- Total: 1"));
        assert!(report.contains("- completed: 1"));
        assert!(report.contains("- Total decisions: 1"));
        assert!(report.contains("- Total fills: 1"));
        assert!(report.contains("`bundle-abc`"));
        assert!(report.contains("1.700")); // best sharpe
        assert!(report.contains("12.50")); // best return
        assert!(report.contains("- Total calls: 2"));
        assert!(report.contains("- Total errors: 1"));
        assert!(report.contains("boom"));
    }
}
