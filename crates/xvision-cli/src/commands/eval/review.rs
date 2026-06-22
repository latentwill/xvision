//! `xvn eval review <run_id> --agent <profile>` — generate or fetch an
//! analytical review of a completed eval run.
//!
//! Local-store mode only for now. The CLI opens an `ApiContext` against
//! the embedded SQLite store, constructs an `LlmDispatch` from the
//! workspace runtime config (matching the agent profile's `provider`
//! column), calls `eval::review::run_review`, then prints the persisted
//! review either as a human-readable summary (default) or as stable
//! JSON (`--format json`, or any `--output <path>`).
//!
//! Remote-CLI mode (proxy through `/api/eval/runs/:id/review`) is
//! deferred. The dashboard route exists in this same PR; wiring the CLI
//! to it adds a remote-mode flag set that's wider in scope than this
//! contract intends. Local mode is enough to verify the engine pipeline
//! end-to-end from a terminal.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Args, ValueEnum};
use xvision_core::config::{self, ProviderEntry, ProviderKind};
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch, OpenaiCompatDispatch};
use xvision_engine::api::{scenario as api_scenario, ApiContext, ApiError};
use xvision_engine::eval::review::{self, ReviewError, ReviewScenarioSummary, ReviewStatus};
use xvision_engine::eval::store::RunStore;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

use super::{api_to_cli, open_ctx};

/// Output rendering modes the CLI advertises. Backed by `clap::ValueEnum`
/// so `--format <invalid>` errors at parse time with a clap-rendered
/// "possible values" message instead of silently falling back to human
/// output.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Args, Debug)]
pub struct ReviewArgs {
    /// Run id to review (from `xvn eval ls`).
    pub run_id: String,
    /// Agent profile id (`fast-trader-agent`, `reasoning-agent`,
    /// `risk-agent`, `research-agent`, or any operator-defined profile).
    #[arg(long = "agent")]
    pub agent: String,
    /// Re-run even if a review for this (run, agent) pair already exists.
    #[arg(long, default_value_t = false)]
    pub force: bool,
    /// Output format. `human` (default) prints a readable summary;
    /// `json` prints the full review + findings as stable JSON. Setting
    /// `--output` implies `--format json`. Unknown values are a usage
    /// error (typed enum, not free-form string).
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    /// Write the JSON review to this file instead of stdout. Implies
    /// `--format json`.
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(serde::Serialize)]
struct CliReviewOutput {
    review: xvision_engine::eval::review::EvalReview,
    findings: Vec<xvision_engine::eval::findings::Finding>,
}

pub async fn run_review_cmd(args: ReviewArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let store = RunStore::new(ctx.db.clone());

    // Idempotency: only short-circuit on a Completed or in-flight
    // review for this (run, profile) pair. A prior Failed row is
    // retry-eligible — returning it would make transient dispatch
    // errors sticky on subsequent calls.
    let existing = if args.force {
        None
    } else {
        store
            .list_reviews_for_run(&args.run_id)
            .await
            .map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("list reviews for {}: {e}", args.run_id),
            })?
            .into_iter()
            .find(|r| r.agent_profile_id == args.agent && !matches!(r.status, ReviewStatus::Failed))
    };

    let outcome_id = if let Some(prior) = existing {
        prior.id
    } else {
        // Load + validate profile so we surface a typed not-found before
        // we touch the dispatch builder.
        let profile = store
            .get_agent_profile(&args.agent)
            .await
            .map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("load profile {}: {e}", args.agent),
            })?
            .ok_or_else(|| CliError {
                exit: XvnExit::NotFound,
                source: anyhow::anyhow!("agent profile `{}` not found", args.agent),
            })?;
        if !profile.enabled {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!("agent profile `{}` is disabled", args.agent),
            });
        }
        let dispatch =
            build_dispatch_for_profile(&ctx, &profile.provider).map_err(|e| api_to_cli("eval review", e))?;
        // Resolve scenario metadata so the payload carries asset /
        // granularity / time-window context (the engine docstring asks
        // the caller to provide this when available).
        let scenario_summary = resolve_scenario_summary(&ctx, &args.run_id).await;
        let outcome = review::run_review(&store, dispatch, &args.run_id, &profile.id, scenario_summary)
            .await
            .map_err(map_review_error)?;
        outcome.review_id
    };

    let review = store
        .get_review(&outcome_id)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("re-read review {outcome_id}: {e}"),
        })?
        .ok_or_else(|| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("review row vanished after persist: {outcome_id}"),
        })?;
    let findings = store
        .read_findings_for_review(&outcome_id)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("read findings for review {outcome_id}: {e}"),
        })?;

    let out = CliReviewOutput { review, findings };

    // JSON output: stdout by default, file when --output. --output
    // implies --format json regardless of the chosen mode.
    let want_json = matches!(args.format, OutputFormat::Json) || args.output.is_some();
    if want_json {
        if let Some(path) = args.output.as_ref() {
            let json = serde_json::to_string_pretty(&out).context("serialize review JSON")?;
            std::fs::write(path, &json).with_context(|| format!("write {}", path.display()))?;
            // Path-of-record goes to stderr — stdout is reserved for the
            // structured payload when `--format json` was requested.
            crate::human!("{}", path.display());
        } else {
            crate::io::print_json(&out).map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("emit review json: {}", e.source),
            })?;
        }
        return Ok(());
    }

    print_human(&out);
    if matches!(out.review.status, ReviewStatus::Failed) {
        // Surface "model side failed" as upstream — operator can inspect
        // eval_reviews.error for the detail.
        return Err(CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!(
                "review {} marked failed: {}",
                out.review.id,
                out.review.error.unwrap_or_else(|| "(no detail recorded)".into())
            ),
        });
    }
    Ok(())
}

fn print_human(out: &CliReviewOutput) {
    let r = &out.review;
    println!("Review {}", r.id);
    println!("  run             {}", r.eval_run_id);
    println!("  agent_profile   {}", r.agent_profile_id);
    println!("  status          {}", r.status.as_str());
    if let Some(v) = r.verdict {
        println!("  verdict         {}", v.as_str());
    }
    if let Some(s) = r.score {
        println!("  score           {s}");
    }
    if let Some(c) = r.confidence {
        println!("  confidence      {c:.2}");
    }
    if let Some(summary) = r.summary.as_ref() {
        println!("\nSummary\n  {summary}");
    }
    if !out.findings.is_empty() {
        println!("\nFindings ({})", out.findings.len());
        for (i, f) in out.findings.iter().enumerate() {
            let kind = f.review_type.as_deref().unwrap_or(f.kind.as_str());
            let title = f.title.as_deref().unwrap_or(f.summary.as_str());
            println!("  {:>2}. [{}/{}] {}", i + 1, kind, f.severity.as_str(), title);
            if let Some(desc) = f.description.as_deref() {
                println!("      {desc}");
            }
            if let Some(rec) = f.recommendation.as_deref() {
                println!("      → {rec}");
            }
        }
    }
}

// --- helpers -----------------------------------------------------------

/// Resolve `(run.scenario_id → ReviewScenarioSummary)` so the review
/// payload carries scenario context. Returns `None` on any
/// resolution failure — we don't want a scenario-lookup hiccup to take
/// down the review request itself; the engine treats scenario metadata
/// as optional.
///
/// Duplicated with the dashboard route handler. A future track that
/// touches `xvision-engine/src/api/eval.rs` should centralize both the
/// scenario resolver and the provider-dispatch builder there.
async fn resolve_scenario_summary(ctx: &ApiContext, run_id: &str) -> Option<ReviewScenarioSummary> {
    let store = RunStore::new(ctx.db.clone());
    let run = store.get(run_id).await.ok()?;
    let scenario = api_scenario::get(ctx, &run.scenario_id).await.ok()?;
    Some(ReviewScenarioSummary {
        id: scenario.id.clone(),
        name: Some(scenario.display_name.clone()),
        // Scenarios are asset-free; the run is multi-asset and the per-decision
        // asset is the source of truth, so a single run-level asset is no longer
        // meaningful.
        asset: None,
        granularity: None,
        start: Some(scenario.time_window.start.to_rfc3339()),
        end: Some(scenario.time_window.end.to_rfc3339()),
    })
}

fn map_review_error(e: ReviewError) -> CliError {
    match e {
        ReviewError::ProfileNotFound(m) => CliError {
            exit: XvnExit::NotFound,
            source: anyhow::anyhow!("agent profile `{m}` not found"),
        },
        ReviewError::ProfileDisabled(m) => CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("agent profile `{m}` is disabled"),
        },
        ReviewError::RunNotCompleted(m) => CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("review requires a completed run (got `{m}`)"),
        },
        ReviewError::Dispatch(m) => CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("review dispatch failed: {m}"),
        },
        ReviewError::Db(e) => {
            // Engine routes "run not found" through the untyped Db
            // variant. Reclassify on a substring match (same shape the
            // dashboard route uses).
            let msg = format!("{e:#}");
            let exit = if msg.contains("run not found") {
                XvnExit::NotFound
            } else {
                XvnExit::Upstream
            };
            CliError {
                exit,
                source: anyhow::anyhow!(msg),
            }
        }
    }
}

pub(crate) fn build_dispatch_for_profile(
    ctx: &ApiContext,
    provider_name: &str,
) -> Result<Arc<dyn LlmDispatch>, ApiError> {
    let cfg_path = runtime_config_path(ctx);
    let cfg =
        config::load_runtime(&cfg_path).map_err(|e| ApiError::Validation(format!("load config: {e}")))?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == provider_name)
        .ok_or_else(|| {
            ApiError::Validation(format!(
                "provider `{provider_name}` is not configured. Pick a configured provider/model for the agent profile."
            ))
        })?;
    dispatch_from_provider(entry)
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
    let no_auth = matches!(
        entry.kind,
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm
    );
    if api_key.is_empty() && !no_auth {
        return Err(ApiError::Validation(format!(
            "provider `{}` has no API key set",
            entry.name
        )));
    }
    Ok(match entry.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicDispatch::new(api_key)),
        ProviderKind::OpenaiCompat | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::LocalCandle => Arc::new(MockDispatch::echo(
            r#"{"summary":"local-candle stub","verdict":"inconclusive","confidence":0.0,"score":0,"findings":[],"risks":[],"next_tests":[],"questions":[]}"#,
        )),
    })
}

fn runtime_config_path(ctx: &ApiContext) -> std::path::PathBuf {
    config::runtime_config_path(&ctx.xvn_home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tempfile::TempDir;
    use xvision_engine::eval::run::{MetricsSummary, Run, RunMode};
    use xvision_engine::eval::store::DecisionRow;

    /// Build a fresh xvn_home under a TempDir with the canonical runtime
    /// config + a `local-candle` provider entry so dispatch resolves
    /// without API keys. Returns the home + a hydrated `ApiContext`.
    async fn fresh_home() -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let xvn_home = tmp.path().to_path_buf();
        std::fs::create_dir_all(xvn_home.join("config")).unwrap();
        let mut cfg =
            std::fs::read_to_string("../../config/default.toml").expect("read workspace config/default.toml");
        cfg.push_str(
            "\n[[providers]]\nname = \"anthropic\"\nkind = \"local-candle\"\nbase_url = \"\"\napi_key_env = \"\"\n",
        );
        std::fs::write(xvn_home.join("config/default.toml"), cfg).unwrap();
        (tmp, xvn_home)
    }

    async fn seed_run(pool: &SqlitePool) -> String {
        // The `pool_with_migrations()` path doesn't seed canonical
        // scenarios (it bypasses ApiContext::open), so we insert one
        // explicitly here. The `fresh_home()` path that exercises
        // resolve_scenario_summary uses ApiContext::open, which seeds
        // canonical rows on its own.
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
                    baselines: None,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let t0 = Utc::now();
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
                .record_equity(&run.id, t0 + chrono::Duration::seconds(i as i64), 100_000.0)
                .await
                .unwrap();
        }
        run.id
    }

    static NEXT_DB: AtomicU64 = AtomicU64::new(0);

    /// Open a SQLite pool with engine migrations applied. Use a unique
    /// temporary DB path so parallel tests don't race on SQLx migrations.
    async fn pool_with_migrations() -> SqlitePool {
        let db_path = std::env::temp_dir().join(format!(
            "xvision-review-test-{}-{}.db",
            std::process::id(),
            NEXT_DB.fetch_add(1, Ordering::Relaxed)
        ));
        let url = format!("sqlite://{}", db_path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!("../xvision-engine/migrations")
            .run(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn unknown_profile_maps_to_not_found_exit() {
        // Direct call against the engine layer to confirm the error
        // mapping; we don't need to drive `run_review_cmd` end-to-end
        // for this assertion because that path also opens ApiContext
        // (which requires a real xvn_home setup).
        let pool = pool_with_migrations().await;
        let store = RunStore::new(pool.clone());
        let run_id = seed_run(&pool).await;

        // Use a stub dispatch that won't be called because the engine
        // bails before model dispatch on missing profile.
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo("{}"));
        let err = review::run_review(&store, dispatch, &run_id, "ghost-profile", None)
            .await
            .unwrap_err();
        let cli_err = map_review_error(err);
        assert_eq!(cli_err.exit, XvnExit::NotFound);
    }

    #[tokio::test]
    async fn unknown_run_maps_to_not_found_exit() {
        let pool = pool_with_migrations().await;
        let store = RunStore::new(pool.clone());
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo("{}"));
        let err = review::run_review(&store, dispatch, "no-such-run", "reasoning-agent", None)
            .await
            .unwrap_err();
        let cli_err = map_review_error(err);
        assert_eq!(cli_err.exit, XvnExit::NotFound, "got {cli_err:?}");
    }

    #[tokio::test]
    async fn build_dispatch_for_profile_reads_local_candle_provider() {
        let (_tmp, xvn_home) = fresh_home().await;
        let ctx = ApiContext::open(&xvn_home, xvision_engine::api::Actor::Cli { user: "test".into() })
            .await
            .expect("open ctx");
        let dispatch = build_dispatch_for_profile(&ctx, "anthropic").expect("build");
        // Smoke-test: dispatch returns the canned stub.
        let req = xvision_engine::agent::llm::LlmRequest {
            model: "stub".into(),
            system_prompt: "".into(),
            messages: vec![xvision_engine::agent::llm::Message::user_text("hi")],
            max_tokens: Some(64),
            tools: vec![],
            temperature: None,
            response_schema: None,
            cache_control: None,
            force_json: false,
        };
        let resp = dispatch.complete(req).await.expect("dispatch");
        assert!(resp.text().contains("local-candle stub"));
    }

    #[tokio::test]
    async fn resolve_scenario_summary_fills_id_name_window_for_canonical() {
        // ApiContext::open seeds canonical scenarios; build a run that
        // points at one and confirm the resolver returns a populated
        // ReviewScenarioSummary (not None).
        let (_tmp, xvn_home) = fresh_home().await;
        let ctx = ApiContext::open(&xvn_home, xvision_engine::api::Actor::Cli { user: "test".into() })
            .await
            .expect("open ctx");

        let scenario_id = xvision_engine::eval::canonical_scenarios()
            .into_iter()
            .next()
            .expect("at least one canonical scenario")
            .id;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("agent-1".into(), scenario_id.clone(), RunMode::Backtest);
        store.create(&run).await.unwrap();

        let summary = resolve_scenario_summary(&ctx, &run.id)
            .await
            .expect("canonical scenario should resolve");
        assert_eq!(summary.id, scenario_id);
        assert!(summary.name.is_some());
        assert!(summary.granularity.is_none());
        assert!(summary.start.is_some());
        assert!(summary.end.is_some());
    }

    #[test]
    fn format_flag_rejects_unknown_values_at_parse_time() {
        // clap's ValueEnum catches `--format yaml` before run_review_cmd
        // runs; previously the CLI silently treated anything-but-json as
        // human output, hiding typos.
        use clap::Parser;

        #[derive(clap::Parser)]
        struct TestApp {
            #[command(flatten)]
            args: ReviewArgs,
        }

        let good = TestApp::try_parse_from(["x", "--agent", "reasoning-agent", "--format", "json", "RUN"]);
        assert!(good.is_ok(), "json should parse");
        let bad = TestApp::try_parse_from(["x", "--agent", "reasoning-agent", "--format", "yaml", "RUN"]);
        assert!(bad.is_err(), "yaml should be rejected as invalid value");
        let typo = TestApp::try_parse_from(["x", "--agent", "reasoning-agent", "--format", "jsno", "RUN"]);
        assert!(typo.is_err(), "typo should be rejected as invalid value");
    }

    #[tokio::test]
    async fn idempotency_skips_failed_reviews_in_cli_path() {
        // Pre-seed a Failed review for (run, profile); ensure the
        // CLI path's existence-check ignores it and dispatches a
        // fresh attempt. We exercise the same find-logic the CLI
        // command uses via direct store calls.
        let pool = pool_with_migrations().await;
        let store = RunStore::new(pool.clone());
        let run_id = seed_run(&pool).await;

        let mut failed = xvision_engine::eval::review::EvalReview::new_queued(
            run_id.clone(),
            "reasoning-agent".to_string(),
        );
        failed.status = ReviewStatus::Failed;
        store.create_review(&failed).await.unwrap();
        store
            .fail_review(&failed.id, "synthetic prior failure")
            .await
            .unwrap();

        let existing = store
            .list_reviews_for_run(&run_id)
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.agent_profile_id == "reasoning-agent" && !matches!(r.status, ReviewStatus::Failed));
        assert!(existing.is_none(), "Failed rows must not be reused");
    }

    #[tokio::test]
    async fn end_to_end_review_persists_inconclusive_with_local_candle() {
        let (_tmp, xvn_home) = fresh_home().await;
        // Use a real file-backed ApiContext so the CLI command can
        // reach the same pool as the engine's RunStore.
        let ctx = ApiContext::open(&xvn_home, xvision_engine::api::Actor::Cli { user: "test".into() })
            .await
            .expect("open ctx");
        let store = RunStore::new(ctx.db.clone());

        // Seed against the same pool.
        let run_id = seed_run(&ctx.db).await;

        let dispatch = build_dispatch_for_profile(&ctx, "anthropic").expect("build dispatch");
        let outcome = review::run_review(&store, dispatch, &run_id, "reasoning-agent", None)
            .await
            .expect("run_review");
        assert_eq!(outcome.status, ReviewStatus::Completed);
        // Local-candle stub returns inconclusive, which the parser
        // accepts with zero findings.
        assert_eq!(
            outcome.verdict,
            Some(xvision_engine::eval::review::ReviewVerdict::Inconclusive)
        );
    }
}
