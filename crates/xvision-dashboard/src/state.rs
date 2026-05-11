//! Shared dashboard state — DB pool + xvn home — built once at server start
//! and threaded into every API route via axum's `State` extractor.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;

use xvision_engine::api::settings::providers::ProviderModelsReport;
use xvision_engine::api::{Actor, ApiContext};

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
    /// Per-provider catalog cache so the chat-rail dropdown doesn't
    /// hammer upstream `/models` on every page load. 5-minute TTL is the
    /// sweet spot between freshness and rate-limit pressure (OpenRouter
    /// publishes dozens of model rotations per day; longer than that
    /// and the operator sees stale options).
    models_cache: Arc<Mutex<HashMap<String, CachedModels>>>,
}

impl AppState {
    /// Open `xvn.db` under `xvn_home` (creating both if missing) and run the
    /// engine API migrations. Safe to call from `xvn dashboard serve` and from
    /// integration tests against a tempdir.
    pub async fn new(xvn_home: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&xvn_home)
            .with_context(|| format!("create XVN_HOME dir {}", xvn_home.display()))?;

        let db_path = xvn_home.join("xvn.db");
        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts)
            .await
            .with_context(|| format!("open sqlite {}", db_path.display()))?;

        sqlx::migrate!("../xvision-engine/migrations")
            .run(&pool)
            .await
            .context("run xvision-engine migrations")?;

        // Hydrate the process env with persisted provider API keys so backend
        // constructors that call std::env::var(api_key_env) see the keys the
        // operator pasted via Settings → Providers. Env vars set in the shell
        // win — we don't clobber them.
        if let Err(e) = xvision_engine::api::settings::providers::load_providers_secrets_into_env(
            &xvn_home,
        )
        .await
        {
            tracing::warn!(error = %e, "could not hydrate provider secrets into env");
        }

        Ok(Self {
            pool,
            xvn_home,
            models_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Build an `ApiContext` for one HTTP request. The dashboard always
    /// presents itself as `Actor::Cli { user: "dashboard" }` for now —
    /// per-user identity arrives with the auth plan in v1.5.
    pub fn api_context(&self) -> ApiContext {
        ApiContext {
            db: self.pool.clone(),
            actor: Actor::Cli {
                user: "dashboard".to_string(),
            },
            xvn_home: self.xvn_home.clone(),
        }
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
}
