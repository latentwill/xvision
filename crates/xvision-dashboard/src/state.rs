//! Shared dashboard state — DB pool + xvn home — built once at server start
//! and threaded into every API route via axum's `State` extractor.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use sqlx::SqlitePool;

use crate::cli_jobs::runner::CliJobRunner;
use crate::cli_jobs::store::CliJobStore;
use xvision_engine::api::chart::RunEventBus;
use xvision_engine::api::settings::providers::ProviderModelsReport;
use xvision_engine::api::{Actor, ApiContext};
use xvision_observability::{
    AgentRunRecorder, BroadcastSubscriber, RunEventBus as ObsRunEventBus, SqliteRecorder,
};

const MODELS_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
struct CachedModels {
    fetched_at: Instant,
    report: ProviderModelsReport,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub xvn_home: PathBuf,
    /// Singleton live-stream event bus. Constructed once at server start and
    /// shared across all HTTP requests via `api_context()`.
    pub event_bus: Arc<RunEventBus>,
    /// Agent-run observability bus + the broadcast subscriber that
    /// backs `/api/agent-runs/:id/stream`. The bus owns the
    /// `SqliteRecorder` (canonical persistence) and the
    /// `BroadcastSubscriber` (fan-out to per-run SSE channels) as
    /// recorders — keeping a separate handle to the subscriber lets
    /// route handlers call `subscribe_run` without going through the
    /// recorder trait.
    pub obs_event_bus: Arc<ObsRunEventBus>,
    pub obs_broadcast: Arc<BroadcastSubscriber>,
    /// Resolved observability config (precedence: CLI > env > file >
    /// default). Threaded into every per-request `ApiContext` so engine
    /// eval handlers can stamp the right `retention_mode` on the
    /// `RunStarted` event.
    pub obs_config: Arc<xvision_observability::ObservabilityConfig>,
    /// Per-provider catalog cache so the chat-rail dropdown doesn't
    /// hammer upstream `/models` on every page load. 5-minute TTL is the
    /// sweet spot between freshness and rate-limit pressure (OpenRouter
    /// publishes dozens of model rotations per day; longer than that
    /// and the operator sees stale options).
    models_cache: Arc<Mutex<HashMap<String, CachedModels>>>,
    cli_command: PathBuf,
    cli_runner: Arc<CliJobRunner>,
}

impl AppState {
    /// Open `xvn.db` under `xvn_home` (creating both if missing) and run the
    /// engine API migrations. Safe to call from `xvn dashboard serve` and from
    /// integration tests against a tempdir.
    pub async fn new(xvn_home: PathBuf) -> anyhow::Result<Self> {
        let bootstrap_ctx = ApiContext::open(
            &xvn_home,
            Actor::Cli {
                user: "dashboard-bootstrap".into(),
            },
        )
        .await
        .with_context(|| format!("open ApiContext at {}", xvn_home.display()))?;
        let pool = bootstrap_ctx.db.clone();

        // Hydrate the process env with persisted provider API keys so backend
        // constructors that call std::env::var(api_key_env) see the keys the
        // operator pasted via Settings → Providers. Env vars set in the shell
        // win — we don't clobber them.
        if let Err(e) =
            xvision_engine::api::settings::providers::load_providers_secrets_into_env(&xvn_home).await
        {
            tracing::warn!(error = %e, "could not hydrate provider secrets into env");
        }

        let cli_command = PathBuf::from("xvn");
        let cli_runner = Arc::new(CliJobRunner::new(pool.clone(), cli_command.clone()));

        // Agent-run observability fan-out: SqliteRecorder for persistence
        // + BroadcastSubscriber for the live SSE stream. The bus drives
        // both as recorders on a single consumer task.
        let obs_broadcast: Arc<BroadcastSubscriber> = Arc::new(BroadcastSubscriber::new());
        let sqlite_recorder: Arc<SqliteRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
        let subscribers: Vec<Arc<dyn AgentRunRecorder>> = vec![
            sqlite_recorder as Arc<dyn AgentRunRecorder>,
            obs_broadcast.clone() as Arc<dyn AgentRunRecorder>,
        ];
        let obs_event_bus = Arc::new(ObsRunEventBus::new(subscribers));

        // Resolve the observability config once at startup — precedence
        // CLI flag > env > file > built-in default. CLI flag is None here
        // (the dashboard process doesn't take retention flags), so the
        // effective chain is env > file > default. Persisted across the
        // process lifetime so eval handlers don't re-read it per request.
        //
        // We go through `retention::resolve` (rather than the simpler
        // `ObservabilityConfig::load_with_env`) so the explicit-source
        // startup `warn!` line still fires when an operator set
        // full_debug via env or TOML — `load_with_env` no longer
        // distinguishes default vs. explicit and would only emit the
        // quiet info-level line.
        let obs_config_path = xvision_observability::default_config_path();
        let obs_config = match xvision_observability::retention::resolve(
            &obs_config_path,
            &xvision_observability::retention::CliOverrides::default(),
        ) {
            Ok(view) => Arc::new(view.config()),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "could not resolve observability config; using defaults"
                );
                Arc::new(xvision_observability::ObservabilityConfig::default())
            }
        };

        Ok(Self {
            pool,
            xvn_home,
            event_bus: Arc::new(RunEventBus::new()),
            obs_event_bus,
            obs_broadcast,
            obs_config,
            models_cache: Arc::new(Mutex::new(HashMap::new())),
            cli_command,
            cli_runner,
        })
    }

    /// Build an `ApiContext` for one HTTP request. The dashboard always
    /// presents itself as `Actor::Cli { user: "dashboard" }` for now —
    /// per-user identity arrives with the auth plan in v1.5.
    pub fn api_context(&self) -> ApiContext {
        ApiContext::new(
            self.pool.clone(),
            Actor::Cli {
                user: "dashboard".to_string(),
            },
            self.xvn_home.clone(),
        )
        .with_event_bus(self.event_bus.clone())
        // qa-eval-observability-wiring (2026-05-17): hand the
        // dashboard's singleton observability bus to every
        // engine-side eval run so spans + errors land in
        // `/api/agent-runs/<eval_run_id>` and the trace dock.
        .with_obs_event_bus(self.obs_event_bus.clone())
        .with_obs_config(self.obs_config.clone())
    }

    /// Read a cached models report for `provider` if it's within the TTL.
    /// Stale entries are evicted lazily on the next put.
    pub fn models_cache_get(&self, provider: &str) -> Option<ProviderModelsReport> {
        let cache = self.models_cache.lock().ok()?;
        let entry = cache.get(provider)?;
        if entry.fetched_at.elapsed() > MODELS_CACHE_TTL {
            None
        } else {
            Some(entry.report.clone())
        }
    }

    /// Insert a freshly-fetched report into the cache.
    pub fn models_cache_put(&self, provider: String, report: ProviderModelsReport) {
        if let Ok(mut cache) = self.models_cache.lock() {
            cache.insert(
                provider,
                CachedModels {
                    fetched_at: Instant::now(),
                    report,
                },
            );
        }
    }

    /// Drop a specific provider's cache (after a key rotation or a
    /// manual "refresh" from the UI). No-op if the entry doesn't exist.
    pub fn models_cache_invalidate(&self, provider: &str) {
        if let Ok(mut cache) = self.models_cache.lock() {
            cache.remove(provider);
        }
    }

    pub fn cli_command(&self) -> &std::path::Path {
        &self.cli_command
    }

    pub fn cli_runner(&self) -> Arc<CliJobRunner> {
        self.cli_runner.clone()
    }

    pub async fn recover_cli_jobs(&self) -> anyhow::Result<()> {
        let store = CliJobStore::new(self.pool.clone());
        let recovery = store
            .recover_after_restart()
            .await
            .context("recover cli jobs after dashboard restart")?;

        let restarted = recovery.restarted_queued.len();
        for job in recovery.restarted_queued {
            self.cli_runner.start(job);
        }

        if restarted > 0 || recovery.failed_running > 0 {
            tracing::info!(
                target: "xvision::dashboard",
                restarted_queued = restarted,
                failed_running = recovery.failed_running,
                "recovered cli jobs at startup",
            );
        }

        Ok(())
    }

    pub fn with_cli_command_for_tests(mut self, cli_command: PathBuf) -> Self {
        self.cli_runner = Arc::new(CliJobRunner::new(self.pool.clone(), cli_command.clone()));
        self.cli_command = cli_command;
        self
    }
}
