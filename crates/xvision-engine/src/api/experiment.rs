//! Experiment-ledger API dispatch.
//!
//! Exposes CRUD operations over the `experiments` table (migration 022).
//! An experiment groups a research question with a set of strategies +
//! scenarios and optionally binds to an `eval_batches` row when run.
//!
//! Public surface:
//! - `create_experiment` — insert a new experiment row
//! - `get_experiment` — load by id with linked batch metadata
//! - `list_experiments` — list all, most-recent first
//! - `update_experiment` — apply partial mutations (conclusion, next_recommendation, batch_id)

use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::experiment_store::{
    CreateExperimentRequest as StoreCreateRequest, Experiment, ExperimentMutations, ExperimentStore,
};

// ── Request / response types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateExperimentRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// At least one strategy id is required.
    pub strategy_ids: Vec<String>,
    /// At least one scenario id is required.
    pub scenario_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_budget: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListExperimentsRequest {
    // Reserved for future filter fields (e.g. strategy_id filter).
    // Currently a no-op; all experiments are returned.
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateExperimentRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_recommendation: Option<String>,
    /// Bind this experiment to an existing eval batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
}

/// Detailed view of an experiment including the bound batch (if any).
/// Currently identical to `Experiment`; the separate type leaves room to
/// embed linked batch metadata in a future iteration without breaking the
/// wire shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentDetail {
    #[serde(flatten)]
    pub experiment: Experiment,
}

// ── API functions ────────────────────────────────────────────────────────────

/// Create a new experiment in the ledger.
///
/// Validates that `strategy_ids` and `scenario_ids` are non-empty.
/// Does NOT validate that the referenced strategy/scenario ids actually exist
/// in their respective stores — that check belongs at batch-run time.
pub async fn create_experiment(ctx: &ApiContext, req: CreateExperimentRequest) -> ApiResult<Experiment> {
    if req.strategy_ids.is_empty() {
        return Err(ApiError::Validation(
            "strategy_ids must contain at least one strategy id".to_string(),
        ));
    }
    if req.scenario_ids.is_empty() {
        return Err(ApiError::Validation(
            "scenario_ids must contain at least one scenario id".to_string(),
        ));
    }
    if req.name.trim().is_empty() {
        return Err(ApiError::Validation("name must not be blank".to_string()));
    }

    let store = ExperimentStore::new(ctx.db.clone());
    let exp = store
        .create(StoreCreateRequest {
            name: req.name.trim().to_string(),
            question: req.question,
            strategy_ids: req.strategy_ids,
            scenario_ids: req.scenario_ids,
            decision_budget: req.decision_budget,
        })
        .await
        .map_err(ApiError::Other)?;

    Ok(exp)
}

/// Load a single experiment by id. Returns `ApiError::NotFound` when
/// the id is not in the ledger.
pub async fn get_experiment(ctx: &ApiContext, experiment_id: &str) -> ApiResult<ExperimentDetail> {
    let store = ExperimentStore::new(ctx.db.clone());
    let exp = store
        .get(experiment_id)
        .await
        .map_err(ApiError::Other)?
        .ok_or_else(|| ApiError::NotFound(format!("experiment not found: {experiment_id}")))?;

    Ok(ExperimentDetail { experiment: exp })
}

/// List all experiments, most-recent first.
pub async fn list_experiments(ctx: &ApiContext, _req: ListExperimentsRequest) -> ApiResult<Vec<Experiment>> {
    let store = ExperimentStore::new(ctx.db.clone());
    store.list().await.map_err(ApiError::Other)
}

/// Apply partial mutations to an existing experiment.
pub async fn update_experiment(
    ctx: &ApiContext,
    experiment_id: &str,
    req: UpdateExperimentRequest,
) -> ApiResult<Experiment> {
    let store = ExperimentStore::new(ctx.db.clone());
    store
        .update(
            experiment_id,
            ExperimentMutations {
                conclusion: req.conclusion,
                next_recommendation: req.next_recommendation,
                batch_id: req.batch_id,
            },
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                ApiError::NotFound(format!("experiment not found: {experiment_id}"))
            } else {
                ApiError::Other(e)
            }
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

    fn sample_create(name: &str) -> CreateExperimentRequest {
        CreateExperimentRequest {
            name: name.to_string(),
            question: Some("Which regime benefits compression-breakout?".to_string()),
            strategy_ids: vec!["strat-alpha".to_string()],
            scenario_ids: vec!["sc-bull-2024".to_string()],
            decision_budget: Some(50),
        }
    }

    #[tokio::test]
    async fn create_returns_experiment_with_generated_id() {
        let (ctx, _dir) = fresh_ctx().await;
        let exp = create_experiment(&ctx, sample_create("API Create Test"))
            .await
            .unwrap();
        assert!(exp.experiment_id.starts_with("exp_"));
        assert_eq!(exp.name, "API Create Test");
    }

    #[tokio::test]
    async fn create_rejects_empty_name() {
        let (ctx, _dir) = fresh_ctx().await;
        let req = CreateExperimentRequest {
            name: "   ".to_string(),
            question: None,
            strategy_ids: vec!["strat-x".to_string()],
            scenario_ids: vec!["sc-x".to_string()],
            decision_budget: None,
        };
        let err = create_experiment(&ctx, req).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn create_rejects_empty_strategy_ids() {
        let (ctx, _dir) = fresh_ctx().await;
        let req = CreateExperimentRequest {
            name: "Test".to_string(),
            question: None,
            strategy_ids: vec![],
            scenario_ids: vec!["sc-x".to_string()],
            decision_budget: None,
        };
        let err = create_experiment(&ctx, req).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn create_rejects_empty_scenario_ids() {
        let (ctx, _dir) = fresh_ctx().await;
        let req = CreateExperimentRequest {
            name: "Test".to_string(),
            question: None,
            strategy_ids: vec!["strat-x".to_string()],
            scenario_ids: vec![],
            decision_budget: None,
        };
        let err = create_experiment(&ctx, req).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn get_returns_not_found_for_missing_id() {
        let (ctx, _dir) = fresh_ctx().await;
        let err = get_experiment(&ctx, "exp_missing").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_returns_all_experiments() {
        let (ctx, _dir) = fresh_ctx().await;
        create_experiment(&ctx, sample_create("Exp 1")).await.unwrap();
        create_experiment(&ctx, sample_create("Exp 2")).await.unwrap();

        let list = list_experiments(&ctx, ListExperimentsRequest::default())
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn update_applies_conclusion() {
        let (ctx, _dir) = fresh_ctx().await;
        let exp = create_experiment(&ctx, sample_create("Update API Test"))
            .await
            .unwrap();

        let updated = update_experiment(
            &ctx,
            &exp.experiment_id,
            UpdateExperimentRequest {
                conclusion: Some("Significant alpha found.".to_string()),
                next_recommendation: Some("Backtest ETH.".to_string()),
                batch_id: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(updated.conclusion.as_deref(), Some("Significant alpha found."));
        assert_eq!(updated.next_recommendation.as_deref(), Some("Backtest ETH."));
    }

    #[tokio::test]
    async fn update_not_found_returns_not_found_error() {
        let (ctx, _dir) = fresh_ctx().await;
        let err = update_experiment(
            &ctx,
            "exp_ghost",
            UpdateExperimentRequest {
                conclusion: Some("test".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::NotFound(_)));
    }

    #[tokio::test]
    async fn get_experiment_detail_wraps_experiment() {
        let (ctx, _dir) = fresh_ctx().await;
        let exp = create_experiment(&ctx, sample_create("Detail Test"))
            .await
            .unwrap();

        let detail = get_experiment(&ctx, &exp.experiment_id).await.unwrap();
        assert_eq!(detail.experiment.experiment_id, exp.experiment_id);
        assert_eq!(detail.experiment.name, "Detail Test");
    }
}
