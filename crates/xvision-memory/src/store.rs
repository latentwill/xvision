//! SQLite-backed memory store (V2D).

use std::path::Path;

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::types::{MemoryItem, MemoryMatch, Namespace};

pub struct MemoryStore {
    pool: SqlitePool,
}

impl MemoryStore {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("memory: create parent dir")?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .context("memory: open sqlite pool")?;
        sqlx::migrate!("./migrations").run(&pool).await.context("memory: migrate")?;
        Ok(Self { pool })
    }

    pub async fn open_in_memory() -> anyhow::Result<Self> {
        let opts = SqliteConnectOptions::new()
            .in_memory(true)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool { &self.pool }
}
