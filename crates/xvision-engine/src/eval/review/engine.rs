//! Run a review end-to-end.
//!
//! Pipeline:
//!
//! 1. Load run + decisions + equity curve from `RunStore`.
//! 2. Build the bounded payload (`payload::build_review_payload`).
//! 3. If the payload is too sparse to ground 3 findings → mark
//!    `inconclusive`, persist, return.
//! 4. Otherwise: render the system prompt + JSON contract, dispatch
//!    through the caller-provided `LlmDispatch` with low-temperature
//!    deterministic params from the agent profile, parse + validate the
//!    response.
//! 5. On a clean parse → `complete_review` plus normalized findings.
//!    On an ungrounded / contract-violating parse → persist as
//!    `inconclusive` with the validator's reason on `summary`.
//!    On a transport / JSON-slice failure → `fail_review` with the
//!    transport error.
//!
//! The engine itself does **not** know how to construct an
//! `Arc<dyn LlmDispatch>` from a provider config; that's the API/CLI
//! track's responsibility (`api::eval::dispatch_from_provider`). Keeping
//! it injected makes the engine unit-testable with `MockDispatch`.

use std::sync::Arc;

use chrono::Utc;
use thiserror::Error;
use tracing::warn;
use ulid::Ulid;

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::eval::findings::{Finding, Severity};
use crate::eval::review::parser::{parse_review_output, ParsedReview};
use crate::eval::review::payload::{build_review_payload, ReviewPayload, ReviewScenarioSummary};
use crate::eval::review::prompt::{build_system_prompt, render_evidence_legend};
use crate::eval::review::{AgentProfile, EvalReview, ReviewStatus, ReviewVerdict};
use crate::eval::store::RunStore;

/// Hard ceiling on response size in tokens. The profile may request fewer
/// but never more, regardless of operator misconfiguration.
const HARD_MAX_TOKENS: u32 = 16_000;

/// Maximum input payload size in characters before we switch to compact
/// JSON (serde_json::to_string vs serde_json::to_string_pretty). Punts
/// overflow prevention to the serialization layer rather than guessing
/// token counts.
const COMPACT_JSON_THRESHOLD_CHARS: usize = 80_000;

/// What `run_review` returns. The persisted review id is always present
/// (even on `failed` outcomes) so callers can link to the audit row.
#[derive(Debug, Clone)]
pub struct ReviewOutcome {
    pub review_id: String,
    pub status: ReviewStatus,
    pub verdict: Option<ReviewVerdict>,
}

#[derive(Debug, Error)]
pub enum ReviewError {
    #[error("agent profile `{0}` not found")]
    ProfileNotFound(String),
    #[error("agent profile `{0}` is disabled")]
    ProfileDisabled(String),
    #[error("review run requires a completed eval run, got status `{0}`")]
    RunNotCompleted(String),
    /// Wraps any underlying store error, including the
    /// `RunStore::get` "run not found" string. We don't unwrap that into
    /// a typed `RunNotFound` here because the store's not-found path
    /// itself is untyped; a typed variant would only ever be reachable
    /// through brittle error-string parsing.
    #[error(transparent)]
    Db(#[from] anyhow::Error),
    #[error("llm dispatch failed: {0}")]
    Dispatch(String),
}

/// Orchestrate one review. Takes the store + dispatch + scenario summary
/// (the caller — usually the API layer — resolves scenario metadata).
///
/// When `custom_prompt` is `Some(...)`, it overrides the agent profile's
/// stored `system_prompt`. This lets callers supply ad-hoc review prompts
/// from the CLI without modifying persisted profiles.
///
/// Errors out only for setup-level problems (run not found, profile
/// disabled, DB down). Model-side failures (bad JSON, hallucinated
/// evidence, sparse payload) are persisted as terminal reviews and
/// returned as `Ok(ReviewOutcome { status: Failed | Completed-inconclusive })`.
pub async fn run_review(
    store: &RunStore,
    dispatch: Arc<dyn LlmDispatch>,
    run_id: &str,
    agent_profile_id: &str,
    scenario: Option<ReviewScenarioSummary>,
    custom_prompt: Option<String>,
) -> Result<ReviewOutcome, ReviewError> {
    let run = store
        .get(run_id)
        .await
        .map_err(|e| ReviewError::Db(e.context(format!("load run {run_id}"))))?;

    if !matches!(
        run.status,
        crate::eval::RunStatus::Completed
            | crate::eval::RunStatus::Failed
            | crate::eval::RunStatus::Cancelled
            | crate::eval::RunStatus::Disconnected
    ) {
        return Err(ReviewError::RunNotCompleted(format!("{:?}", run.status)));
    }

    let profile = store
        .get_agent_profile(agent_profile_id)
        .await
        .map_err(|e| ReviewError::Db(e.context(format!("load profile {agent_profile_id}"))))?
        .ok_or_else(|| ReviewError::ProfileNotFound(agent_profile_id.to_string()))?;
    if !profile.enabled {
        return Err(ReviewError::ProfileDisabled(agent_profile_id.to_string()));
    }

    // When a custom prompt was supplied, override the profile's stored
    // system_prompt so call_model uses the caller's text instead of the
    // operator-configured persona. The profile id / provider / model /
    // temperature all stay from the DB row.
    let profile = if let Some(ref custom) = custom_prompt {
        AgentProfile {
            system_prompt: custom.clone(),
            ..profile
        }
    } else {
        profile
    };

    // Persist queued, then advance to running. begin_review_running
    // returns false for a stale callback path; here we're the writer so a
    // false return means a bug.
    let review = EvalReview::new_queued(run.id.clone(), profile.id.clone());
    store
        .create_review(&review)
        .await
        .map_err(|e| ReviewError::Db(e.context("create eval_review")))?;
    let _ = store
        .begin_review_running(&review.id)
        .await
        .map_err(|e| ReviewError::Db(e.context("begin review running")))?;

    // Build the payload.
    let decisions = store
        .read_decisions(&run.id)
        .await
        .map_err(|e| ReviewError::Db(e.context("read decisions")))?;
    let equity = store
        .read_equity_curve(&run.id)
        .await
        .map_err(|e| ReviewError::Db(e.context("read equity curve")))?;
    let payload = build_review_payload(&run, decisions, equity, scenario, &profile);

    // Sparse payload short-circuits — never ask the model to invent findings.
    if payload.is_sparse {
        return persist_inconclusive(
            store,
            &review.id,
            "payload is sparse: no metrics, decisions, or equity samples were persisted for this run.",
            &payload,
        )
        .await;
    }

    // Dispatch through the model.
    let llm_response = match call_model(&dispatch, &profile, &payload).await {
        Ok(text) => text,
        Err(e) => {
            // Transport-level failure — keep the audit row but mark
            // failed so the caller (UI / CLI) can surface the error.
            let _ = store
                .fail_review(&review.id, &format!("dispatch error: {e}"))
                .await
                .map_err(|db| ReviewError::Db(db.context("fail review after dispatch error")))?;
            return Ok(ReviewOutcome {
                review_id: review.id,
                status: ReviewStatus::Failed,
                verdict: None,
            });
        }
    };

    // Parse + validate.
    let parsed = match parse_review_output(&llm_response, &payload) {
        Ok(p) => p,
        Err(e) => {
            warn!(review_id = %review.id, error = %e, "review parse failed; persisting as inconclusive");
            return persist_inconclusive(
                store,
                &review.id,
                &format!("review-output validation failed: {e}"),
                &payload,
            )
            .await;
        }
    };

    // If the verdict is inconclusive *with* zero findings, persist it as
    // such (do not synthesise). If the verdict is anything else, the
    // parser has already enforced 3..=10 findings.
    persist_completed(store, &review.id, parsed, &payload).await
}

async fn call_model(
    dispatch: &Arc<dyn LlmDispatch>,
    profile: &AgentProfile,
    payload: &ReviewPayload,
) -> Result<String, String> {
    let system_prompt = build_system_prompt(profile);
    let legend = render_evidence_legend(&payload.valid_evidence_refs);

    // Prefer pretty-printed JSON for model readability. If the
    // serialized payload is large enough to risk context overflow, send
    // compact JSON instead (saves ~30 % from whitespace alone).
    let pretty = serde_json::to_string_pretty(payload).map_err(|e| format!("serialize: {e}"))?;
    let (body_json, is_compact) = if pretty.len() > COMPACT_JSON_THRESHOLD_CHARS {
        let compact = serde_json::to_string(payload).map_err(|e| format!("serialize: {e}"))?;
        (compact, true)
    } else {
        (pretty, false)
    };

    let user_text = format!("{legend}\n\nReview payload:\n{body_json}");
    let req = LlmRequest {
        model: profile.model.clone(),
        system_prompt: system_prompt.clone(),
        messages: vec![Message::user_text(user_text)],
        max_tokens: Some(profile.max_tokens.min(HARD_MAX_TOKENS)),
        tools: vec![],
        temperature: Some(profile.temperature),
        response_schema: None,
        cache_control: None,
        force_json: true,
    };
    let resp = dispatch.complete(req).await;

    // On context overflow from the pretty-printed version, retry once
    // with compact JSON before giving up.
    if is_compact {
        return match resp {
            Ok(r) => Ok(r.text()),
            Err(e) => Err(format!("{e:#}")),
        };
    }
    match resp {
        Ok(r) => Ok(r.text()),
        Err(e) => {
            let msg = format!("{e:#}");
            if is_overflow_error(&msg) {
                // Retry with compact JSON.
                let compact =
                    serde_json::to_string(payload).map_err(|e| format!("serialize: {e}"))?;
                let user_text = format!("{legend}\n\nReview payload:\n{compact}");
                let req = LlmRequest {
                    model: profile.model.clone(),
                    system_prompt,
                    messages: vec![Message::user_text(user_text)],
                    max_tokens: Some(profile.max_tokens.min(HARD_MAX_TOKENS)),
                    tools: vec![],
                    temperature: Some(profile.temperature),
                    response_schema: None,
                    cache_control: None,
                    force_json: true,
                };
                let retry = dispatch.complete(req).await.map_err(|e| format!("{e:#}"))?;
                Ok(retry.text())
            } else {
                Err(msg)
            }
        }
    }
}

/// True when the error message indicates the model's context window was
/// exceeded. Matches the same markers used by
/// `agent::llm::body_indicates_context_overflow` and
/// `agent::recovery::FailureClass::ContextOverflow`.
fn is_overflow_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("context_length_exceeded")
        || lower.contains("prompt is too long")
        || lower.contains("context window")
        || lower.contains("context length exceeded")
        || lower.contains("max_tokens exceeded")
}

async fn persist_inconclusive(
    store: &RunStore,
    review_id: &str,
    reason: &str,
    payload: &ReviewPayload,
) -> Result<ReviewOutcome, ReviewError> {
    let raw_audit = serde_json::json!({
        "synthesized": true,
        "reason": reason,
        "payload_is_sparse": payload.is_sparse,
    })
    .to_string();
    let _ = store
        .complete_review(review_id, ReviewVerdict::Inconclusive, 0.0, 0, reason, &raw_audit)
        .await
        .map_err(|e| ReviewError::Db(e.context("persist inconclusive review")))?;
    Ok(ReviewOutcome {
        review_id: review_id.to_string(),
        status: ReviewStatus::Completed,
        verdict: Some(ReviewVerdict::Inconclusive),
    })
}

async fn persist_completed(
    store: &RunStore,
    review_id: &str,
    parsed: ParsedReview,
    payload: &ReviewPayload,
) -> Result<ReviewOutcome, ReviewError> {
    let verdict = ReviewVerdict::parse(&parsed.verdict).ok_or_else(|| {
        // Should already be guarded by parser, but keep a defensive
        // path so a parser bug can't ship malformed verdicts.
        ReviewError::Db(anyhow::anyhow!(
            "parser returned unknown verdict `{}`",
            parsed.verdict
        ))
    })?;

    let _ = store
        .complete_review_with_annotations(
            review_id,
            verdict,
            parsed.confidence,
            parsed.score,
            &parsed.summary,
            &parsed.raw_json,
            &parsed.annotations,
        )
        .await
        .map_err(|e| ReviewError::Db(e.context("complete review")))?;

    // Normalize each review finding into an `eval_findings` row.
    for finding in &parsed.findings {
        let normalized = normalize_finding(review_id, &payload.eval_run_id, finding)?;
        store
            .record_finding(&normalized)
            .await
            .map_err(|e| ReviewError::Db(e.context("record review finding")))?;
    }

    Ok(ReviewOutcome {
        review_id: review_id.to_string(),
        status: ReviewStatus::Completed,
        verdict: Some(verdict),
    })
}

fn normalize_finding(
    review_id: &str,
    run_id: &str,
    f: &crate::eval::review::parser::ReviewFinding,
) -> Result<Finding, ReviewError> {
    let severity = match f.severity.as_str() {
        "critical" => Severity::Critical,
        "high" | "medium" => Severity::Warning,
        _ => Severity::Info,
    };
    let evidence_payload = serde_json::Value::Array(
        f.evidence
            .iter()
            .map(|e| {
                serde_json::json!({
                    "kind": e.kind,
                    "reference": e.reference,
                })
            })
            .collect(),
    );
    let now = Utc::now();
    Ok(Finding {
        id: Ulid::new().to_string(),
        run_id: run_id.to_string(),
        kind: f.finding_type.clone(),
        severity,
        summary: f.title.clone(),
        evidence: evidence_payload,
        extracted_at: now,
        schema_version: "2".into(),
        evidence_cycle_ids: None,
        produced_by_check: Some("review_engine".into()),
        eval_review_id: Some(review_id.to_string()),
        review_type: Some(f.finding_type.clone()),
        confidence: Some(f.confidence),
        title: Some(f.title.clone()),
        description: Some(f.description.clone()),
        recommendation: Some(f.recommendation.clone()),
        created_at: Some(now),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{ContentBlock, LlmResponse, MockDispatch, StopReason};
    use crate::eval::run::{MetricsSummary, Run, RunMode};
    use crate::eval::store::DecisionRow;
    use chrono::{TimeZone, Utc};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open sqlite mem pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("apply migrations");
        // Seed a scenarios row so eval_runs.scenario_id can satisfy its FK.
        sqlx::query(
            "INSERT INTO scenarios (id, parent_scenario_id, source, display_name, description, \
                                    body_json, created_at, created_by, archived_at) \
             VALUES (?, NULL, 'built', 'test scenario', '', '{}', ?, 'test', NULL)",
        )
        .bind("sc-1")
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("seed scenario");
        pool
    }

    async fn seed_completed_run(store: &RunStore) -> Run {
        let mut run = Run::new_queued("agent-1".into(), "sc-1".into(), RunMode::Backtest);
        store.create(&run).await.expect("create run");
        store.begin_running(&run.id).await.expect("begin running");
        let metrics = MetricsSummary {
            total_return_pct: 5.0,
            sharpe: 1.2,
            max_drawdown_pct: -3.0,
            win_rate: 0.55,
            n_trades: 4,
            n_decisions: 3,
            baselines: None,
            ..Default::default()
        };
        store.finalize(&run.id, &metrics).await.expect("finalize");
        run.metrics = Some(metrics);
        run.status = crate::eval::RunStatus::Completed;

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
                    delayed: None,
                })
                .await
                .expect("record decision");
        }
        for (i, equity) in [100_000.0, 100_500.0, 99_000.0].iter().enumerate() {
            store
                .record_equity(&run.id, t0 + chrono::Duration::minutes(i as i64), *equity)
                .await
                .expect("record equity");
        }
        run
    }

    fn well_formed_response() -> String {
        serde_json::json!({
            "summary": "Strategy looks plausible.",
            "verdict": "promising",
            "confidence": 0.7,
            "score": 70,
            "findings": (0..3).map(|_| serde_json::json!({
                "type": "performance",
                "severity": "medium",
                "confidence": 0.6,
                "title": "Modest sharpe",
                "description": "Sharpe 1.2 is modest.",
                "evidence": [{"kind": "metric", "reference": "metric:sharpe"}],
                "recommendation": "Test on a longer window.",
            })).collect::<Vec<_>>(),
            "risks": ["concentration"],
            "next_tests": ["longer backtest", "stress test", "out-of-sample"],
            "questions": ["does it survive 2022 chop?"],
        })
        .to_string()
    }

    fn mock_dispatch_text(text: impl Into<String>) -> Arc<dyn LlmDispatch> {
        Arc::new(MockDispatch::echo(text.into()))
    }

    fn mock_dispatch_err() -> Arc<dyn LlmDispatch> {
        // sequence of zero responses → MockDispatch will panic; we want a
        // graceful Err, so use a stub via the response shape.
        struct ErrDispatch;
        #[async_trait::async_trait]
        impl LlmDispatch for ErrDispatch {
            async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
                Err(anyhow::anyhow!("simulated provider 500"))
            }
        }
        Arc::new(ErrDispatch)
    }

    async fn seed_profile(pool: &SqlitePool) {
        // migration 016 seeds the four canonical profiles; nothing to do.
        let count: (i64,) = sqlx::query_as("SELECT count(*) FROM agent_profiles")
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(count.0 >= 4, "expected migration 016 to seed profiles");
    }

    #[tokio::test]
    async fn well_formed_response_persists_completed_with_findings() {
        let pool = fresh_pool().await;
        seed_profile(&pool).await;
        let store = RunStore::new(pool.clone());
        let _run = seed_completed_run(&store).await;
        let dispatch = mock_dispatch_text(well_formed_response());

        // Pull a run id back out so we use exactly what's persisted.
        let runs = store
            .list(crate::eval::store::ListFilter::default())
            .await
            .expect("list runs");
        let run_id = runs[0].id.clone();

        let outcome = run_review(&store, dispatch, &run_id, "reasoning-agent", None, None)
            .await
            .expect("review runs");
        assert_eq!(outcome.status, ReviewStatus::Completed);
        assert_eq!(outcome.verdict, Some(ReviewVerdict::Promising));

        // Confirm findings landed with eval_review_id set.
        let findings = store
            .read_findings_for_review(&outcome.review_id)
            .await
            .expect("read review findings");
        assert_eq!(findings.len(), 3);
        for f in &findings {
            assert_eq!(f.eval_review_id.as_deref(), Some(outcome.review_id.as_str()));
            assert_eq!(f.review_type.as_deref(), Some("performance"));
        }
    }

    #[tokio::test]
    async fn malformed_json_marks_review_inconclusive_not_panic() {
        let pool = fresh_pool().await;
        seed_profile(&pool).await;
        let store = RunStore::new(pool.clone());
        let _ = seed_completed_run(&store).await;
        let dispatch = mock_dispatch_text("definitely not json");

        let runs = store
            .list(crate::eval::store::ListFilter::default())
            .await
            .unwrap();
        let outcome = run_review(&store, dispatch, &runs[0].id, "reasoning-agent", None, None)
            .await
            .expect("no panic");
        assert_eq!(outcome.verdict, Some(ReviewVerdict::Inconclusive));
        let review = store.get_review(&outcome.review_id).await.unwrap().unwrap();
        assert_eq!(review.status, ReviewStatus::Completed);
        assert!(review.summary.unwrap().contains("validation failed"));
    }

    #[tokio::test]
    async fn ungrounded_evidence_marks_review_inconclusive() {
        let pool = fresh_pool().await;
        seed_profile(&pool).await;
        let store = RunStore::new(pool.clone());
        let _ = seed_completed_run(&store).await;

        let mut body: serde_json::Value = serde_json::from_str(&well_formed_response()).unwrap();
        // Replace evidence reference with one that isn't in the allowlist.
        for f in body["findings"].as_array_mut().unwrap() {
            f["evidence"][0]["reference"] = serde_json::json!("metric:made_up_metric");
        }
        let dispatch = mock_dispatch_text(body.to_string());

        let runs = store
            .list(crate::eval::store::ListFilter::default())
            .await
            .unwrap();
        let outcome = run_review(&store, dispatch, &runs[0].id, "reasoning-agent", None, None)
            .await
            .expect("no panic");
        assert_eq!(outcome.verdict, Some(ReviewVerdict::Inconclusive));
        let review = store.get_review(&outcome.review_id).await.unwrap().unwrap();
        assert!(review
            .summary
            .as_deref()
            .map(|s| s.contains("references unknown evidence"))
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn sparse_run_short_circuits_to_inconclusive() {
        let pool = fresh_pool().await;
        seed_profile(&pool).await;
        let store = RunStore::new(pool.clone());

        // Build a completed run with no metrics, no decisions, no equity.
        let run = Run::new_queued("agent-1".into(), "sc-1".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store.begin_running(&run.id).await.unwrap();
        sqlx::query("UPDATE eval_runs SET status = 'completed' WHERE id = ?")
            .bind(&run.id)
            .execute(&pool)
            .await
            .unwrap();

        // Dispatch that would panic if called — proves we short-circuit.
        struct ShouldNotCall;
        #[async_trait::async_trait]
        impl LlmDispatch for ShouldNotCall {
            async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
                panic!("dispatch must not be called for sparse payloads")
            }
        }
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(ShouldNotCall);

        let outcome = run_review(&store, dispatch, &run.id, "reasoning-agent", None, None)
            .await
            .expect("sparse review succeeds");
        assert_eq!(outcome.verdict, Some(ReviewVerdict::Inconclusive));
        let review = store.get_review(&outcome.review_id).await.unwrap().unwrap();
        assert!(review.summary.unwrap().contains("sparse"));
    }

    #[tokio::test]
    async fn dispatch_error_marks_review_failed() {
        let pool = fresh_pool().await;
        seed_profile(&pool).await;
        let store = RunStore::new(pool.clone());
        let _ = seed_completed_run(&store).await;
        let dispatch = mock_dispatch_err();

        let runs = store
            .list(crate::eval::store::ListFilter::default())
            .await
            .unwrap();
        let outcome = run_review(&store, dispatch, &runs[0].id, "reasoning-agent", None, None)
            .await
            .expect("dispatch failure is recorded, not propagated");
        assert_eq!(outcome.status, ReviewStatus::Failed);
        assert_eq!(outcome.verdict, None);
        let review = store.get_review(&outcome.review_id).await.unwrap().unwrap();
        assert!(review.error.unwrap().contains("simulated provider 500"));
    }

    #[tokio::test]
    async fn missing_profile_errors() {
        let pool = fresh_pool().await;
        seed_profile(&pool).await;
        let store = RunStore::new(pool.clone());
        let _ = seed_completed_run(&store).await;
        let dispatch = mock_dispatch_text(well_formed_response());

        let runs = store
            .list(crate::eval::store::ListFilter::default())
            .await
            .unwrap();
        let err = run_review(&store, dispatch, &runs[0].id, "ghost-profile", None, None)
            .await
            .expect_err("missing profile");
        assert!(matches!(err, ReviewError::ProfileNotFound(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn temperature_passes_through_from_profile() {
        // Inspect the LlmRequest the engine sends to confirm the
        // low-temperature deterministic params from the profile are
        // wired through. We use a capturing dispatch.
        use std::sync::Mutex;

        struct Capture {
            seen: Mutex<Option<LlmRequest>>,
        }
        #[async_trait::async_trait]
        impl LlmDispatch for Capture {
            async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
                *self.seen.lock().unwrap() = Some(req);
                Ok(LlmResponse {
                    content: vec![ContentBlock::Text {
                        text: well_formed_response(),
                    }],
                    stop_reason: StopReason::EndTurn,
                    input_tokens: 100,
                    output_tokens: 100,
                })
            }
        }
        let pool = fresh_pool().await;
        seed_profile(&pool).await;
        let store = RunStore::new(pool.clone());
        let _ = seed_completed_run(&store).await;
        let cap = Arc::new(Capture {
            seen: Mutex::new(None),
        });
        let dispatch: Arc<dyn LlmDispatch> = cap.clone();

        let runs = store
            .list(crate::eval::store::ListFilter::default())
            .await
            .unwrap();
        let _ = run_review(&store, dispatch, &runs[0].id, "reasoning-agent", None, None)
            .await
            .expect("review runs");

        let seen = cap.seen.lock().unwrap().clone().expect("dispatch called");
        // reasoning-agent is seeded with max_tokens=8000, temperature=0.2,
        // and the seeded model name — verify each rides on the request.
        assert_eq!(seen.max_tokens, Some(8000));
        assert_eq!(seen.model, "claude-sonnet-4-6");
        // Determinism knob: the profile's temperature must actually be
        // forwarded so reviews don't fall back to the provider default
        // (~1.0). Migration 016 seeds reasoning-agent at 0.2.
        assert_eq!(seen.temperature, Some(0.2));
        assert!(seen.system_prompt.contains("Reasoning"));
        // The strict-JSON contract is appended after the persona prompt.
        assert!(seen.system_prompt.contains("\"verdict\": \"promising\""));
    }
}
