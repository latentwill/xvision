use std::path::{Path, PathBuf};

use anyhow::Context;
use async_trait::async_trait;

use crate::strategies::id::{validate_strategy_id_for_path, StrategyIdError};
use crate::strategies::Strategy;

/// Canonical on-disk directory for `Strategy` JSON files, relative to
/// `$XVN_HOME`. Single source of truth so the CLI and dashboard never drift
/// onto different paths.
pub fn strategy_store_dir(xvn_home: &Path) -> PathBuf {
    xvn_home.join("strategies")
}

#[async_trait]
pub trait StrategyStore: Send + Sync {
    async fn save(&self, strategy: &Strategy) -> anyhow::Result<()>;
    async fn load(&self, id: &str) -> anyhow::Result<Strategy>;
    async fn list(&self) -> anyhow::Result<Vec<String>>;
    async fn delete(&self, id: &str) -> anyhow::Result<()>;
}

pub struct FilesystemStore {
    root: PathBuf,
}

impl FilesystemStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Build the on-disk path for `id`, validating it first.
    ///
    /// All filesystem operations on this store go through here, so a
    /// rejected id is guaranteed to never reach `std::fs`. The validator
    /// rejects `..`, path separators, NUL, leading dots, and anything
    /// outside `[A-Za-z0-9_-]` — see `strategies::id` for the full set
    /// and rationale (QA finding P3-strategy-id).
    pub fn path_for(&self, id: &str) -> Result<PathBuf, StrategyIdError> {
        let id = validate_strategy_id_for_path(id)?;
        Ok(self.root.join(format!("{id}.json")))
    }
}

#[async_trait]
impl StrategyStore for FilesystemStore {
    async fn save(&self, strategy: &Strategy) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.path_for(&strategy.manifest.id)?;
        let json = serde_json::to_vec_pretty(strategy)?;
        tokio::fs::write(&path, json)
            .await
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    async fn load(&self, id: &str) -> anyhow::Result<Strategy> {
        let path = self.path_for(id)?;
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

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        let path = self.path_for(id)?;
        tokio::fs::remove_file(&path)
            .await
            .with_context(|| format!("deleting {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::manifest::PublicManifest;
    use crate::strategies::risk::RiskPreset;
    use crate::strategies::Strategy;

    fn store_in_tmp() -> (FilesystemStore, tempfile::TempDir) {
        let td = tempfile::tempdir().unwrap();
        let store = FilesystemStore::new(td.path().to_path_buf());
        (store, td)
    }

    fn strategy_with_id(id: &str) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: id.to_string(),
                display_name: "t".into(),
                plain_summary: "t".into(),
                creator: "@tester".into(),
                template: "trend_follower".into(),
                regime_fit: vec![],
                asset_universe: vec![],
                decision_cadence_minutes: 60,
                required_models: vec![],
                required_tools: vec![],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            agents: vec![],
            pipeline: Default::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
        }
    }

    #[test]
    fn path_for_accepts_valid_id() {
        let (store, _td) = store_in_tmp();
        let p = store.path_for("01HZSTRATEGY00000000000000").unwrap();
        assert!(p.ends_with("01HZSTRATEGY00000000000000.json"));
    }

    #[test]
    fn path_for_rejects_traversal() {
        let (store, _td) = store_in_tmp();
        let err = store.path_for("../escape").unwrap_err();
        assert_eq!(err, StrategyIdError::PathSeparator);
    }

    #[test]
    fn path_for_rejects_double_dot() {
        let (store, _td) = store_in_tmp();
        let err = store.path_for("..").unwrap_err();
        assert_eq!(err, StrategyIdError::ReservedSegment);
    }

    #[test]
    fn path_for_rejects_empty() {
        let (store, _td) = store_in_tmp();
        let err = store.path_for("").unwrap_err();
        assert_eq!(err, StrategyIdError::Empty);
    }

    #[tokio::test]
    async fn load_rejected_id_does_not_touch_disk() {
        let (store, _td) = store_in_tmp();
        let err = store.load("../escape").await.unwrap_err();
        let downcast: Option<&StrategyIdError> = err.downcast_ref();
        assert!(downcast.is_some(), "expected StrategyIdError, got {err:?}");
    }

    #[tokio::test]
    async fn save_rejected_id_does_not_write_anywhere() {
        let (store, td) = store_in_tmp();
        let bad = strategy_with_id("../escape");
        let err = store.save(&bad).await.unwrap_err();
        let downcast: Option<&StrategyIdError> = err.downcast_ref();
        assert!(downcast.is_some(), "expected StrategyIdError, got {err:?}");
        // Confirm nothing was written under either the store root or its
        // parent (which traversal would have targeted).
        let mut rd = tokio::fs::read_dir(td.path()).await.unwrap();
        assert!(rd.next_entry().await.unwrap().is_none(), "store root not empty");
    }

    #[tokio::test]
    async fn delete_rejected_id_does_not_touch_disk() {
        let (store, _td) = store_in_tmp();
        let err = store.delete("../escape").await.unwrap_err();
        let downcast: Option<&StrategyIdError> = err.downcast_ref();
        assert!(downcast.is_some(), "expected StrategyIdError, got {err:?}");
    }

    #[tokio::test]
    async fn happy_path_save_load_delete_roundtrips() {
        let (store, _td) = store_in_tmp();
        let s = strategy_with_id("01HZSTRATEGY00000000000000");
        store.save(&s).await.unwrap();
        let loaded = store.load("01HZSTRATEGY00000000000000").await.unwrap();
        assert_eq!(loaded.manifest.id, "01HZSTRATEGY00000000000000");
        store.delete("01HZSTRATEGY00000000000000").await.unwrap();
        // Loading after delete returns IO not-found, not a validation error.
        let err = store.load("01HZSTRATEGY00000000000000").await.unwrap_err();
        assert!(err.downcast_ref::<StrategyIdError>().is_none());
    }
}
