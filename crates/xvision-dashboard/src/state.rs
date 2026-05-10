//! Shared dashboard state — DB pool + xvn home — built once at server start
//! and threaded into every API route via axum's `State` extractor.

use std::path::PathBuf;

use anyhow::Context;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;

use xvision_engine::api::{Actor, ApiContext};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub xvn_home: PathBuf,
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

        Ok(Self { pool, xvn_home })
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
}
