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
use xvision_engine::api::eval::RunDetail;
use xvision_engine::api::settings::providers::ProviderModelsReport;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::autooptimizer::progress::CycleProgressEvent;
use xvision_engine::safety::SafetyManager;
use xvision_observability::{
    AgentRunRecorder, BroadcastSubscriber, RunEventBus as ObsRunEventBus, SqliteRecorder,
};

const MODELS_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

/// Eval `get_run` response cache TTL. Tight cap on purpose: the goal is to
/// collapse the UI's burst polling (one tab firing per-tick refetches when an
/// operator is watching a long backtest) into a single DB hit per ~500ms
/// window without ever serving meaningfully stale data — the dashboard's
/// in-flight tick is 2s, so a 500ms TTL still produces a per-tick refresh on
/// the wire but absorbs duplicate concurrent reads from multiple tabs / the
/// detail view + the list view's sibling lookup.
///
/// Terminal-status responses bypass this cache entirely (see
/// `routes::eval_runs::get`) since they never change — caching a `completed`
/// run is the engine's job, not ours.
const EVAL_RUN_CACHE_TTL: Duration = Duration::from_millis(500);

#[derive(Clone)]
struct CachedModels {
    fetched_at: Instant,
    report: ProviderModelsReport,
}

#[derive(Clone)]
struct CachedRunDetail {
    fetched_at: Instant,
    detail: RunDetail,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub xvn_home: PathBuf,
    /// Cortex-memory recorder, shared across the surfaces that use memory
    /// (chat rail, optimizer cycles). Populated from the bootstrap
    /// `ApiContext`'s provisioned recorder whenever one is available — the
    /// recorder being present is harmless; per-surface USAGE is gated by
    /// [`AppState::chat_memory_enabled`] / [`AppState::optimizer_memory_enabled`]
    /// (config-backed, default ON; env overrides win). `None` only when the
    /// bootstrap context could not provision a recorder. Behind an `Arc` so
    /// `AppState` stays cheaply `Clone`.
    pub memory_recorder: Option<Arc<xvision_engine::agent::memory_recorder::MemoryRecorder>>,
    /// Startup snapshot of the persisted memory enablement config
    /// (`$XVN_HOME/config/memory.toml`). Read once at server start so the
    /// chat/cycle hot paths never do per-request file I/O. Changing the
    /// config via the settings card takes effect on the next restart
    /// (matches the `obs_config` snapshot pattern). The accessors fold in
    /// the env overrides (`XVN_CHAT_MEMORY` / `XVN_OPTIMIZER_MEMORY`) which
    /// always win.
    memory_config: xvision_engine::api::settings::memory::MemoryConfig,
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
    /// Per-chat-session live fan-out of unified events (Phase 1.2). The chat
    /// route publishes each projected `UnifiedEvent` here; the unified-stream
    /// handler (`GET /api/chat-rail/sessions/:id/stream`) subscribes for the
    /// live tail after replaying the persisted `session_events` log.
    pub session_event_bus: Arc<crate::session_bus::SessionEventBus>,
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
    /// Per-run cache for the `GET /api/eval/runs/:id` route. Keyed by run id;
    /// a 500ms TTL absorbs burst polling. Terminal runs are not inserted (the
    /// route bypasses the cache for them) — see
    /// `EVAL_RUN_CACHE_TTL` and `routes::eval_runs::get`.
    eval_run_cache: Arc<Mutex<HashMap<String, CachedRunDetail>>>,
    eval_run_cache_ttl: Duration,
    cli_command: PathBuf,
    cli_runner: Arc<CliJobRunner>,
    /// Global safety pause-gate singleton. Bootstrapped at server startup from
    /// `safety_state` table (migration 030). Clone-cheap — inner `Arc<RwLock<>>`.
    safety_manager: SafetyManager,
    /// Broadcast channel for autooptimizer cycle progress events. The IPC
    /// bridge (`ipc::spawn_autooptimizer_subscriber`) publishes here when
    /// `xvn optimizer mutate-once --ipc-socket` connects; the SSE handler
    /// at `GET /api/autooptimizer/events` subscribes per request.
    pub autooptimizer_tx: tokio::sync::broadcast::Sender<CycleProgressEvent>,
    /// F28: registry of cooperative cancel flags for in-flight optimizer cycles,
    /// keyed by `cycle_id`. `start_cycle` registers a flag and passes it into
    /// `run_cycle`; `POST /cycles/:id/cancel` sets it so the cycle stops
    /// launching further candidates. Entries are removed when the cycle ends.
    autooptimizer_cancels: Arc<Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>>,
    /// P4: registry of cooperative pause flags for in-flight optimizer cycles,
    /// keyed by `cycle_id`. `start_cycle` registers a flag alongside the cancel
    /// flag; `POST /cycles/:id/pause` sets it so the cycle suspends at the next
    /// safe checkpoint; `POST /cycles/:id/resume` clears it to continue.
    /// Entries are removed when the cycle ends (same lifecycle as cancel flags).
    autooptimizer_pauses: Arc<Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>>,
    /// Marketplace indexer snapshot — written by the background poller
    /// (`marketplace_index::spawn_indexer`), read by the
    /// `/api/marketplace/*` read routes. Defaults to the empty snapshot; the
    /// indexer only spawns when the chain env is configured (see
    /// `IndexerCfg::from_env` + `server::serve`).
    pub marketplace_snapshot: crate::marketplace_index::SharedSnapshot,
    /// Whether the marketplace indexer task was actually spawned this
    /// process. Distinct from snapshot freshness: `active` on the status
    /// route requires BOTH this flag AND a completed first poll.
    marketplace_indexer_active: Arc<std::sync::atomic::AtomicBool>,
    /// Marketplace chain config, resolved ONCE at server startup
    /// (`server::serve`) instead of per request (xvision-df3). `None` =
    /// dormant: chain-touching marketplace routes return the same 503s they
    /// did when they read the env per request. Tests inject via
    /// [`AppState::with_marketplace_chain_config`].
    marketplace_chain: Option<Arc<crate::chain_config::MarketplaceChainConfig>>,
    /// Test-only override for the sealed-bundle crypto backend. Production
    /// resolves the backend from `marketplace_chain.lit`
    /// (`MarketplaceChainConfig::resolve_sealed_crypto`); route tests inject a
    /// deterministic fake here via [`AppState::with_sealed_crypto`] so the
    /// sealed-publish encrypt path is exercised without a live Lit endpoint —
    /// mirroring how `marketplace_snapshot` / `marketplace_chain` are injected.
    sealed_crypto_override: Option<Arc<dyn xvision_marketplace::SealedBundleCrypto>>,
    /// Lane cgz: server-issued, single-use, time-bounded nonce store backing
    /// the sealed-import proof-of-address challenge
    /// (`GET /api/marketplace/listings/:id/import-challenge` issues, the
    /// sealed-import route consumes). In-memory + Arc-shared so single-use holds
    /// across the per-request `AppState` clones, like the autooptimizer maps.
    marketplace_nonces: crate::marketplace_nonce::NonceStore,
}

/// Adapts an `Arc<dyn SealedBundleCrypto>` (the test-injection shape) into a
/// `Box<dyn SealedBundleCrypto>` (the resolver's return shape) by forwarding
/// every trait method to the shared inner backend.
struct SealedCryptoHandle(Arc<dyn xvision_marketplace::SealedBundleCrypto>);

#[async_trait::async_trait]
impl xvision_marketplace::SealedBundleCrypto for SealedCryptoHandle {
    async fn encrypt(&self, plaintext: &[u8]) -> Result<String, xvision_marketplace::MarketplaceError> {
        self.0.encrypt(plaintext).await
    }

    fn gate_action_cid(&self) -> &str {
        self.0.gate_action_cid()
    }

    fn is_configured(&self) -> bool {
        self.0.is_configured()
    }
}

impl AppState {
    /// F28: register a fresh cancel flag for a cycle and return it (to thread
    /// into `run_cycle`).
    pub fn autooptimizer_register_cancel(&self, cycle_id: &str) -> Arc<std::sync::atomic::AtomicBool> {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Ok(mut map) = self.autooptimizer_cancels.lock() {
            map.insert(cycle_id.to_string(), Arc::clone(&flag));
        }
        flag
    }

    /// F28: request cancellation of an in-flight cycle. Returns true if a running
    /// cycle with that id was found and signalled.
    pub fn autooptimizer_request_cancel(&self, cycle_id: &str) -> bool {
        match self.autooptimizer_cancels.lock() {
            Ok(map) => map
                .get(cycle_id)
                .map(|f| {
                    f.store(true, std::sync::atomic::Ordering::Relaxed);
                    true
                })
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// F28: drop a cycle's cancel flag once it has finished.
    pub fn autooptimizer_deregister_cancel(&self, cycle_id: &str) {
        if let Ok(mut map) = self.autooptimizer_cancels.lock() {
            map.remove(cycle_id);
        }
    }

    /// P4: register a fresh pause flag for a cycle and return it (to thread
    /// into `run_cycle`). Called alongside `autooptimizer_register_cancel` when
    /// `start_cycle` launches a new cycle.
    pub fn autooptimizer_register_pause(&self, cycle_id: &str) -> Arc<std::sync::atomic::AtomicBool> {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Ok(mut map) = self.autooptimizer_pauses.lock() {
            map.insert(cycle_id.to_string(), Arc::clone(&flag));
        }
        flag
    }

    /// P4: request a pause of an in-flight cycle. Returns true if a running
    /// cycle with that id was found and signalled.
    pub fn autooptimizer_request_pause(&self, cycle_id: &str) -> bool {
        match self.autooptimizer_pauses.lock() {
            Ok(map) => map
                .get(cycle_id)
                .map(|f| {
                    f.store(true, std::sync::atomic::Ordering::Relaxed);
                    true
                })
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// P4: clear the pause flag to resume an in-flight cycle. Returns true if
    /// a paused cycle with that id was found and cleared.
    pub fn autooptimizer_request_resume(&self, cycle_id: &str) -> bool {
        match self.autooptimizer_pauses.lock() {
            Ok(map) => map
                .get(cycle_id)
                .map(|f| {
                    f.store(false, std::sync::atomic::Ordering::Relaxed);
                    true
                })
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// P4: check whether the pause flag for a cycle is currently set (i.e. the
    /// cycle is at a pause checkpoint or in the middle of suspending).
    pub fn autooptimizer_is_paused(&self, cycle_id: &str) -> bool {
        match self.autooptimizer_pauses.lock() {
            Ok(map) => map
                .get(cycle_id)
                .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// P4: drop a cycle's pause flag once it has finished (mirror of
    /// `autooptimizer_deregister_cancel`).
    pub fn autooptimizer_deregister_pause(&self, cycle_id: &str) {
        if let Ok(mut map) = self.autooptimizer_pauses.lock() {
            map.remove(cycle_id);
        }
    }
    /// Open `xvn.db` under `xvn_home` (creating both if missing) and run the
    /// engine API migrations. Safe to call from `xvn dashboard serve` and from
    /// integration tests against a tempdir.
    pub async fn new(xvn_home: PathBuf) -> anyhow::Result<Self> {
        Self::new_with_eval_run_cache_ttl(xvn_home, EVAL_RUN_CACHE_TTL).await
    }

    /// Build state with a caller-provided eval-run cache TTL. Production uses
    /// [`AppState::new`]; tests that assert cache hits can use a longer TTL to
    /// avoid coupling correctness to scheduler timing.
    pub async fn new_with_eval_run_cache_ttl(
        xvn_home: PathBuf,
        eval_run_cache_ttl: Duration,
    ) -> anyhow::Result<Self> {
        let bootstrap_ctx = ApiContext::open(
            &xvn_home,
            Actor::Cli {
                user: "dashboard-bootstrap".into(),
            },
        )
        .await
        .with_context(|| format!("open ApiContext at {}", xvn_home.display()))?;
        let pool = bootstrap_ctx.db.clone();

        // Cortex memory: always capture the bootstrap context's
        // provider-aware recorder. Memory now defaults ON for chat +
        // optimizer (the embedder resolves to the offline Local fallback
        // when no real provider is configured, so it works out of the box).
        // Whether a given surface actually records/recalls is decided per
        // request by `chat_memory_enabled` / `optimizer_memory_enabled`
        // (config-backed, env-override). Holding the recorder here is
        // harmless when a surface is disabled.
        let memory_recorder = bootstrap_ctx.memory_recorder.clone();

        // Startup snapshot of the persisted memory enablement config.
        // Missing/invalid file → defaults (Auto embedder + both surfaces ON).
        let memory_config = xvision_engine::api::settings::memory::load_from_file(
            &xvn_home.join("config").join("memory.toml"),
        );

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

        // Bootstrap the safety manager. Constructs from the already-open pool,
        // then loads / seeds the safety_state row. For v1 we treat any non-paper
        // venue as absent (live surfaces are all stubbed), so `live_venue_present`
        // is always false here. A follow-on PR wires broker-config inspection.
        let safety_manager = SafetyManager::new(pool.clone());
        if let Err(e) = safety_manager.bootstrap(false).await {
            tracing::warn!(error = %e, "safety_manager bootstrap failed; using default (unpaused) state");
        }

        let (autooptimizer_tx, _) = tokio::sync::broadcast::channel(256);

        Ok(Self {
            pool,
            xvn_home,
            memory_recorder,
            memory_config,
            event_bus: Arc::new(RunEventBus::new()),
            obs_event_bus,
            obs_broadcast,
            session_event_bus: Arc::new(crate::session_bus::SessionEventBus::new()),
            obs_config,
            models_cache: Arc::new(Mutex::new(HashMap::new())),
            eval_run_cache: Arc::new(Mutex::new(HashMap::new())),
            eval_run_cache_ttl,
            cli_command,
            cli_runner,
            safety_manager,
            autooptimizer_tx,
            autooptimizer_cancels: Arc::new(Mutex::new(HashMap::new())),
            autooptimizer_pauses: Arc::new(Mutex::new(HashMap::new())),
            marketplace_snapshot: Default::default(),
            marketplace_indexer_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            marketplace_chain: None,
            sealed_crypto_override: None,
            marketplace_nonces: crate::marketplace_nonce::NonceStore::new(),
        })
    }

    /// Lane cgz: the server-issued single-use nonce store for sealed-import
    /// proof-of-address challenges.
    pub fn marketplace_nonces(&self) -> &crate::marketplace_nonce::NonceStore {
        &self.marketplace_nonces
    }

    /// The startup-resolved marketplace chain config (`None` = dormant).
    pub fn marketplace_chain(&self) -> Option<&crate::chain_config::MarketplaceChainConfig> {
        self.marketplace_chain.as_deref()
    }

    /// Attach the startup-resolved marketplace chain config. Called once by
    /// `server::serve`; tests use it to inject config without touching env.
    pub fn with_marketplace_chain_config(mut self, cfg: crate::chain_config::MarketplaceChainConfig) -> Self {
        self.marketplace_chain = Some(Arc::new(cfg));
        self
    }

    /// Resolve the sealed-bundle crypto backend for the sealed-publish path.
    /// Returns the test-injected override when present, else the backend
    /// resolved from the startup `MarketplaceChainConfig` (`lit` → Lit client,
    /// absent → `NoopSealed`), else `NoopSealed` when there is no config at all.
    pub fn sealed_crypto(&self) -> Box<dyn xvision_marketplace::SealedBundleCrypto> {
        if let Some(crypto) = &self.sealed_crypto_override {
            return Box::new(SealedCryptoHandle(Arc::clone(crypto)));
        }
        match self.marketplace_chain() {
            Some(cfg) => cfg.resolve_sealed_crypto(),
            None => Box::new(xvision_marketplace::NoopSealed),
        }
    }

    /// Inject a sealed-bundle crypto backend (tests only). Lets a route test
    /// exercise the encrypt path with a deterministic fake instead of a live
    /// Lit endpoint.
    pub fn with_sealed_crypto(mut self, crypto: Arc<dyn xvision_marketplace::SealedBundleCrypto>) -> Self {
        self.sealed_crypto_override = Some(crypto);
        self
    }

    /// Whether the marketplace indexer task was spawned this process.
    pub fn marketplace_indexer_active(&self) -> bool {
        self.marketplace_indexer_active
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Record that the marketplace indexer was spawned. Called once from
    /// `server::serve` when the chain env is configured; also used by route
    /// tests to simulate an active indexer.
    pub fn mark_marketplace_indexer_active(&self) {
        self.marketplace_indexer_active
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Shared safety manager reference for route handlers.
    pub fn safety_manager(&self) -> &SafetyManager {
        &self.safety_manager
    }

    /// Read the `XVN_CHAT_MEMORY` / `XVN_OPTIMIZER_MEMORY` env override.
    /// `1`/`true` → `Some(true)`, `0`/`false` → `Some(false)`, anything
    /// else (incl. unset) → `None` so the config value applies.
    fn env_memory_override(var: &str) -> Option<bool> {
        match std::env::var(var).ok().as_deref() {
            Some("1") | Some("true") => Some(true),
            Some("0") | Some("false") => Some(false),
            _ => None,
        }
    }

    /// Whether the chat rail should record/recall. Env `XVN_CHAT_MEMORY`
    /// wins when set; otherwise the config snapshot (`chat_enabled`,
    /// default ON). Cheap — no per-request file I/O.
    pub fn chat_memory_enabled(&self) -> bool {
        Self::env_memory_override("XVN_CHAT_MEMORY").unwrap_or(self.memory_config.chat_enabled)
    }

    /// Whether the optimizer should record/recall. Env
    /// `XVN_OPTIMIZER_MEMORY` wins when set; otherwise the config snapshot
    /// (`optimizer_enabled`, default ON). Cheap — no per-request file I/O.
    pub fn optimizer_memory_enabled(&self) -> bool {
        Self::env_memory_override("XVN_OPTIMIZER_MEMORY").unwrap_or(self.memory_config.optimizer_enabled)
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

    /// Read a cached `RunDetail` for `run_id` if it's within the
    /// `EVAL_RUN_CACHE_TTL` window. Returns `None` on miss or stale entry.
    /// The route handler is responsible for not inserting terminal runs into
    /// the cache in the first place; this getter does not re-check status.
    pub fn eval_run_cache_get(&self, run_id: &str) -> Option<RunDetail> {
        let cache = self.eval_run_cache.lock().ok()?;
        let entry = cache.get(run_id)?;
        if entry.fetched_at.elapsed() > self.eval_run_cache_ttl {
            None
        } else {
            Some(entry.detail.clone())
        }
    }

    /// Insert a freshly-fetched `RunDetail` into the cache. Callers should
    /// only call this for non-terminal runs.
    pub fn eval_run_cache_put(&self, run_id: String, detail: RunDetail) {
        if let Ok(mut cache) = self.eval_run_cache.lock() {
            cache.insert(
                run_id,
                CachedRunDetail {
                    fetched_at: Instant::now(),
                    detail,
                },
            );
        }
    }

    /// Drop the cache entry for `run_id`. Called when an operation that
    /// can mutate run state (cancel, retry, delete) lands so a subsequent
    /// poll re-fetches fresh.
    pub fn eval_run_cache_invalidate(&self, run_id: &str) {
        if let Ok(mut cache) = self.eval_run_cache.lock() {
            cache.remove(run_id);
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

        if restarted > 0 || recovery.orphaned_running > 0 {
            tracing::info!(
                target: "xvision::dashboard",
                restarted_queued = restarted,
                orphaned_running = recovery.orphaned_running,
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

    /// Bootstrap the dashboard-owned tables (dashboard_sessions, auth_audit).
    ///
    /// Uses direct `CREATE TABLE IF NOT EXISTS` DDL rather than sqlx's
    /// versioned migration system. The engine already populates
    /// `_sqlx_migrations` with its own versions; running a second independent
    /// sqlx Migrator over the same pool causes UNIQUE constraint violations
    /// on that table. Plain DDL avoids the conflict and is idempotent.
    ///
    /// Called from `server::serve` at startup and from integration-test
    /// helpers that need the session/audit tables present.
    pub async fn run_dashboard_migrations(&self) -> anyhow::Result<()> {
        // dashboard_sessions: live session tokens.
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS dashboard_sessions (
                session_id   TEXT NOT NULL PRIMARY KEY,
                token_hash   TEXT NOT NULL UNIQUE,
                created_at   TEXT NOT NULL,
                expires_at   TEXT NOT NULL,
                source_ip    TEXT,
                label        TEXT
            )"#,
        )
        .execute(&self.pool)
        .await
        .context("create dashboard_sessions table")?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_dashboard_sessions_token_hash ON dashboard_sessions (token_hash)",
        )
        .execute(&self.pool)
        .await
        .context("create dashboard_sessions token_hash index")?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_dashboard_sessions_expires_at ON dashboard_sessions (expires_at)",
        )
        .execute(&self.pool)
        .await
        .context("create dashboard_sessions expires_at index")?;

        // auth_audit: append-only log of every mutating route call.
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS auth_audit (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp           TEXT NOT NULL,
                route               TEXT NOT NULL,
                method              TEXT NOT NULL,
                session_token_hash  TEXT NOT NULL,
                source_ip           TEXT NOT NULL,
                response_status     INTEGER NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await
        .context("create auth_audit table")?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_auth_audit_timestamp ON auth_audit (timestamp)")
            .execute(&self.pool)
            .await
            .context("create auth_audit timestamp index")?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_auth_audit_session_token_hash ON auth_audit (session_token_hash)",
        )
        .execute(&self.pool)
        .await
        .context("create auth_audit session_token_hash index")?;

        // publish_receipts: idempotency key for marketplace publishing
        // (bead xvision-4dn). One row per published agent_id (the strategy
        // ULID / NFT token id); the publish handler short-circuits a
        // re-publish with 409 Conflict when a receipt is present, so a
        // re-click / retry cannot mint a duplicate NFT + listing.
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS publish_receipts (
                agent_id     TEXT NOT NULL PRIMARY KEY,
                token_id     TEXT NOT NULL,
                listing_id   TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                published_at TEXT NOT NULL,
                name         TEXT
            )"#,
        )
        .execute(&self.pool)
        .await
        .context("create publish_receipts table")?;

        // `name`: the creator-chosen listing name captured at publish time
        // (defaults to the strategy's display name). Added after the original
        // table shipped, so ALTER any pre-existing table idempotently — a
        // "duplicate column name" error on a table that already has it is
        // expected and ignored.
        let _ = sqlx::query("ALTER TABLE publish_receipts ADD COLUMN name TEXT")
            .execute(&self.pool)
            .await;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_publish_receipts_token_id ON publish_receipts (token_id)",
        )
        .execute(&self.pool)
        .await
        .context("create publish_receipts token_id index")?;

        Ok(())
    }
}
