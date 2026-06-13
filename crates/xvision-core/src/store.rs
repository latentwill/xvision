//! SQLite persistence using `sqlx` with runtime queries.
//!
//! Compile-time `query!` macros are intentionally NOT used in v1 — they require
//! a live DATABASE_URL or sqlx-prepare cache, which adds CI friction during the
//! schema-churn phase. The runtime `sqlx::query` API gives the same safety net
//! at the cost of one extra parse per call; performance is irrelevant for this
//! workload (≤100 rows per backtest run).
//!
//! `decisions` and `risk_outcomes` are keyed on `(cycle_id, arm_name)` so
//! multiple strategy arms persist independently.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};
use thiserror::Error;
use uuid::Uuid;

use crate::trading::{RiskDecision, TraderDecision};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlx error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Open or create a SQLite database at `path` and run pending migrations.
    /// Pass `:memory:` for tests.
    pub async fn open(url: &str) -> Result<Self, StoreError> {
        if let Some(file) = url.strip_prefix("sqlite://") {
            if file != ":memory:" && !file.is_empty() {
                if let Some(parent) = Path::new(file).parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
            }
        }
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(
                url.parse::<sqlx::sqlite::SqliteConnectOptions>()?
                    .create_if_missing(true),
            )
            .await?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    pub async fn migrate(&self) -> Result<(), StoreError> {
        // sqlx::migrate! discovers all .sql files in the migrations directory at
        // compile time, applies them in lexical order, and tracks applied versions
        // in the _sqlx_migrations table — so re-running is idempotent.
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Row counts for the named tables, in input order. Missing tables surface
    /// as the underlying `sqlx::Error`. Use for diagnostic CLI output.
    pub async fn counts(&self, tables: &[&str]) -> Result<Vec<(String, i64)>, StoreError> {
        let mut out = Vec::with_capacity(tables.len());
        for t in tables {
            let q = format!("SELECT COUNT(*) FROM {t}");
            let n: i64 = sqlx::query_scalar(&q).fetch_one(&self.pool).await?;
            out.push(((*t).to_string(), n));
        }
        Ok(out)
    }

    // --- cycles ----------------------------------------------------------

    pub async fn upsert_cycle(
        &self,
        cycle_id: &Uuid,
        asset: &str,
        horizon_h: u32,
        market_state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT OR REPLACE INTO cycles (cycle_id, asset, horizon_h, market_state_json, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(cycle_id.to_string())
        .bind(asset)
        .bind(horizon_h as i64)
        .bind(market_state.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- decisions -------------------------------------------------------

    pub async fn insert_decision(&self, arm_name: &str, decision: &TraderDecision) -> Result<(), StoreError> {
        let json = serde_json::to_string(decision)?;
        sqlx::query(
            "INSERT OR REPLACE INTO decisions (cycle_id, arm_name, decision_json, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(decision.cycle_id.to_string())
        .bind(arm_name)
        .bind(json)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_decisions_for_setup(
        &self,
        cycle_id: &Uuid,
    ) -> Result<Vec<(String, TraderDecision)>, StoreError> {
        let rows =
            sqlx::query("SELECT arm_name, decision_json FROM decisions WHERE cycle_id = ? ORDER BY arm_name")
                .bind(cycle_id.to_string())
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|r| {
                let arm: String = r.get(0);
                let json: String = r.get(1);
                let d = serde_json::from_str::<TraderDecision>(&json).map_err(StoreError::Json)?;
                Ok((arm, d))
            })
            .collect()
    }

    // --- risk outcomes ---------------------------------------------------

    pub async fn insert_risk_outcome(
        &self,
        arm_name: &str,
        decision: &RiskDecision,
    ) -> Result<(), StoreError> {
        let cycle_id = decision
            .effective()
            .map(|d| d.cycle_id)
            .or(match decision {
                RiskDecision::Vetoed { original, .. } => Some(original.cycle_id),
                _ => None,
            })
            .expect("RiskDecision must reference a TraderDecision");
        let json = serde_json::to_string(decision)?;
        sqlx::query(
            "INSERT OR REPLACE INTO risk_outcomes (cycle_id, arm_name, risk_decision_json, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(cycle_id.to_string())
        .bind(arm_name)
        .bind(json)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- traces ----------------------------------------------------------

    pub async fn insert_trace(&self, span: &TraceSpan) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT OR REPLACE INTO traces \
             (trace_id, span_id, parent_id, run_id, cycle_id, stage, name, attrs_json, started_at, ended_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&span.trace_id)
        .bind(&span.span_id)
        .bind(span.parent_id.as_deref())
        .bind(&span.run_id)
        .bind(span.cycle_id.map(|u| u.to_string()))
        .bind(&span.stage)
        .bind(&span.name)
        .bind(serde_json::to_string(&span.attrs)?)
        .bind(span.started_at.to_rfc3339())
        .bind(span.ended_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_id: Option<String>,
    pub run_id: String,
    pub cycle_id: Option<Uuid>,
    pub stage: String,
    pub name: String,
    pub attrs: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading::{Action, AssetSymbol, Direction, TraderDecision, VetoReason};
    use chrono::TimeZone;

    fn make_decision() -> TraderDecision {
        TraderDecision {
            cycle_id: Uuid::nil(),
            action: Action::Buy,
            size_bps: 1000,
            direction: Direction::Long,
            stop_loss_pct: 2.5,
            take_profit_pct: 5.0,
            trader_summary: "Long entry on confirmed range break with 2:1 R:R.".into(),
            asset: AssetSymbol::Btc,
            // Advanced SL/TP knobs default to None in fixtures (added after this
            // helper was written; keeps the lib-test build compiling).
            trailing_stop_pct: None,
            breakeven_trigger_pct: None,
            breakeven_offset_pct: None,
            fade_sl_bars: None,
            fade_sl_start_pct: None,
            fade_sl_end_pct: None,
            max_bars_held: None,
            sl_atr_mult: None,
            tp_atr_mult: None,
            tp1_pct: None,
            tp1_close_fraction: None,
            tp2_pct: None,
        }
    }

    async fn fresh_store() -> Store {
        let s = Store::open("sqlite::memory:").await.expect("memory db must open");
        // All test rows reference setup nil; insert it once so FK constraints hold.
        s.upsert_cycle(&Uuid::nil(), "BTC", 24, &serde_json::json!({"price": 70000.0}))
            .await
            .expect("seed setup row");
        s
    }

    #[tokio::test]
    async fn paired_decisions_persist_independently() {
        // Tier 1 fix #1 corollary: same cycle_id, different arm_name → both
        // rows exist.
        let s = fresh_store().await;
        let d = make_decision();
        s.insert_decision("trader_arm", &d).await.unwrap();
        s.insert_decision("buy_and_hold", &d).await.unwrap();
        let rows = s.get_decisions_for_setup(&Uuid::nil()).await.unwrap();
        assert_eq!(rows.len(), 2, "both arms persist");
        let arms: Vec<_> = rows.iter().map(|(a, _)| a.as_str()).collect();
        assert!(arms.contains(&"trader_arm") && arms.contains(&"buy_and_hold"));
    }

    #[tokio::test]
    async fn risk_outcome_keyed_by_arm() {
        let s = fresh_store().await;
        let d = make_decision();
        let approved = RiskDecision::Approved {
            decision: d.clone(),
            warnings: vec![],
        };
        let vetoed = RiskDecision::Vetoed {
            original: d,
            reason: VetoReason::DailyLossCircuitBreaker,
        };
        s.insert_risk_outcome("trader_arm", &approved).await.unwrap();
        s.insert_risk_outcome("buy_and_hold", &vetoed).await.unwrap();
        // Both rows must exist.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM risk_outcomes WHERE cycle_id = ?")
            .bind(Uuid::nil().to_string())
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn trace_span_round_trips() {
        let s = fresh_store().await;
        let span = TraceSpan {
            trace_id: "t1".into(),
            span_id: "s1".into(),
            parent_id: None,
            run_id: "r1".into(),
            cycle_id: Some(Uuid::nil()),
            stage: "briefing".into(),
            name: "brief".into(),
            attrs: serde_json::json!({"provider": "anthropic"}),
            started_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
            ended_at: Utc.timestamp_opt(1_700_000_001, 0).single().unwrap(),
        };
        s.insert_trace(&span).await.unwrap();
        let row: (String, String) = sqlx::query_as("SELECT stage, name FROM traces WHERE span_id = ?")
            .bind("s1")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(row, ("briefing".into(), "brief".into()));
    }

    #[tokio::test]
    async fn migrate_is_idempotent() {
        let s = fresh_store().await;
        s.migrate().await.expect("re-migrate must not fail");
        s.migrate().await.expect("third re-migrate must not fail");
    }
}
