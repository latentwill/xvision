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
use xvision_engine::api::{scenario as api_scenario, ApiContext, ApiError};
use xvision_engine::eval::findings::Finding;
use xvision_engine::eval::review::{self, AgentProfile, EvalReview, ReviewScenarioSummary, ReviewStatus};
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

    // Idempotency: only short-circuit on a Completed or in-flight
    // (Queued / Running) review. A prior Failed row is retry-eligible —
    // returning it here would make transient dispatch errors sticky.
    if !body.force {
        if let Some(existing) = find_reusable_review(&store, &run_id, &body.agent_profile_id).await? {
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

    // Resolve scenario metadata so the review payload carries asset /
    // granularity / time-window context. The engine treats this as
    // optional, but the engine docstring asks the API layer to provide
    // it when available — without this every review is run on a payload
    // that omits the scenario block entirely.
    let scenario_summary = resolve_scenario_summary(&ctx, &run_id).await;

    let outcome = review::run_review(&store, dispatch, &run_id, &profile.id, scenario_summary, None)
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

/// Find a reusable prior review for this (run, profile) pair. We
/// consider a review reusable when it is `Completed` (success or
/// `Inconclusive`) or in-flight (`Queued` / `Running`); `Failed` rows
/// are skipped so a transient dispatch error doesn't permanently pin
/// the operator to the failure.
async fn find_reusable_review(
    store: &RunStore,
    run_id: &str,
    profile_id: &str,
) -> Result<Option<EvalReview>, DashboardError> {
    let all = store
        .list_reviews_for_run(run_id)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    Ok(all
        .into_iter()
        .find(|r| r.agent_profile_id == profile_id && !matches!(r.status, ReviewStatus::Failed)))
}

/// Resolve `(run.scenario_id → ReviewScenarioSummary)` so the review
/// payload carries scenario context. Returns `None` silently when the
/// scenario row is missing or the run lookup fails — we don't want a
/// scenario-resolution hiccup to take down the review request itself.
async fn resolve_scenario_summary(ctx: &ApiContext, run_id: &str) -> Option<ReviewScenarioSummary> {
    let store = RunStore::new(ctx.db.clone());
    let run = match store.get(run_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(run_id, error = %e, "scenario summary: run lookup failed");
            return None;
        }
    };
    let scenario = match api_scenario::get(ctx, &run.scenario_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(
                scenario_id = %run.scenario_id,
                error = %e,
                "scenario summary: scenario lookup failed",
            );
            return None;
        }
    };
    Some(ReviewScenarioSummary {
        id: scenario.id.clone(),
        name: Some(scenario.display_name.clone()),
        // Scenarios are asset-free; a run is multi-asset and the per-decision
        // asset is the source of truth for per-asset review, so a single
        // run-level asset is no longer meaningful.
        asset: None,
        granularity: None,
        start: Some(scenario.time_window.start.to_rfc3339()),
        end: Some(scenario.time_window.end.to_rfc3339()),
    })
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
        ReviewError::Dispatch(m) => {
            DashboardError::from(ApiError::Internal(format!("review dispatch failed: {m}")))
        }
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

    if let Some(entry) = cfg.providers.iter().find(|p| p.name == profile.provider) {
        return dispatch_from_provider(entry).map_err(DashboardError::from);
    }

    // Same-kind substitution: if the operator named their Anthropic key
    // something other than "anthropic" (e.g. "anthropic-prod"), we can
    // still dispatch the seeded profile's Anthropic model id against it
    // because the wire format matches. Cross-kind substitution does NOT
    // work — the seeded research/reasoning/risk/fast-trader profiles
    // (migration 016) carry Anthropic model ids like `claude-sonnet-4-6`
    // which an OpenAI-compatible endpoint would 404 on. See
    // qa-review-agent-provider-config contract for the chosen path.
    let requested_kind = inferred_kind_for_provider_name(&profile.provider);
    if let Some(kind) = requested_kind {
        if let Some(entry) = cfg.providers.iter().find(|p| p.kind == kind) {
            tracing::warn!(
                profile_id = %profile.id,
                requested_provider = %profile.provider,
                substituted_provider = %entry.name,
                provider_kind = ?kind,
                "agent profile's named provider not configured; substituting same-kind provider",
            );
            return dispatch_from_provider(entry).map_err(DashboardError::from);
        }
    }

    // No exact match and no kind-compatible substitute. Return a clearer
    // skip-with-remediation error so the operator knows what to add.
    // We deliberately do NOT cross-kind substitute: sending the seeded
    // profile's Anthropic model id to an OpenAI-compatible endpoint
    // would fail at the wire layer.
    let configured = if cfg.providers.is_empty() {
        "none".to_string()
    } else {
        cfg.providers
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(DashboardError::from(ApiError::Validation(format!(
        "review skipped: agent profile `{}` requires provider `{}` which is not configured in Settings → Providers (configured: {}). Add a compatible provider to run this review.",
        profile.id, profile.provider, configured,
    ))))
}

/// Map well-known provider names to their `ProviderKind` so the resolver
/// can substitute across configured providers of the same kind without
/// sending a model id to a wire format that can't serve it. Returns
/// `None` for operator-defined provider names we don't recognize —
/// callers fall through to the skip-with-error path rather than guessing.
fn inferred_kind_for_provider_name(name: &str) -> Option<ProviderKind> {
    match name {
        "anthropic" => Some(ProviderKind::Anthropic),
        "openai" | "openai-compat" | "openrouter" => Some(ProviderKind::OpenaiCompat),
        "vllm" => Some(ProviderKind::Vllm),
        "local-candle" => Some(ProviderKind::LocalCandle),
        _ => None,
    }
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
    let no_auth_review = matches!(
        entry.kind,
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm
    );
    if api_key.is_empty() && !no_auth_review {
        return Err(ApiError::Validation(format!(
            "provider `{}` has no API key set",
            entry.name
        )));
    }
    Ok(match entry.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicDispatch::new(api_key)),
        ProviderKind::OpenaiCompat | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::Ollama => {
            let base = entry.base_url.trim_end_matches('/');
            let url = if base.ends_with("/v1") {
                base.to_string()
            } else {
                format!("{base}/v1")
            };
            Arc::new(OpenaiCompatDispatch::new(url, api_key))
        }
        ProviderKind::LocalCandle => Arc::new(MockDispatch::echo(
            r#"{"summary":"local-candle stub","verdict":"inconclusive","confidence":0.0,"score":0,"findings":[],"risks":[],"next_tests":[],"questions":[]}"#,
        )),
    })
}

fn runtime_config_path(ctx: &ApiContext) -> std::path::PathBuf {
    xvision_core::config::runtime_config_path(&ctx.xvn_home)
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
        fresh_state_with_providers(
            "\n[[providers]]\nname = \"anthropic\"\nkind = \"local-candle\"\nbase_url = \"\"\napi_key_env = \"\"\n",
        )
        .await
    }

    /// Build a fresh AppState with EXACTLY the operator-provided
    /// `[[providers]]` TOML (or none, if empty). Lets tests exercise the
    /// provider-resolution branches in `build_dispatch_for_profile`.
    ///
    /// The workspace `config/default.toml` ships with starter providers
    /// (gemini, nous-research). Strip those before appending `providers_toml`
    /// so `""` genuinely yields a zero-provider config — otherwise the
    /// "no providers configured" path can never be tested.
    async fn fresh_state_with_providers(providers_toml: &str) -> (AppState, TempDir) {
        let tmp = TempDir::new().unwrap();
        let xvn_home = tmp.path().to_path_buf();
        std::fs::create_dir_all(xvn_home.join("config")).unwrap();
        let base =
            std::fs::read_to_string("../../config/default.toml").expect("read workspace config/default.toml");
        let mut cfg = strip_provider_blocks(&base);
        cfg.push_str(providers_toml);
        std::fs::write(xvn_home.join("config/default.toml"), cfg).unwrap();
        let state = AppState::new(xvn_home).await.expect("AppState::new");
        (state, tmp)
    }

    /// Remove every `[[providers]]` array-of-tables block from a TOML string.
    /// A block runs from its `[[providers]]` header to the next top-level
    /// section header (`[` at column 0) or EOF.
    fn strip_provider_blocks(toml: &str) -> String {
        let mut out = String::new();
        let mut in_provider = false;
        for line in toml.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("[[providers]]") {
                in_provider = true;
                continue;
            }
            if in_provider {
                // A new top-level table / array-of-tables ends the block.
                if line.starts_with('[') {
                    in_provider = false;
                } else {
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    async fn seed_completed_run(pool: &SqlitePool) -> String {
        // ApiContext::open already seeded canonical scenarios into this
        // pool; pick the first one for the run's FK + the resolver
        // tests (which need a body_json that parses as a real Scenario).
        let scenario_id = xvision_engine::eval::scenario_seed::canonical_seed_rows()
            .into_iter()
            .next()
            .expect("at least one canonical scenario")
            .id;
        let store = RunStore::new(pool.clone());
        let run = Run::new_queued("agent-1".into(), scenario_id.clone(), RunMode::Backtest);
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
                    baselines: None,
                    ..Default::default()
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
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
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
        let force_body = serde_json::json!({"agent_profile_id": "reasoning-agent", "force": true});
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
    async fn post_review_resolves_scenario_summary_from_db() {
        // The dashboard route enriches the engine call with scenario
        // metadata so the review payload's `scenario` block isn't empty.
        // We verify the resolver directly because the route's effect on
        // the payload isn't observable from outside the engine.
        let (_server, _tmp, state) = boot().await;
        let run_id = seed_completed_run(&state.pool).await;
        let ctx = state.api_context();

        // Walk the steps the resolver walks so a failure here points at
        // the exact layer that broke (run lookup vs scenario lookup vs
        // body_json round-trip) rather than at the bare `None` the
        // resolver returns.
        let store = RunStore::new(ctx.db.clone());
        let run = store
            .get(&run_id)
            .await
            .expect("seeded run should be retrievable");
        let scenario = api_scenario::get(&ctx, &run.scenario_id)
            .await
            .expect("seeded scenario should deserialize");
        assert!(!scenario.id.is_empty());

        let summary = resolve_scenario_summary(&ctx, &run_id)
            .await
            .expect("seeded canonical scenario should resolve");
        // id and name come from the canonical scenario; granularity +
        // window come from the parsed body_json.
        assert_eq!(summary.id, scenario.id);
        assert_eq!(summary.name.as_deref(), Some(scenario.display_name.as_str()));
        assert!(summary.granularity.is_some(), "granularity should be set");
        assert!(summary.start.is_some(), "time_window.start should be set");
        assert!(summary.end.is_some(), "time_window.end should be set");
    }

    #[tokio::test]
    async fn resolve_scenario_summary_returns_none_for_unknown_run() {
        // Missing run → None, no panic. Guards the "scenario-resolution
        // hiccup shouldn't take down the review request" contract.
        let (_server, _tmp, state) = boot().await;
        let ctx = state.api_context();
        assert!(resolve_scenario_summary(&ctx, "does-not-exist").await.is_none());
    }

    #[tokio::test]
    async fn idempotency_skips_failed_reviews_and_runs_fresh_attempt() {
        let (server, _tmp, state) = boot().await;
        let run_id = seed_completed_run(&state.pool).await;
        let store = RunStore::new(state.pool.clone());

        // Pre-seed a Failed review for this (run, profile) pair, then
        // POST without --force. The route must NOT return the failed
        // row; it must run a fresh review.
        let mut failed = xvision_engine::eval::review::EvalReview::new_queued(
            run_id.clone(),
            "reasoning-agent".to_string(),
        );
        failed.status = ReviewStatus::Failed;
        failed.error = Some("synthetic prior failure".into());
        store.create_review(&failed).await.unwrap();
        // Bump it to failed via the typed update path so timestamps
        // reflect the transition.
        store
            .fail_review(&failed.id, "synthetic prior failure")
            .await
            .unwrap();

        let resp: serde_json::Value = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&serde_json::json!({"agent_profile_id": "reasoning-agent"}))
            .await
            .json();
        let new_id = resp["review"]["id"].as_str().unwrap();
        assert_ne!(
            new_id, failed.id,
            "must run a fresh review instead of returning the failed row"
        );
        assert_eq!(resp["review"]["verdict"].as_str(), Some("inconclusive"));
    }

    #[tokio::test]
    async fn list_reviews_returns_newest_first() {
        let (server, _tmp, state) = boot().await;
        let run_id = seed_completed_run(&state.pool).await;
        let body = serde_json::json!({"agent_profile_id": "reasoning-agent", "force": true});
        // Two forced reviews so we have ordering to assert on.
        for _ in 0..2 {
            server
                .post(&format!("/api/eval/runs/{run_id}/review"))
                .json(&body)
                .await
                .assert_status_ok();
        }
        let resp = server.get(&format!("/api/eval/runs/{run_id}/reviews")).await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        // store::list_reviews_for_run orders by created_at DESC.
        let t0 = items[0]["created_at"].as_str().unwrap();
        let t1 = items[1]["created_at"].as_str().unwrap();
        assert!(t0 >= t1, "newest first ({t0} vs {t1})");
    }

    // --- qa-review-agent-provider-config regression coverage ---

    /// Same-kind substitution: an Anthropic provider named anything
    /// OTHER than "anthropic" (e.g. an operator's `anthropic-prod`
    /// key) resolves the seeded `anthropic`-pinned profile because
    /// the wire format and model ids match across providers of the
    /// same `ProviderKind`. We use a stub API key to get past
    /// `dispatch_from_provider`'s api-key gate; the actual provider
    /// HTTP call would then fail authentication, but that's downstream
    /// of the substitution path we're asserting on.
    #[tokio::test]
    async fn post_review_substitutes_same_kind_provider_with_different_name() {
        // SAFETY: env var mutation is process-global; this key is
        // namespaced and other tests in this file don't read it.
        std::env::set_var("QA_REVIEW_SUBSTITUTE_TEST_KEY", "fake-key-for-substitution-test");
        let (state, _tmp) = fresh_state_with_providers(
            "\n[[providers]]\nname = \"anthropic-prod\"\nkind = \"anthropic\"\nbase_url = \"https://api.anthropic.com\"\napi_key_env = \"QA_REVIEW_SUBSTITUTE_TEST_KEY\"\n",
        )
        .await;
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let run_id = seed_completed_run(&state.pool).await;

        let resp = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&serde_json::json!({"agent_profile_id": "reasoning-agent"}))
            .await;
        let body_text = resp.text();
        // The point: the substitution PATH was taken — we did NOT
        // get the skip-with-remediation Validation error. The
        // downstream Anthropic call fails auth (fake key), which
        // surfaces as a persisted Failed review or 5xx, but never as
        // the "review skipped" Validation copy.
        assert!(
            !body_text.contains("review skipped"),
            "same-kind substitution should not skip; got: {body_text}"
        );
    }

    /// Cross-kind substitution is REFUSED. If the only configured
    /// provider has a different `ProviderKind` than the profile's
    /// requested provider, the resolver must NOT substitute it —
    /// dispatching an Anthropic model id to an OpenAI-compatible
    /// endpoint would 404 at the wire layer. Instead, return a clear
    /// skip-with-remediation error.
    #[tokio::test]
    async fn post_review_does_not_cross_kind_substitute() {
        let (state, _tmp) = fresh_state_with_providers(
            "\n[[providers]]\nname = \"openrouter\"\nkind = \"openai-compat\"\nbase_url = \"https://openrouter.ai/api/v1\"\napi_key_env = \"OPENROUTER_KEY\"\n",
        )
        .await;
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let run_id = seed_completed_run(&state.pool).await;

        let resp = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&serde_json::json!({"agent_profile_id": "reasoning-agent"}))
            .await;
        let body_text = resp.text();
        assert!(
            body_text.contains("review skipped"),
            "should refuse cross-kind substitution, got: {body_text}"
        );
        assert!(
            body_text.contains("openrouter"),
            "skip message should list configured providers, got: {body_text}"
        );
        assert!(
            body_text.contains("anthropic"),
            "skip message should name the requested provider so operator knows what to add, got: {body_text}"
        );
    }

    /// When NO providers are configured, the resolver returns a
    /// skip-with-remediation error listing `configured: none` rather
    /// than the older "anthropic not configured" line.
    #[tokio::test]
    async fn post_review_returns_clearer_error_when_no_providers() {
        let (state, _tmp) = fresh_state_with_providers("").await;
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let run_id = seed_completed_run(&state.pool).await;

        let resp = server
            .post(&format!("/api/eval/runs/{run_id}/review"))
            .json(&serde_json::json!({"agent_profile_id": "reasoning-agent"}))
            .await;
        let body_text = resp.text();
        assert!(
            body_text.contains("review skipped"),
            "expected skip-with-remediation, got: {body_text}"
        );
        assert!(
            body_text.contains("configured: none"),
            "should list configured providers (none), got: {body_text}"
        );
    }

    /// Unit test for the kind inference helper used to gate same-kind
    /// substitution. Unknown names return None so callers fall through
    /// to the skip-with-error path rather than guessing.
    #[test]
    fn inferred_kind_for_provider_name_handles_known_and_unknown() {
        assert_eq!(
            inferred_kind_for_provider_name("anthropic"),
            Some(ProviderKind::Anthropic)
        );
        assert_eq!(
            inferred_kind_for_provider_name("openrouter"),
            Some(ProviderKind::OpenaiCompat)
        );
        assert_eq!(
            inferred_kind_for_provider_name("local-candle"),
            Some(ProviderKind::LocalCandle)
        );
        assert_eq!(inferred_kind_for_provider_name("operator-custom"), None);
    }
}
