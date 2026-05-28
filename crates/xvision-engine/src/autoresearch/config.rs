use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

fn default_mutation_kinds() -> Vec<String> {
    vec!["prose".into(), "param".into(), "tool".into()]
}

fn default_min_improvement() -> f64 {
    0.05
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    /// Operator-surface name: `--min-improvement`.
    #[serde(default = "default_min_improvement")]
    pub min_improvement: f64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self { min_improvement: default_min_improvement() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoresearchConfig {
    #[serde(default = "default_mutation_kinds")]
    pub allowed_mutation_kinds: Vec<String>,
    #[serde(default)]
    pub gate: GateConfig,
}

impl Default for AutoresearchConfig {
    fn default() -> Self {
        Self {
            allowed_mutation_kinds: default_mutation_kinds(),
            gate: GateConfig::default(),
        }
    }
}

impl AutoresearchConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config from {}", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("parsing config from {}", path.display()))
    }

    /// Returns an error if any field violates operator-visible invariants.
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.gate.min_improvement > 0.0,
            "--min-improvement must be positive (got {})",
            self.gate.min_improvement
        );
        Ok(())
    }
}
