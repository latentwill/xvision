use std::path::{Path, PathBuf};

use anyhow::Context;
use async_trait::async_trait;

use crate::bundle::StrategyBundle;

/// Canonical on-disk directory for `StrategyBundle` JSON files, relative to
/// `$XVN_HOME`. Single source of truth so the CLI and dashboard never drift
/// onto different paths.
pub fn strategy_store_dir(xvn_home: &Path) -> PathBuf {
    xvn_home.join("strategies")
}

#[async_trait]
pub trait BundleStore: Send + Sync {
    async fn save(&self, bundle: &StrategyBundle) -> anyhow::Result<()>;
    async fn load(&self, id: &str) -> anyhow::Result<StrategyBundle>;
    async fn list(&self) -> anyhow::Result<Vec<String>>;
}

pub struct FilesystemStore {
    root: PathBuf,
}

impl FilesystemStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
}

#[async_trait]
impl BundleStore for FilesystemStore {
    async fn save(&self, bundle: &StrategyBundle) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.path_for(&bundle.manifest.id);
        let json = serde_json::to_vec_pretty(bundle)?;
        tokio::fs::write(&path, json)
            .await
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    async fn load(&self, id: &str) -> anyhow::Result<StrategyBundle> {
        let path = self.path_for(id);
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        if !self.root.exists() {
            return Ok(vec![]);
        }
        let mut ids = vec![];
        let mut rd = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = rd.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(id) = name_str.strip_suffix(".json") {
                ids.push(id.to_string());
            }
        }
        Ok(ids)
    }
}
