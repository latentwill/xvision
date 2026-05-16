//! Eval-review HTTP API.
//!
//! Three routes:
//!
//! * `POST /api/eval/runs/:id/review` — run a review by agent profile id.
//!   Body: `{ "agent_profile_id": "reasoning-agent", "force"?: bool }`.
//!   `force` re-runs even when a completed review for that (run, profile)
//!   pair already exists; without `force` an existing completed review is
//!   returned instead of generating a new one (idempotent against
//!   accidental double-clicks).
//! * `GET /api/eval/runs/:id/reviews` — list reviews for a run, newest
//!   first. Returns the `EvalReview` rows without findings; clients pull
//!   the detail route below for findings on demand.
//! * `GET /api/eval/reviews/:id` — single review plus normalized findings
//!   linked to that review id.
//!
//! Dispatch construction (provider → Arc<dyn LlmDispatch>) lives in this
//! file rather than the engine crate so we don't have to widen the
//! engine's public `api::eval` surface from inside this track's
//! allowed-paths scope. The shape mirrors `engine::api::eval::dispatch_from_provider`.
//!
//! Path note: this file lives at `routes/eval/review.rs` per the
//! contract's `allowed_paths`. The older `routes/eval_runs.rs` stays as
//! a flat sibling; a future track can pull it into `eval::runs` if the
//! directory layout becomes worth the churn.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_core::config::{self, ProviderEntry, ProviderKind};
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch, OpenaiCompatDispatch};
use xvision_engine::api::{ApiContext, ApiError};
use xvision_engine::eval::findings::Finding;
use xvision_engine::eval::review::{self, AgentProfile, EvalReview, ReviewStatus};
use xvision_engine::eval::store::RunStore;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct GenerateRequest {
    pub agent_profile_id: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Serialize)]
pub struct ReviewListResponse {
    pub items: Vec<EvalReview>,
}

/// Detail payload: the review row plus its normalized findings. The CLI
/// + dashboard surface read both in one call so we don't force the
/// frontend to chain requests for the common case.
#[derive(Debug, Serialize)]
pub struct ReviewDetailResponse {
    pub review: EvalReview,
    pub findings: Vec<Finding>,
}

pub async fn generate(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(body): Json<GenerateRequest>,
) -> Result<(StatusCode, Json<ReviewDetailResponse>), DashboardError> {
    let ctx = state.api_context();
    let store = RunStore::new(ctx.db.clone());

    // Idempotency: if a completed (or queued/running) review already
    // exists for this (run, profile) and `force` is not set, return it.
    if !body.force {
        if let Some(existing) = find_latest_review(&store, &run_id, &body.agent_profile_id).await? {
            let findings = store
                .read_findings_for_review(&existing.id)
                .await
                .map_err(|e| DashboardError::Internal(e))?;
            return Ok((
                StatusCode::OK,
                Json(ReviewDetailResponse {
                    review: existing,
                    findings,
                }),
            ));
        }
    }

    let profile = store
        .get_agent_profile(&body.agent_profile_id)
        .await
        .map_err(|e| DashboardError::Internal(e))?
        .ok_or_else(|| {
            DashboardError::from(ApiError::NotFound(format!(
                "agent profile `{}` not found",
                body.agent_profile_id
            )))
        })?;
    if !profile.enabled {
        return Err(DashboardError::from(ApiError::Validation(format!(
            "agent profile `{}` is disabled",
            body.agent_profile_id
        ))));
    }

    let dispatch = build_dispatch_for_profile(&ctx, &profile).await?;

    let outcome = review::run_review(&store, dispatch, &run_id, &profile.id, None)
        .await
        .map_err(map_review_error)?;

    // run_review already persisted the row. Re-fetch for the response so
    // the caller sees the canonical persisted shape (including timestamps
    // the engine stamped on update).
    let persisted = store
        .get_review(&outcome.review_id)
        .await
        .map_err(|e| DashboardError::Internal(e))?
        .ok_or_else(|| {
            DashboardError::Internal(anyhow::anyhow!(
                "review row vanished immediately after persist: {}",
                outcome.review_id
            ))
        })?;
    let findings = store
        .read_findings_for_review(&outcome.review_id)
        .await
        .map_err(|e| DashboardError::Internal(e))?;

    let status = if matches!(persisted.status, ReviewStatus::Failed) {
        // We persisted the row but the model side failed. Surface 502 so
        // the dashboard / CLI can distinguish "no review available" from
        // "request was invalid" — the body still carries the persisted
        // review id so the caller can audit the failure.
        StatusCode::BAD_GATEWAY
    } else {
        StatusCode::OK
    };

    Ok((
        status,
        Json(ReviewDetailResponse {
            review: persisted,
            findings,
        }),
    ))
}

pub async fn list_for_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<ReviewListResponse>, DashboardError> {
    let store = RunStore::new(state.api_context().db.clone());
    let items = store
        .list_reviews_for_run(&run_id)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    Ok(Json(ReviewListResponse { items }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReviewDetailResponse>, DashboardError> {
    let store = RunStore::new(state.api_context().db.clone());
    let review = store
        .get_review(&id)
        .await
        .map_err(|e| DashboardError::Internal(e))?
        .ok_or_else(|| DashboardError::from(ApiError::NotFound(format!("review `{id}` not found"))))?;
    let findings = store
        .read_findings_for_review(&id)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    Ok(Json(ReviewDetailResponse { review, findings }))
}

// --- helpers -----------------------------------------------------------

async fn find_latest_review(
    store: &RunStore,
    run_id: &str,
    profile_id: &str,
) -> Result<Option<EvalReview>, DashboardError> {
    let all = store
        .list_reviews_for_run(run_id)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    Ok(all.into_iter().find(|r| r.agent_profile_id == profile_id))
}

fn map_review_error(e: review::ReviewError) -> DashboardError {
    use review::ReviewError;
    match e {
        ReviewError::ProfileNotFound(m) => {
            DashboardError::from(ApiError::NotFound(format!("agent profile `{m}` not found")))
        }
        ReviewError::ProfileDisabled(m) => {
            DashboardError::from(ApiError::Validation(format!("agent profile `{m}` is disabled")))
        }
        ReviewError::RunNotCompleted(m) => DashboardError::from(ApiError::Validation(format!(
            "review requires a completed run, got status `{m}`"
        ))),
        ReviewError::Dispatch(m) => DashboardError::from(ApiError::Internal(format!(
            "review dispatch failed: {m}"
        ))),
        ReviewError::Db(e) => {
            // Engine routes "run not found" through the untyped Db
            // variant (see engine/review/engine.rs doc comment). Surface
            // it as 404 when we recognize the message; otherwise 500.
            let msg = format!("{e:#}");
            if msg.contains("run not found") {
                DashboardError::from(ApiError::NotFound(msg))
            } else {
                DashboardError::Internal(e)
            }
        }
    }
}

/// Build an Arc<dyn LlmDispatch> for a review profile, reading the
/// provider entry from `$XVN_HOME/config/default.toml`.
///
/// Mirrors `engine::api::eval::dispatch_from_provider` but reads the
/// provider name from the `AgentProfile.provider` column instead of the
/// strategy's slot. Lives here (not in the engine crate) so this track
/// doesn't have to widen the engine's public `api` surface.
async fn build_dispatch_for_profile(
    ctx: &ApiContext,
    profile: &AgentProfile,
) -> Result<Arc<dyn LlmDispatch>, DashboardError> {
    let cfg_path = runtime_config_path(ctx);
    let cfg = tokio::task::spawn_blocking(move || config::load_runtime(&cfg_path))
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("spawn_blocking: {e}")))?
        .map_err(|e| DashboardError::from(ApiError::Validation(format!("load config: {e}"))))?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == profile.provider)
        .ok_or_else(|| {
            DashboardError::from(ApiError::Validation(format!(
                "agent profile `{}` references provider `{}` which is not configured in Settings → Providers.",
                profile.id, profile.provider
            )))
        })?;
    dispatch_from_provider(entry).map_err(DashboardError::from)
}

fn dispatch_from_provider(entry: &ProviderEntry) -> Result<Arc<dyn LlmDispatch>, ApiError> {
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).map_err(|_| {
            ApiError::Validation(format!(
                "no API key for provider `{}` (env var {} is unset)",
                entry.name, entry.api_key_env
            ))
        })?
    };
    if api_key.is_empty() && entry.kind != ProviderKind::LocalCandle {
        return Err(ApiError::Validation(format!(
            "provider `{}` has no API key set",
            entry.name
        )));
    }
    Ok(match entry.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicDispatch::new(api_key)),
        ProviderKind::OpenaiCompat => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::LocalCandle => Arc::new(MockDispatch::echo(
            r#"{"summary":"local-candle stub","verdict":"inconclusive","confidence":0.0,"score":0,"findings":[],"risks":[],"next_tests":[],"questions":[]}"#,
        )),
    })
}

fn runtime_config_path(ctx: &ApiContext) -> std::path::PathBuf {
    if let Ok(p) = std::env::var("XVN_CONFIG_PATH") {
        if !p.is_empty() {
            return p.into();
        }
    }
    ctx.xvn_home.join("config").join("default.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use chrono::{TimeZone, Utc};
    use sqlx::SqlitePool;
    use tempfile::TempDir;
    use xvision_engine::eval::run::{MetricsSummary, Run, RunMode};
    use xvision_engine::eval::store::DecisionRow;

    async fn fresh_state() -> (AppState, TempDir) {
        let tmp = TempDir::new().unwrap();
        let xvn_home = tmp.path().to_path_buf();
        std::fs::create_dir_all(xvn_home.join("config")).unwrap();
        // Reuse the canonical workspace config and append a local-candle
        // provider so build_dispatch_for_profile resolves without
        // needing real API keys.
        let mut cfg = std::fs::read_to_string("../../config/default.toml")
            .expect("read workspace config/default.toml");
        cfg.push_str(
            "\n[[providers]]\nname = \"anthropic\"\nkind = \"local-candle\"\nbase_url = \"\"\napi_key_env = \"\"\n",
        );
        std::fs::write(xvn_home.join("config/default.toml"), cfg).unwrap();
        let state = AppState::new(xvn_home).await.expect("AppState::new");
        (state, tmp)
    }

    async fn seed_completed_run(pool: &SqlitePool) -> String {
        // Seed scenario for FK
        sqlx::query(
            "INSERT INTO scenarios (id, parent_scenario_id, source, display_name, description, body_json, created_at, created_by, archived_at) \
             VALUES (?, NULL, 'built', 'test', '', '{}', ?, 'test', NULL)",
        )
        .bind("sc-1")
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();

        let store = RunStore::new(pool.clone());
        let run = Run::new_queued("agent-1".into(), "sc-1".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store.begin_running(&run.id).await.unwrap();
        store
            .finalize(
                &run.id,
                &MetricsSummary {
                    total_return_pct: 5.0,
                    sharpe: 1.2,
                    max_drawdown_pct: -3.0,
                    win_rate: 0.55,
                    n_trades: 4,
                    n_decisions: 3,
                },
            )
            .await
            .unwrap();
        let t0 = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        for i in 0..3 {
            store
                .record_decision(&DecisionRow {
                    run_id: run.id.clone(),
                    decision_index: i,
                    timestamp: t0,
                    asset: "BTC-USD".into(),
                    action: "long_open".into(),
                    conviction: Some(0.7),
                    justification: None,
                    reasoning: None,
                    order_size: Some(0.01),
                    fill_price: Some(50_000.0),
                    fill_size: Some(0.01),
                    fee: Some(1.0),
                    pnl_realized: Some(0.0),
                })
                .await
                .unwrap();
            store
                .record_equity(&run.id, t0 + chrono::Duration::minutes(i as i64), 100_000.0)
                .await
                .unwrap();
        }
        run.id
    }

    async fn boot() -> (TestServer, TempDir, AppState) {
        let (state, tmp) = fresh_state().await;
        let server =
            TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        (server, tmp, state)
    }

    #[tokio::test]
    async fn post_review_persists_inconclusive_when_local_candle_returns_stub() {
        let (server, _tmp, state) = boot().await;
        let run_id = seed_completed_run(&state.pool).await;

        let resp = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&serde_json::json!({"agent_profile_id": "reasoning-agent"}))
            .await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        // Local-candle stub returns the inconclusive shape directly.
        assert_eq!(body["review"]["verdict"].as_str(), Some("inconclusive"));
        assert_eq!(body["findings"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn post_review_404s_on_unknown_profile() {
        let (server, _tmp, state) = boot().await;
        let run_id = seed_completed_run(&state.pool).await;

        let resp = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&serde_json::json!({"agent_profile_id": "ghost-agent"}))
            .await;
        resp.assert_status_not_found();
    }

    #[tokio::test]
    async fn post_review_is_idempotent_without_force() {
        let (server, _tmp, state) = boot().await;
        let run_id = seed_completed_run(&state.pool).await;
        let body = serde_json::json!({"agent_profile_id": "reasoning-agent"});

        let first: serde_json::Value = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&body)
            .await
            .json();
        let first_id = first["review"]["id"].as_str().unwrap().to_string();

        // Second POST without force returns the SAME review id.
        let second: serde_json::Value = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&body)
            .await
            .json();
        assert_eq!(second["review"]["id"].as_str().unwrap(), first_id);

        // Third POST WITH force creates a new id.
        let force_body =
            serde_json::json!({"agent_profile_id": "reasoning-agent", "force": true});
        let third: serde_json::Value = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&force_body)
            .await
            .json();
        assert_ne!(third["review"]["id"].as_str().unwrap(), first_id);
    }

    #[tokio::test]
    async fn get_review_returns_review_with_findings() {
        let (server, _tmp, state) = boot().await;
        let run_id = seed_completed_run(&state.pool).await;

        let post: serde_json::Value = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&serde_json::json!({"agent_profile_id": "reasoning-agent"}))
            .await
            .json();
        let review_id = post["review"]["id"].as_str().unwrap().to_string();

        let resp = server.get(&format!("/api/eval/reviews/{review_id}")).await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        assert_eq!(v["review"]["id"].as_str(), Some(review_id.as_str()));
        assert!(v["findings"].is_array());
    }

    #[tokio::test]
    async fn get_review_404s_for_unknown_id() {
        let (server, _tmp, _state) = boot().await;
        server
            .get("/api/eval/reviews/does-not-exist")
            .await
            .assert_status_not_found();
    }

    #[tokio::test]
    async fn list_reviews_returns_newest_first() {
        let (server, _tmp, state) = boot().await;
        let run_id = seed_completed_run(&state.pool).await;
        let body =
            serde_json::json!({"agent_profile_id": "reasoning-agent", "force": true});
        // Two forced reviews so we have ordering to assert on.
        for _ in 0..2 {
            server
                .post(&format!("/api/eval/runs/{run_id}/review"))
                .json(&body)
                .await
                .assert_status_ok();
        }
        let resp = server
            .get(&format!("/api/eval/runs/{run_id}/reviews"))
            .await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        // store::list_reviews_for_run orders by created_at DESC.
        let t0 = items[0]["created_at"].as_str().unwrap();
        let t1 = items[1]["created_at"].as_str().unwrap();
        assert!(t0 >= t1, "newest first ({t0} vs {t1})");
    }
}
