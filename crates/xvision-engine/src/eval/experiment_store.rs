//! `ExperimentStore` — sqlx-backed persistence for the experiment ledger.
//!
//! Owned data: `experiments` table (created by migration 022). An experiment
//! groups one research question across a set of strategies + scenarios, and
//! optionally binds to an `eval_batches` row once the experiment has been run.
//!
//! Design notes:
//! - `strategy_ids` and `scenario_ids` are stored as JSON arrays in TEXT
//!   columns (same pattern as other list-valued columns in this codebase).
//! - `result_json` is operator/auto-populated when the bound batch finishes.
//! - `bind_batch` attaches an existing `eval_batches.batch_id` to the
//!   experiment; the FK is declared but not enforced at insert time (SQLite
//!   deferred FKs).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ulid::Ulid;

/// Row shape mirroring the `experiments` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Experiment {
    pub experiment_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// Strategy ids under test (stored as JSON array in DB).
    pub strategy_ids: Vec<String>,
    /// Scenario ids used for runs (stored as JSON array in DB).
    pub scenario_ids: Vec<String>,
    /// Bound eval batch id. `None` until the experiment is run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_budget: Option<i64>,
    /// JSON result summary, populated when the bound batch finishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_json: Option<serde_json::Value>,
    /// Operator-written conclusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<String>,
    /// Operator-written recommendation for follow-up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_recommendation: Option<String>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}

/// Fields required to create a new experiment.
#[derive(Debug, Clone)]
pub struct CreateExperimentRequest {
    pub name: String,
    pub question: Option<String>,
    pub strategy_ids: Vec<String>,
    pub scenario_ids: Vec<String>,
    pub decision_budget: Option<i64>,
}

/// Fields that can be mutated on an existing experiment.
#[derive(Debug, Clone, Default)]
pub struct ExperimentMutations {
    pub conclusion: Option<String>,
    pub next_recommendation: Option<String>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExperimentStore {
    pool: SqlitePool,
}

impl ExperimentStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert a new experiment row.
    pub async fn create(&self, req: CreateExperimentRequest) -> Result<Experiment> {
        let experiment_id = format!("exp_{}", Ulid::new());
        let now = Utc::now();
        let strategy_ids_json =
            serde_json::to_string(&req.strategy_ids).context("serialize strategy_ids")?;
        let scenario_ids_json =
            serde_json::to_string(&req.scenario_ids).context("serialize scenario_ids")?;

        sqlx::query(
            "INSERT INTO experiments \
             (experiment_id, name, question, strategy_ids, scenario_ids, batch_id, \
              decision_budget, result_json, conclusion, next_recommendation, \
              created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, NULL, ?, NULL, NULL, NULL, ?, ?)",
        )
        .bind(&experiment_id)
        .bind(&req.name)
        .bind(&req.question)
        .bind(&strategy_ids_json)
        .bind(&scenario_ids_json)
        .bind(req.decision_budget)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .with_context(|| format!("insert experiment experiment_id={experiment_id}"))?;

        Ok(Experiment {
            experiment_id,
            name: req.name,
            question: req.question,
            strategy_ids: req.strategy_ids,
            scenario_ids: req.scenario_ids,
            batch_id: None,
            decision_budget: req.decision_budget,
            result_json: None,
            conclusion: None,
            next_recommendation: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Load a single experiment by id. Returns `None` when not found.
    pub async fn get(&self, experiment_id: &str) -> Result<Option<Experiment>> {
        let row: Option<ExperimentRow> = sqlx::query_as(
            "SELECT experiment_id, name, question, strategy_ids, scenario_ids, batch_id, \
             decision_budget, result_json, conclusion, next_recommendation, created_at, updated_at \
             FROM experiments WHERE experiment_id = ?",
        )
        .bind(experiment_id)
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("get experiment experiment_id={experiment_id}"))?;

        row.map(row_to_experiment).transpose()
    }

    /// List all experiments, most-recent first.
    pub async fn list(&self) -> Result<Vec<Experiment>> {
        let rows: Vec<ExperimentRow> = sqlx::query_as(
            "SELECT experiment_id, name, question, strategy_ids, scenario_ids, batch_id, \
             decision_budget, result_json, conclusion, next_recommendation, created_at, updated_at \
             FROM experiments ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("list experiments")?;

        rows.into_iter().map(row_to_experiment).collect()
    }

    /// Apply a partial mutation to an existing experiment. Only `Some` fields
    /// are written; `None` fields are left unchanged.
    pub async fn update(
        &self,
        experiment_id: &str,
        mutations: ExperimentMutations,
    ) -> Result<Experiment> {
        let now = Utc::now();

        // Build a dynamic UPDATE using only the provided mutations.
        // Always bumps `updated_at`.
        let mut sets: Vec<String> = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<Option<String>> = vec![Some(now.to_rfc3339())];

        if let Some(ref v) = mutations.conclusion {
            sets.push("conclusion = ?".into());
            binds.push(Some(v.clone()));
        }
        if let Some(ref v) = mutations.next_recommendation {
            sets.push("next_recommendation = ?".into());
            binds.push(Some(v.clone()));
        }
        if let Some(ref v) = mutations.batch_id {
            sets.push("batch_id = ?".into());
            binds.push(Some(v.clone()));
        }

        let sql = format!(
            "UPDATE experiments SET {} WHERE experiment_id = ?",
            sets.join(", ")
        );

        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b.as_deref());
        }
        q = q.bind(experiment_id);

        let result = q
            .execute(&self.pool)
            .await
            .with_context(|| format!("update experiment experiment_id={experiment_id}"))?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!("experiment not found: {experiment_id}"));
        }

        self.get(experiment_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("experiment disappeared after update: {experiment_id}"))
    }

    /// Store a result JSON blob for the experiment (called when the bound batch
    /// finishes).
    pub async fn set_result(
        &self,
        experiment_id: &str,
        result: serde_json::Value,
    ) -> Result<Experiment> {
        let now = Utc::now();
        let result_str = serde_json::to_string(&result).context("serialize result_json")?;

        let rows = sqlx::query(
            "UPDATE experiments SET result_json = ?, updated_at = ? \
             WHERE experiment_id = ?",
        )
        .bind(&result_str)
        .bind(now.to_rfc3339())
        .bind(experiment_id)
        .execute(&self.pool)
        .await
        .with_context(|| format!("set_result experiment_id={experiment_id}"))?;

        if rows.rows_affected() == 0 {
            return Err(anyhow::anyhow!("experiment not found: {experiment_id}"));
        }

        self.get(experiment_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("experiment disappeared after set_result: {experiment_id}"))
    }

    /// Bind an `eval_batches.batch_id` to this experiment.
    pub async fn bind_batch(
        &self,
        experiment_id: &str,
        batch_id: &str,
    ) -> Result<Experiment> {
        self.update(
            experiment_id,
            ExperimentMutations {
                batch_id: Some(batch_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }
}

// ── Internal DB row type ────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ExperimentRow {
    experiment_id: String,
    name: String,
    question: Option<String>,
    strategy_ids: String,
    scenario_ids: String,
    batch_id: Option<String>,
    decision_budget: Option<i64>,
    result_json: Option<String>,
    conclusion: Option<String>,
    next_recommendation: Option<String>,
    created_at: String,
    updated_at: String,
}

fn row_to_experiment(row: ExperimentRow) -> Result<Experiment> {
    let strategy_ids: Vec<String> =
        serde_json::from_str(&row.strategy_ids).context("parse strategy_ids JSON")?;
    let scenario_ids: Vec<String> =
        serde_json::from_str(&row.scenario_ids).context("parse scenario_ids JSON")?;
    let result_json: Option<serde_json::Value> = row
        .result_json
        .map(|s| serde_json::from_str(&s).context("parse result_json"))
        .transpose()?;

    Ok(Experiment {
        experiment_id: row.experiment_id,
        name: row.name,
        question: row.question,
        strategy_ids,
        scenario_ids,
        batch_id: row.batch_id,
        decision_budget: row.decision_budget,
        result_json,
        conclusion: row.conclusion,
        next_recommendation: row.next_recommendation,
        created_at: row.created_at.parse().context("parse created_at")?,
        updated_at: row.updated_at.parse().context("parse updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Actor, ApiContext};

    async fn fresh_ctx() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        (ctx, dir)
    }

    fn sample_request(name: &str) -> CreateExperimentRequest {
        CreateExperimentRequest {
            name: name.to_string(),
            question: Some("Does compression-breakout outperform buy-and-hold?".to_string()),
            strategy_ids: vec!["strat-01".to_string(), "strat-02".to_string()],
            scenario_ids: vec!["sc-01".to_string()],
            decision_budget: Some(100),
        }
    }

    #[tokio::test]
    async fn create_and_get_experiment() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let exp = store.create(sample_request("Breakout Test")).await.unwrap();
        assert!(exp.experiment_id.starts_with("exp_"), "id prefix: {}", exp.experiment_id);
        assert_eq!(exp.name, "Breakout Test");
        assert_eq!(
            exp.question.as_deref(),
            Some("Does compression-breakout outperform buy-and-hold?")
        );
        assert_eq!(exp.strategy_ids, vec!["strat-01", "strat-02"]);
        assert_eq!(exp.scenario_ids, vec!["sc-01"]);
        assert!(exp.batch_id.is_none());
        assert_eq!(exp.decision_budget, Some(100));

        // Round-trip via get
        let fetched = store.get(&exp.experiment_id).await.unwrap().unwrap();
        assert_eq!(fetched.experiment_id, exp.experiment_id);
        assert_eq!(fetched.name, "Breakout Test");
        assert_eq!(fetched.strategy_ids, vec!["strat-01", "strat-02"]);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let result = store.get("exp_nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_returns_all_most_recent_first() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let _a = store.create(sample_request("Alpha")).await.unwrap();
        // Small sleep to ensure different created_at timestamps
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let b = store.create(sample_request("Beta")).await.unwrap();

        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 2);
        // Most recent first
        assert_eq!(list[0].experiment_id, b.experiment_id, "most recent first");
    }

    #[tokio::test]
    async fn update_conclusion_and_next_recommendation() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let exp = store.create(sample_request("Update Test")).await.unwrap();
        let updated = store
            .update(
                &exp.experiment_id,
                ExperimentMutations {
                    conclusion: Some("Strategy beats baseline by 3.2%".to_string()),
                    next_recommendation: Some("Test on ETH/USD next".to_string()),
                    batch_id: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(
            updated.conclusion.as_deref(),
            Some("Strategy beats baseline by 3.2%")
        );
        assert_eq!(
            updated.next_recommendation.as_deref(),
            Some("Test on ETH/USD next")
        );
        // Unchanged fields preserved
        assert_eq!(updated.name, "Update Test");
        assert!(updated.batch_id.is_none());
    }

    #[tokio::test]
    async fn update_nonexistent_returns_error() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let result = store
            .update(
                "exp_nonexistent",
                ExperimentMutations {
                    conclusion: Some("test".to_string()),
                    ..Default::default()
                },
            )
            .await;
        assert!(result.is_err(), "updating nonexistent experiment must fail");
    }

    #[tokio::test]
    async fn bind_batch_attaches_batch_id() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let exp = store.create(sample_request("Bind Test")).await.unwrap();
        assert!(exp.batch_id.is_none());

        // First seed an eval_batch so the FK is satisfied
        sqlx::query(
            "INSERT INTO eval_batches (batch_id, strategy_id, review_with, created_at, completed_at, status) \
             VALUES ('batch_test_001', 'strat-01', NULL, ?, NULL, 'running')",
        )
        .bind(Utc::now().to_rfc3339())
        .execute(&ctx.db)
        .await
        .unwrap();

        let bound = store
            .bind_batch(&exp.experiment_id, "batch_test_001")
            .await
            .unwrap();
        assert_eq!(bound.batch_id.as_deref(), Some("batch_test_001"));
    }

    #[tokio::test]
    async fn set_result_stores_json_blob() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let exp = store.create(sample_request("Result Test")).await.unwrap();
        let result_payload = serde_json::json!({
            "total_return_pct": 12.5,
            "sharpe": 1.8,
            "n_decisions": 200
        });

        let updated = store
            .set_result(&exp.experiment_id, result_payload.clone())
            .await
            .unwrap();

        let stored = updated.result_json.unwrap();
        assert_eq!(stored["total_return_pct"], 12.5);
        assert_eq!(stored["sharpe"], 1.8);
    }

    #[tokio::test]
    async fn create_experiment_without_question_or_budget() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let exp = store
            .create(CreateExperimentRequest {
                name: "Minimal".to_string(),
                question: None,
                strategy_ids: vec!["strat-x".to_string()],
                scenario_ids: vec!["sc-x".to_string()],
                decision_budget: None,
            })
            .await
            .unwrap();

        assert!(exp.question.is_none());
        assert!(exp.decision_budget.is_none());

        let fetched = store.get(&exp.experiment_id).await.unwrap().unwrap();
        assert!(fetched.question.is_none());
    }

    #[tokio::test]
    async fn partial_update_noop_does_not_change_fields() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = ExperimentStore::new(ctx.db.clone());

        let mut req = sample_request("Noop Test");
        req.question = Some("original question".to_string());
        let exp = store.create(req).await.unwrap();

        // Update with all-None mutations (only updated_at changes)
        let after = store
            .update(&exp.experiment_id, ExperimentMutations::default())
            .await
            .unwrap();

        assert_eq!(after.name, "Noop Test");
        assert_eq!(after.question.as_deref(), Some("original question"));
        assert!(after.conclusion.is_none());
    }
}
