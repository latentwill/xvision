//! Strategy bundle operations. Backed by the existing filesystem bundle
//! store from Plan #1 (`xvision-engine/src/bundle/store.rs`). Every function
//! records to `api_audit` via `audit::record` on completion.

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiError, ApiResult,
};
use crate::bundle::{
    store::{BundleStore, FilesystemStore},
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
    let store = FilesystemStore::new(ctx.xvn_home.join("bundles"));
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
    let store = FilesystemStore::new(ctx.xvn_home.join("bundles"));
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
