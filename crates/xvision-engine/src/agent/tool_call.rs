//! Tool-call helpers for the `execute_slot` tool-use loop.
//!
//! Plan 2a Phase 2A.C T11. The slot's `allowed_tools` list is converted
//! into a `Vec<ToolDefinition>` advertised to the LLM each turn; on
//! `ContentBlock::ToolUse` blocks in the response we look the tool up in
//! the registry, invoke it, and feed the JSON-stringified result back
//! into the next turn as a `ToolResult` block.

use std::sync::Arc;

use crate::agent::llm::{ContentBlock, ToolDefinition};
use crate::tools::{ToolName, ToolRegistry};

/// Build tool definitions from a slot's `allowed_tools`. Tools the registry
/// doesn't know are silently dropped (strategy validation should catch
/// misspelled names at draft time). v1 input schemas are minimal — Plan 2c
/// will add per-tool argument schemas; for now every tool gets `{type:
/// "object"}` and the model is expected to pass the right shape based on
/// the tool description and prompt.
pub fn definitions_for_slot(
    allowed_tools: &[String],
    registry: &ToolRegistry,
) -> Vec<ToolDefinition> {
    allowed_tools
        .iter()
        .filter_map(|name| {
            registry
                .get(&ToolName::new(name.clone()))
                .map(|tool| ToolDefinition {
                    name: name.clone(),
                    description: tool.description().to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                })
        })
        .collect()
}

/// Invoke a tool by name and return a stringified result for inclusion in
/// the next message as a `ToolResult` content block. Errors surface as
/// strings so the model can recover (vs. tearing down the whole turn).
pub async fn invoke(
    name: &str,
    input: serde_json::Value,
    registry: Arc<ToolRegistry>,
) -> anyhow::Result<String> {
    let tool = registry
        .get(&ToolName::new(name.to_string()))
        .ok_or_else(|| anyhow::anyhow!("tool '{name}' not in registry"))?;
    let out = tool.invoke(input).await?;
    Ok(out.to_string())
}

/// Extract every `ToolUse` block from a response in `(id, name, input)`
/// triples — the routing surface for `execute_slot`'s loop.
pub(crate) fn tool_uses(
    content: &[ContentBlock],
) -> Vec<(String, String, serde_json::Value)> {
    content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::ToolUse { id, name, input } => {
                Some((id.clone(), name.clone(), input.clone()))
            }
            _ => None,
        })
        .collect()
}
