use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Prose,
    Param,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProseEdit {
    pub agent_role: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamChange {
    pub key: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationDiff {
    pub kind: MutationKind,
    pub prose: Vec<ProseEdit>,
    pub params: Vec<ParamChange>,
    pub tools: ToolDiff,
    pub rationale: String,
}

pub fn empty_mutation() -> MutationDiff {
    MutationDiff {
        kind: MutationKind::Prose,
        prose: Vec::new(),
        params: Vec::new(),
        tools: ToolDiff { added: Vec::new(), removed: Vec::new() },
        rationale: String::new(),
    }
}

impl MutationDiff {
    pub fn is_empty(&self) -> bool {
        self.prose.is_empty()
            && self.params.is_empty()
            && self.tools.added.is_empty()
            && self.tools.removed.is_empty()
    }
}
