//! Per-run live-deployment capital-risk snapshot (CT5 contract). One row per
//! live (`mode='live'`) run, upserted by `run_inner_live` each bar. Values are
//! per-run `PortfolioBook`-computed (NOT broker-truth — design spec §3).

use anyhow::{Context, Result};
use sqlx::{FromRow, SqlitePool};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct LiveRunState {
    pub run_id: String,
    pub strategy_id: Option<String>,
    pub strategy_name: Option<String>,
    pub deployed_capital_usd: f64,
    pub equity_usd: Option<f64>,
    pub unrealized_pnl_usd: Option<f64>,
    pub realized_pnl_usd: Option<f64>,
    pub realized_today_usd: Option<f64>,
    pub daily_loss_remaining_usd: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub peak_equity_usd: Option<f64>,
    // i64 → JSON integer decodes as a JS `number`, not BigInt; pin the TS type.
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub risk_veto_count: i64,
    pub last_decision_at: Option<String>,
    pub updated_at: String,
    /// Daily-loss budget in USD = kill_pct × initial capital. `None` when
    /// the strategy has no kill percentage configured (kill_pct == 0.0).
    /// Unlocks the strip's buffer %-gradient (remaining / budget).
    pub daily_loss_budget_usd: Option<f64>,
    /// Wall-clock deadline (RFC-3339) = started_at + time_limit_secs. `None`
    /// when the stop policy is bar- or decision-bounded (no wall-clock ETA).
    /// Unlocks awm's ETA display.
    pub stop_at: Option<String>,
}

#[derive(Clone)]
pub struct LiveStateStore {
    pool: SqlitePool,
}

impl LiveStateStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, s: &LiveRunState) -> Result<()> {
        sqlx::query(
            "INSERT INTO live_run_state \
             (run_id, strategy_id, strategy_name, deployed_capital_usd, equity_usd, \
              unrealized_pnl_usd, realized_pnl_usd, realized_today_usd, \
              daily_loss_remaining_usd, drawdown_pct, peak_equity_usd, \
              risk_veto_count, last_decision_at, updated_at, \
              daily_loss_budget_usd, stop_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(run_id) DO UPDATE SET \
              strategy_id=excluded.strategy_id, strategy_name=excluded.strategy_name, \
              deployed_capital_usd=excluded.deployed_capital_usd, equity_usd=excluded.equity_usd, \
              unrealized_pnl_usd=excluded.unrealized_pnl_usd, realized_pnl_usd=excluded.realized_pnl_usd, \
              realized_today_usd=excluded.realized_today_usd, \
              daily_loss_remaining_usd=excluded.daily_loss_remaining_usd, \
              drawdown_pct=excluded.drawdown_pct, peak_equity_usd=excluded.peak_equity_usd, \
              risk_veto_count=excluded.risk_veto_count, last_decision_at=excluded.last_decision_at, \
              updated_at=excluded.updated_at, \
              daily_loss_budget_usd=excluded.daily_loss_budget_usd, \
              stop_at=excluded.stop_at",
        )
        .bind(&s.run_id)
        .bind(&s.strategy_id)
        .bind(&s.strategy_name)
        .bind(s.deployed_capital_usd)
        .bind(s.equity_usd)
        .bind(s.unrealized_pnl_usd)
        .bind(s.realized_pnl_usd)
        .bind(s.realized_today_usd)
        .bind(s.daily_loss_remaining_usd)
        .bind(s.drawdown_pct)
        .bind(s.peak_equity_usd)
        .bind(s.risk_veto_count)
        .bind(&s.last_decision_at)
        .bind(&s.updated_at)
        .bind(s.daily_loss_budget_usd)
        .bind(&s.stop_at)
        .execute(&self.pool)
        .await
        .with_context(|| format!("upsert live_run_state run_id={}", s.run_id))?;
        Ok(())
    }

    pub async fn get(&self, run_id: &str) -> Result<Option<LiveRunState>> {
        Ok(
            sqlx::query_as::<_, LiveRunState>("SELECT * FROM live_run_state WHERE run_id = ?")
                .bind(run_id)
                .fetch_optional(&self.pool)
                .await
                .context("get live_run_state")?,
        )
    }
}
