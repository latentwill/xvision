use serde::Serialize;
use serde_json::Value;

use crate::tools::built_in_tool_descriptors;

#[derive(Debug, Clone, Serialize)]
pub struct ToolCatalogEntry {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub built_in: bool,
}

pub async fn list_tools() -> Vec<ToolCatalogEntry> {
    built_in_tool_descriptors()
        .into_iter()
        .map(|descriptor| ToolCatalogEntry {
            name: descriptor.name,
            description: descriptor.description,
            input_schema: descriptor.input_schema,
            built_in: true,
        })
        .collect()
}
