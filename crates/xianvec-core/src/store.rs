//! SQLite persistence using `sqlx` with runtime queries.
//!
//! Compile-time `query!` macros are intentionally NOT used in v1 — they require
//! a live DATABASE_URL or sqlx-prepare cache, which adds CI friction during the
//! schema-churn phase. The runtime `sqlx::query` API gives the same safety net
//! at the cost of one extra parse per call; performance is irrelevant for this
//! workload (≤100 rows per backtest run).
//!
//! Tier 1 fix #1: `briefings` is keyed on `setup_id` alone — every arm reads
//! the same briefing. `decisions` and `risk_outcomes` are keyed on
//! `(setup_id, arm_name)` so multiple strategy arms persist independently.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};
use thiserror::Error;
use uuid::Uuid;

use crate::trading::{InternBriefing, RiskDecision, TraderDecision};

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
        // Single migration file shipped with the crate. When the schema settles
        // we can switch to sqlx_migrate macro that bundles the dir.
        let sql = include_str!("../migrations/0001_init.sql");
        sqlx::raw_sql(sql).execute(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // --- setups ----------------------------------------------------------

    pub async fn upsert_setup(
        &self,
        setup_id: &Uuid,
        asset: &str,
        horizon_h: u32,
        market_state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT OR REPLACE INTO setups (setup_id, asset, horizon_h, market_state_json, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(setup_id.to_string())
        .bind(asset)
        .bind(horizon_h as i64)
        .bind(market_state.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- briefings -------------------------------------------------------

    /// Insert or replace the briefing for `setup_id`. All arms read
    /// the same row (Tier 1 fix #1).
    pub async fn upsert_briefing(
        &self,
        provider: &str,
        model: &str,
        briefing: &InternBriefing,
    ) -> Result<(), StoreError> {
        let json = serde_json::to_string(briefing)?;
        sqlx::query(
            "INSERT OR REPLACE INTO briefings (setup_id, provider, model, briefing_json, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(briefing.setup_id.to_string())
        .bind(provider)
        .bind(model)
        .bind(json)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_briefing(&self, setup_id: &Uuid) -> Result<Option<InternBriefing>, StoreError> {
        let row = sqlx::query("SELECT briefing_json FROM briefings WHERE setup_id = ?")
            .bind(setup_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| {
            let s: String = r.get(0);
            serde_json::from_str::<InternBriefing>(&s).map_err(StoreError::Json)
        })
        .transpose()
    }

    // --- decisions -------------------------------------------------------

    pub async fn insert_decision(&self, arm_name: &str, decision: &TraderDecision) -> Result<(), StoreError> {
        let json = serde_json::to_string(decision)?;
        sqlx::query(
            "INSERT OR REPLACE INTO decisions (setup_id, arm_name, decision_json, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(decision.setup_id.to_string())
        .bind(arm_name)
        .bind(json)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_decisions_for_setup(
        &self,
        setup_id: &Uuid,
    ) -> Result<Vec<(String, TraderDecision)>, StoreError> {
        let rows =
            sqlx::query("SELECT arm_name, decision_json FROM decisions WHERE setup_id = ? ORDER BY arm_name")
                .bind(setup_id.to_string())
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
        let setup_id = decision
            .effective()
            .map(|d| d.setup_id)
            .or(match decision {
                RiskDecision::Vetoed { original, .. } => Some(original.setup_id),
                _ => None,
            })
            .expect("RiskDecision must reference a TraderDecision");
        let json = serde_json::to_string(decision)?;
        sqlx::query(
            "INSERT OR REPLACE INTO risk_outcomes (setup_id, arm_name, risk_decision_json, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(setup_id.to_string())
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
             (trace_id, span_id, parent_id, run_id, setup_id, stage, name, attrs_json, started_at, ended_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&span.trace_id)
        .bind(&span.span_id)
        .bind(span.parent_id.as_deref())
        .bind(&span.run_id)
        .bind(span.setup_id.map(|u| u.to_string()))
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
    pub setup_id: Option<Uuid>,
    pub stage: String,
    pub name: String,
    pub attrs: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading::{Action, AssetSymbol, Direction, EvidenceTag, Regime, TraderDecision, VetoReason};
    use chrono::TimeZone;

    fn fixture_briefing() -> InternBriefing {
        InternBriefing {
            setup_id: Uuid::nil(),
            asset: AssetSymbol::Btc,
            bull_case: "Funding rate compressed; smart money accumulating spot.".into(),
            bear_case: "Realized vol expanding; long-leverage near prior squeeze.".into(),
            flat_case: "Range-bound between SMA20 and SMA50; await break.".into(),
            evidence_long: vec![EvidenceTag::Onchain("smart_money_inflow".into())],
            evidence_short: vec![EvidenceTag::Technical("rsi_overbought".into())],
            evidence_flat: vec![EvidenceTag::Technical("range_bound".into())],
            regime: Regime::Chop,
            signal_quality: 0.6,
            horizon_hours: 24,
            created_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    fn make_decision() -> TraderDecision {
        TraderDecision {
            setup_id: Uuid::nil(),
            action: Action::Buy,
            size_bps: 1000,
            direction: Direction::Long,
            stop_loss_pct: 2.5,
            take_profit_pct: 5.0,
            trader_summary: "Long entry on confirmed range break with 2:1 R:R.".into(),
        }
    }

    async fn fresh_store() -> Store {
        let s = Store::open("sqlite::memory:").await.expect("memory db must open");
        // All test rows reference setup nil; insert it once so FK constraints hold.
        s.upsert_setup(&Uuid::nil(), "BTC", 24, &serde_json::json!({"price": 70000.0}))
            .await
            .expect("seed setup row");
        s
    }

    #[tokio::test]
    async fn briefing_round_trips() {
        let s = fresh_store().await;
        let b = fixture_briefing();
        s.upsert_briefing("anthropic", "claude-haiku-4-5", &b)
            .await
            .unwrap();
        let back = s.get_briefing(&b.setup_id).await.unwrap().expect("present");
        assert_eq!(b, back);
    }

    #[tokio::test]
    async fn upsert_briefing_replaces_same_setup() {
        let s = fresh_store().await;
        let mut b = fixture_briefing();
        s.upsert_briefing("anthropic", "claude-haiku-4-5", &b)
            .await
            .unwrap();
        b.bull_case = "Updated bull case with sufficiently long content.".into();
        s.upsert_briefing("anthropic", "claude-haiku-4-5", &b)
            .await
            .unwrap();
        let back = s.get_briefing(&b.setup_id).await.unwrap().expect("present");
        assert_eq!(back.bull_case, b.bull_case);
    }

    #[tokio::test]
    async fn paired_decisions_persist_independently() {
        // Tier 1 fix #1 corollary: same setup_id, different arm_name → both
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
        let approved = RiskDecision::Approved { decision: d.clone() };
        let vetoed = RiskDecision::Vetoed {
            original: d,
            reason: VetoReason::DailyLossCircuitBreaker,
        };
        s.insert_risk_outcome("trader_arm", &approved).await.unwrap();
        s.insert_risk_outcome("buy_and_hold", &vetoed).await.unwrap();
        // Both rows must exist.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM risk_outcomes WHERE setup_id = ?")
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
            setup_id: Some(Uuid::nil()),
            stage: "intern".into(),
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
        assert_eq!(row, ("intern".into(), "brief".into()));
    }

    #[tokio::test]
    async fn migrate_is_idempotent() {
        let s = fresh_store().await;
        s.migrate().await.expect("re-migrate must not fail");
        s.migrate().await.expect("third re-migrate must not fail");
    }
}
