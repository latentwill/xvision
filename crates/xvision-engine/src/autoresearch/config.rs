use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoresearchConfig {
    #[serde(default)]
    pub gate: GateConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    pub min_improvement: f64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self { min_improvement: 0.10 }
    }
}

impl AutoresearchConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading autoresearch config from {}", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("parsing autoresearch config from {}", path.display()))
    }

    pub fn validate(&self) -> Result<()> {
        if self.gate.min_improvement <= 0.0 {
            anyhow::bail!(
                "min-improvement must be greater than zero (got {})",
                self.gate.min_improvement
            );
        }
        Ok(())
    }

    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("no home directory found")?;
        Ok(home.join(".xvn").join("autoresearch.toml"))
    }
}
