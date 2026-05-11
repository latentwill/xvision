//! Strategy bundle operations. Backed by the existing filesystem bundle
//! store from Plan #1 (`xvision-engine/src/bundle/store.rs`). Every function
//! records to `api_audit` via `audit::record` on completion.
//!
//! Read-only ops (`list`, `get`) ship today; the mutation surface
//! (`create_strategy`, `update_slot`, `set_risk_config`, `validate_draft`)
//! lands here as audit-emitting wrappers around the `engine::authoring::*`
//! dispatcher. The dashboard's Inspector route calls these; the MCP tool
//! layer (PR #31) goes through `engine::authoring::*` directly.

use crate::api::{
    audit::{self, Outcome},
    search as api_search, ApiContext, ApiError, ApiResult,
};
use crate::authoring::{
    self, CreateStrategyOut, CreateStrategyReq, SetRiskConfigOut, SetRiskConfigReq,
    UpdateSlotOut, UpdateSlotReq, ValidateDraftOut,
};
use crate::bundle::{
    store::{strategy_store_dir, BundleStore, FilesystemStore},
    StrategyBundle,
};
use std::time::Instant;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategySummary {
    pub agent_id: String,
    pub template: String,
}

pub async fn list(ctx: &ApiContext) -> ApiResult<Vec<StrategySummary>> {
    let started = Instant::now();
    let result = list_inner(ctx).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "list",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_inner(ctx: &ApiContext) -> ApiResult<Vec<StrategySummary>> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let ids = store
        .list()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let bundle = store
            .load(&id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        out.push(StrategySummary {
            agent_id: bundle.manifest.id,
            template: bundle.manifest.template,
        });
    }
    Ok(out)
}

pub async fn get(ctx: &ApiContext, agent_id: &str) -> ApiResult<StrategyBundle> {
    let started = Instant::now();
    let result = get_inner(ctx, agent_id).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "get",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn get_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<StrategyBundle> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    store.load(agent_id).await.map_err(|e| {
        if is_not_found(&e) {
            ApiError::NotFound(format!("strategy '{agent_id}'"))
        } else {
            ApiError::Internal(e.to_string())
        }
    })
}

fn is_not_found(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                return true;
            }
        }
    }
    false
}

/// Map an `anyhow::Error` from `engine::authoring::*` dispatcher fns to a
/// typed `ApiError`. The dispatcher emits validation failures as
/// `anyhow!("...")` strings (no typed enum), so we string-match the prefix
/// for the cases we want to surface as `Validation`. Anything else falls
/// through to `Internal`.
fn map_authoring_error(err: anyhow::Error, agent_id: Option<&str>) -> ApiError {
    if is_not_found(&err) {
        return match agent_id {
            Some(id) => ApiError::NotFound(format!("strategy '{id}'")),
            None => ApiError::NotFound(err.to_string()),
        };
    }
    let msg = err.to_string();
    let validation_markers = [
        "unknown slot",
        "no fields to update",
        "unknown preset",
        "preset and explicit are mutually exclusive",
        "supply either preset or explicit",
        "unknown template",
        "mechanical_params is not a JSON object",
    ];
    if validation_markers.iter().any(|m| msg.contains(m)) {
        return ApiError::Validation(msg);
    }
    ApiError::Internal(msg)
}

/// Create a new draft strategy from a template. Wraps
/// `authoring::create_strategy` with an audit row keyed on the resulting
/// `agent_id` (or no target on failure, since the id only exists on
/// success).
pub async fn create_strategy(
    ctx: &ApiContext,
    req: CreateStrategyReq,
) -> ApiResult<CreateStrategyOut> {
    let started = Instant::now();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::create_strategy(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, None));

    let (outcome, target) = match &result {
        Ok(o) => (Outcome::Ok, Some(o.id.clone())),
        Err(e) => (Outcome::Error(e.to_string()), None),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "create",
        target.as_deref(),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if let Some(id) = target.as_deref() {
        index_strategy_after_mutation(ctx, &store, id).await;
    }
    result
}

/// Update one or more fields on an LLM slot. Wraps `authoring::update_slot`.
pub async fn update_slot(ctx: &ApiContext, req: UpdateSlotReq) -> ApiResult<UpdateSlotOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::update_slot(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "update_slot",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Update the bundle's risk config — preset (Conservative / Balanced /
/// Aggressive) or an explicit `RiskConfig` blob, but not both.
pub async fn set_risk_config(
    ctx: &ApiContext,
    req: SetRiskConfigReq,
) -> ApiResult<SetRiskConfigOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_risk_config(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_risk_config",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Re-load the bundle after a successful mutation and refresh its row in
/// the search index. Best-effort: a failure here is logged inside
/// `api::search::upsert_strategy` and never bubbled up — the mutation has
/// already succeeded and the audit row is already written.
async fn index_strategy_after_mutation(
    ctx: &ApiContext,
    store: &FilesystemStore,
    agent_id: &str,
) {
    match store.load(agent_id).await {
        Ok(bundle) => api_search::upsert_strategy(ctx, &bundle).await,
        Err(e) => tracing::warn!(error = %e, agent_id, "post-mutation reload for indexer failed"),
    }
}

/// Run the bundle through the validator. The result type carries the
/// success/failure verdict + reasons; this wrapper only surfaces an
/// `ApiError` for hard load failures (NotFound / Internal). A validation
/// failure round-trips as `Ok(ValidateDraftOut { ok: false, errors })`.
pub async fn validate_draft(ctx: &ApiContext, agent_id: &str) -> ApiResult<ValidateDraftOut> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::validate_draft(&store, agent_id)
        .await
        .map_err(|e| map_authoring_error(e, Some(agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "validate",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use sqlx::SqlitePool;

    async fn ctx_with_audit() -> (ApiContext, tempfile::TempDir) {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(include_str!("../../migrations/001_api_audit.sql"))
            .execute(&pool)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(strategy_store_dir(dir.path())).unwrap();
        let ctx = ApiContext {
            db: pool,
            actor: Actor::Cli {
                user: "tester".into(),
            },
            xvn_home: dir.path().to_path_buf(),
        };
        (ctx, dir)
    }

    async fn audit_row_exists(ctx: &ApiContext, op: &str, target: &str) -> bool {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM api_audit WHERE operation = ?1 AND target = ?2",
        )
        .bind(op)
        .bind(target)
        .fetch_one(&ctx.db)
        .await
        .unwrap();
        n > 0
    }

    #[tokio::test]
    async fn create_strategy_round_trips_and_audits() {
        let (ctx, _d) = ctx_with_audit().await;
        let out = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "btc-mom".into(),
                creator: Some("@tester".into()),
            },
        )
        .await
        .unwrap();
        assert!(audit_row_exists(&ctx, "create", &out.id).await);
        let bundle = get(&ctx, &out.id).await.unwrap();
        assert_eq!(bundle.manifest.id, out.id);
    }

    #[tokio::test]
    async fn create_strategy_unknown_template_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "no-such-template".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await;
        assert!(
            matches!(r, Err(ApiError::Validation(_))),
            "expected Validation, got {r:?}",
        );
    }

    #[tokio::test]
    async fn update_slot_audits_and_returns_updated_fields() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = update_slot(
            &ctx,
            UpdateSlotReq {
                id: created.id.clone(),
                slot: "trader".into(),
                prompt: Some("New prompt.".into()),
                model_requirement: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(out.updated, vec!["prompt".to_string()]);
        assert!(audit_row_exists(&ctx, "update_slot", &created.id).await);
    }

    #[tokio::test]
    async fn update_slot_unknown_role_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let r = update_slot(
            &ctx,
            UpdateSlotReq {
                id: created.id,
                slot: "no-such-role".into(),
                prompt: Some("p".into()),
                model_requirement: None,
                allowed_tools: None,
            },
        )
        .await;
        assert!(matches!(r, Err(ApiError::Validation(_))));
    }

    #[tokio::test]
    async fn update_slot_missing_draft_is_not_found() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = update_slot(
            &ctx,
            UpdateSlotReq {
                id: "01TOTALLYMISSINGAGENTID000".into(),
                slot: "trader".into(),
                prompt: Some("p".into()),
                model_requirement: None,
                allowed_tools: None,
            },
        )
        .await;
        assert!(
            matches!(r, Err(ApiError::NotFound(_))),
            "expected NotFound, got {r:?}",
        );
    }

    #[tokio::test]
    async fn set_risk_config_preset_round_trips() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = set_risk_config(
            &ctx,
            SetRiskConfigReq {
                id: created.id.clone(),
                preset: Some("conservative".into()),
                explicit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(out.applied, "preset");
        assert!(audit_row_exists(&ctx, "set_risk_config", &created.id).await);
    }

    #[tokio::test]
    async fn set_risk_config_unknown_preset_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let r = set_risk_config(
            &ctx,
            SetRiskConfigReq {
                id: created.id,
                preset: Some("totally-not-a-preset".into()),
                explicit: None,
            },
        )
        .await;
        assert!(matches!(r, Err(ApiError::Validation(_))));
    }

    #[tokio::test]
    async fn validate_draft_audits_and_returns_ok_for_template_default() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = validate_draft(&ctx, &created.id).await.unwrap();
        // Default-from-template drafts may or may not validate cleanly
        // depending on what fields are required. We only assert the audit
        // row landed and the response carries the id.
        assert_eq!(out.id, created.id);
        assert!(audit_row_exists(&ctx, "validate", &created.id).await);
    }

    #[tokio::test]
    async fn validate_draft_missing_is_not_found() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = validate_draft(&ctx, "01TOTALLYMISSINGAGENTID000").await;
        assert!(matches!(r, Err(ApiError::NotFound(_))));
    }
}
