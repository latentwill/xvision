//! Public value types for xvision-memory.

use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    #[default]
    Off,
    Global,
    AgentScoped,
}

impl MemoryMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryMode::Off => "off",
            MemoryMode::Global => "global",
            MemoryMode::AgentScoped => "agent_scoped",
        }
    }

    pub fn parse_or_off(s: &str) -> Self {
        match s {
            "global" => MemoryMode::Global,
            "agent_scoped" => MemoryMode::AgentScoped,
            _ => MemoryMode::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace(String);

impl Namespace {
    pub fn for_mode(mode: MemoryMode, agent_id: &str) -> Self {
        match mode {
            MemoryMode::Off => Namespace(String::new()),
            MemoryMode::Global => Namespace("global".to_string()),
            MemoryMode::AgentScoped => Namespace(format!("agent:{agent_id}")),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_active(&self) -> bool {
        !self.0.is_empty()
    }
}

/// Cortex tier — episodic Observation vs. semantic Pattern. The store
/// enforces tier-shape invariants at write time, and `query` filters
/// to Pattern-only so Observations are never recalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Episodic — concrete observation with mandatory provenance.
    /// Never recalled at decision time.
    Observation,
    /// Semantic — abstracted pattern. Recalled at decision time
    /// (subject to the time-window filter).
    Pattern,
}

impl Tier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Observation => "observation",
            Tier::Pattern => "pattern",
        }
    }
    pub fn parse_or_observation(s: &str) -> Self {
        match s {
            "pattern" => Tier::Pattern,
            _ => Tier::Observation,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryItem {
    pub id: String,
    pub namespace: String,
    pub tier: Tier,
    pub text: String,
    pub embedding: Vec<f32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Provenance — REQUIRED on Observations, MUST be `None` on
    /// Patterns. The store enforces this at write time.
    pub run_id: Option<String>,
    pub scenario_id: Option<String>,
    pub cycle_idx: Option<i64>,
    /// Latest bar timestamp across the Observations that contributed
    /// to this Pattern. REQUIRED on autoresearcher-distilled Patterns;
    /// MAY be `None` on operator-attested manual seeds (recalled in
    /// every scenario; operator owns the safety guarantee). MUST be
    /// `None` on Observations.
    pub training_window_end: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryMatch {
    pub id: String,
    pub text: String,
    pub score: f32,
}
