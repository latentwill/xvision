use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoresearchConfig {
    pub allowed_mutation_kinds: Vec<String>,
}

impl Default for AutoresearchConfig {
    fn default() -> Self {
        Self {
            allowed_mutation_kinds: vec!["prose".into(), "param".into(), "tool".into()],
        }
    }
}
