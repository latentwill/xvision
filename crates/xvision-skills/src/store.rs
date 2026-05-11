use std::path::PathBuf;

use anyhow::Context;
use async_trait::async_trait;

use crate::Skill;

#[async_trait]
pub trait SkillStore: Send + Sync {
    /// Save the original markdown source so the body roundtrips byte-exact.
    async fn save(&self, name: &str, markdown: &str) -> anyhow::Result<()>;
    async fn load(&self, name: &str) -> anyhow::Result<Skill>;
    async fn list(&self) -> anyhow::Result<Vec<String>>;
}

pub struct FilesystemSkillStore {
    root: PathBuf,
}

impl FilesystemSkillStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.md"))
    }
}

#[async_trait]
impl SkillStore for FilesystemSkillStore {
    async fn save(&self, name: &str, markdown: &str) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.path_for(name);
        tokio::fs::write(&path, markdown)
            .await
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    async fn load(&self, name: &str) -> anyhow::Result<Skill> {
        let path = self.path_for(name);
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(crate::parse(std::str::from_utf8(&bytes)?)?)
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        if !self.root.exists() {
            return Ok(vec![]);
        }
        let mut out = vec![];
        let mut rd = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = rd.next_entry().await? {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if let Some(stem) = s.strip_suffix(".md") {
                out.push(stem.to_string());
            }
        }
        out.sort();
        Ok(out)
    }
}
