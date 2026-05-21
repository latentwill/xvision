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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryItem {
    pub id: String,
    pub namespace: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub source_run_id: Option<String>,
    pub source_cycle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryMatch {
    pub id: String,
    pub text: String,
    pub score: f32,
}
