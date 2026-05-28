use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LooseningSchedule {
    pub day_n_thresholds: Vec<f64>,
}

fn default_base_min_improvement() -> f64 {
    0.10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoresearchConfig {
    pub allowed_mutation_kinds: Vec<String>,
    #[serde(default = "default_base_min_improvement")]
    pub base_min_improvement: f64,
    #[serde(default)]
    pub loosening_schedule: LooseningSchedule,
}

impl Default for AutoresearchConfig {
    fn default() -> Self {
        Self {
            allowed_mutation_kinds: vec!["prose".into(), "param".into(), "tool".into()],
            base_min_improvement: default_base_min_improvement(),
            loosening_schedule: LooseningSchedule::default(),
        }
    }
}
