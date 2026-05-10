//! Server-side LLM agent that drives the strategy authoring loop. The user
//! sends one chat message; this struct repeatedly calls the LLM with the
//! seven authoring verbs as `ToolDefinition`s, routes any `ToolUse` blocks
//! the model emits to `xvision_engine::authoring`, appends the
//! `ToolResult`s to the conversation, and re-calls until the model
//! responds with a text-only `EndTurn`.
//!
//! Plan 2d Phase 2D.B Task 6. Stacks on Plan 2a Phase 2A.B (PR #31; the
//! seven authoring verbs in the engine module) and Phase 2A.C T10 (PR
//! #33; the multi-turn `Message`/`ContentBlock`/`ToolDefinition` shape).
//!
//! Deliberately surface-agnostic at this layer — the SSE route in
//! `routes::wizard` (follow-up) wraps `WizardEvent`s into an
//! `event-stream` body. Tests drive the loop directly with a
//! `MockDispatch::sequence(...)`.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, StopReason, ToolDefinition,
};
use xvision_engine::authoring;
use xvision_engine::bundle::store::FilesystemStore;

const WIZARD_SYSTEM_PROMPT: &str = include_str!("../prompts/wizard.md");

/// Cap on tool-use → tool-result iterations per `next_event` call. Prevents
/// a misbehaving model from looping forever; v1 wizards never need more
/// than 3-4 round trips per user turn.
const MAX_TOOL_LOOP_ITERATIONS: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WizardEvent {
    /// A chunk of assistant text. The SSE route streams these as they
    /// arrive; clients append to the visible bubble.
    Token { text: String },
    /// The agent is about to call an authoring verb. Front-end uses this
    /// to render a "running tool" indicator.
    ToolCall {
        tool: String,
        args: serde_json::Value,
    },
    /// Result of the most-recent tool call. Front-end uses this to update
    /// the displayed draft state.
    ToolResult {
        tool: String,
        result: serde_json::Value,
    },
    /// Conversation complete. `draft_id` carries the most recently created
    /// or referenced strategy id (if any), so the front-end can transition
    /// to the inspector view.
    Done { draft_id: Option<String> },
    /// The dispatch errored or the loop hit a hard cap.
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// User's chat message for this turn.
    pub message: String,
    /// Anthropic model id (e.g., `claude-sonnet-4-6`).
    pub model: String,
}

pub struct WizardLoop {
    xvn_home: PathBuf,
    dispatch: Arc<dyn LlmDispatch>,
    model: String,
    messages: Vec<Message>,
    /// Tracked across iterations: the most recent strategy id mentioned in
    /// a tool-call/-result. Used to populate `Done.draft_id`.
    last_draft_id: Option<String>,
    /// Pending events queued during the current `next_event` invocation.
    pending: Vec<WizardEvent>,
    is_done: bool,
}

impl WizardLoop {
    pub fn new(xvn_home: PathBuf, dispatch: Arc<dyn LlmDispatch>, req: ChatRequest) -> Self {
        let messages = vec![Message {
            role: "user".into(),
            content: vec![ContentBlock::Text { text: req.message }],
        }];
        Self {
            xvn_home,
            dispatch,
            model: req.model,
            messages,
            last_draft_id: None,
            pending: vec![],
            is_done: false,
        }
    }

    /// Pop one event. The caller streams these to the client one-by-one
    /// (e.g. via SSE); when this returns `None` the loop is finished.
    pub async fn next_event(&mut self) -> Option<WizardEvent> {
        if let Some(ev) = self.pending.pop() {
            return Some(ev);
        }
        if self.is_done {
            return None;
        }
        if let Err(e) = self.run_one_turn().await {
            self.is_done = true;
            return Some(WizardEvent::Error {
                message: e.to_string(),
            });
        }
        // pending is filled in chronological order, but pop() takes from the
        // back — reverse so the caller streams in the right sequence.
        self.pending.reverse();
        self.pending.pop()
    }

    async fn run_one_turn(&mut self) -> anyhow::Result<()> {
        for _ in 0..MAX_TOOL_LOOP_ITERATIONS {
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: WIZARD_SYSTEM_PROMPT.into(),
                messages: self.messages.clone(),
                max_tokens: 1500,
                tools: wizard_tool_defs(),
            };
            let resp: LlmResponse = self.dispatch.complete(req).await?;

            // Emit Token events for any text blocks the model produced.
            for block in &resp.content {
                if let ContentBlock::Text { text } = block {
                    if !text.is_empty() {
                        self.pending.push(WizardEvent::Token { text: text.clone() });
                    }
                }
            }

            // Collect tool-use calls. If there are none, the loop ends:
            // emit Done and return.
            let tool_uses: Vec<(String, String, serde_json::Value)> = resp
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect();

            if tool_uses.is_empty() {
                self.is_done = true;
                self.pending.push(WizardEvent::Done {
                    draft_id: self.last_draft_id.clone(),
                });
                return Ok(());
            }

            // Append the assistant turn (with the tool_use blocks) so the
            // next call sees its own request.
            self.messages.push(Message {
                role: "assistant".into(),
                content: resp.content.clone(),
            });

            // Run each tool, build a tool_result block per call, emit
            // ToolCall + ToolResult WizardEvents.
            let mut tool_results: Vec<ContentBlock> = Vec::with_capacity(tool_uses.len());
            for (id, name, input) in tool_uses {
                self.pending.push(WizardEvent::ToolCall {
                    tool: name.clone(),
                    args: input.clone(),
                });
                let result = self.run_tool(&name, input).await;
                let result_value = match result {
                    Ok(v) => v,
                    Err(e) => serde_json::json!({ "error": e.to_string() }),
                };
                self.maybe_track_draft_id(&name, &result_value);
                self.pending.push(WizardEvent::ToolResult {
                    tool: name.clone(),
                    result: result_value.clone(),
                });
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: id,
                    content: result_value.to_string(),
                });
            }

            // Append the user turn carrying the tool_result blocks for the
            // next iteration.
            self.messages.push(Message {
                role: "user".into(),
                content: tool_results,
            });

            if !matches!(resp.stop_reason, StopReason::ToolUse) {
                // Defensive: the model said EndTurn/MaxTokens but emitted
                // tool_uses. Anthropic shouldn't do this, but if it does
                // we've already enqueued the tool_results; finish the turn.
                self.is_done = true;
                self.pending.push(WizardEvent::Done {
                    draft_id: self.last_draft_id.clone(),
                });
                return Ok(());
            }
        }
        anyhow::bail!(
            "wizard tool-use loop exceeded {MAX_TOOL_LOOP_ITERATIONS} iterations \
             — model is stuck calling tools without responding"
        );
    }

    fn maybe_track_draft_id(&mut self, tool: &str, result: &serde_json::Value) {
        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            self.last_draft_id = Some(id.to_string());
            return;
        }
        // For get_strategy the bundle's manifest.id is what we want.
        if tool == "get_strategy" {
            if let Some(id) = result
                .get("manifest")
                .and_then(|m| m.get("id"))
                .and_then(|v| v.as_str())
            {
                self.last_draft_id = Some(id.to_string());
            }
        }
    }

    async fn run_tool(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let store = FilesystemStore::new(self.xvn_home.join("strategies"));
        match name {
            "list_templates" => {
                let out = authoring::list_templates();
                Ok(serde_json::to_value(out)?)
            }
            "create_strategy" => {
                let req: authoring::CreateStrategyReq = serde_json::from_value(input)?;
                let out = authoring::create_strategy(&store, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "get_strategy" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("get_strategy: missing `id`"))?;
                let out = authoring::get_strategy(&store, id).await?;
                Ok(serde_json::to_value(out)?)
            }
            "update_slot" => {
                let req: authoring::UpdateSlotReq = serde_json::from_value(input)?;
                let out = authoring::update_slot(&store, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "set_mechanical_param" => {
                let req: authoring::SetMechanicalParamReq = serde_json::from_value(input)?;
                authoring::set_mechanical_param(&store, req).await?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "set_risk_config" => {
                let req: authoring::SetRiskConfigReq = serde_json::from_value(input)?;
                let out = authoring::set_risk_config(&store, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "validate_draft" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("validate_draft: missing `id`"))?;
                let out = authoring::validate_draft(&store, id).await?;
                Ok(serde_json::to_value(out)?)
            }
            other => anyhow::bail!("unknown authoring verb: {other}"),
        }
    }
}

/// The seven authoring verbs as `ToolDefinition`s. The schemas mirror the
/// engine's request structs but only declare the fields a model needs;
/// optional fields are omitted from `required`.
fn wizard_tool_defs() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_templates".into(),
            description: "List the strategy templates with display name + plain summary.".into(),
            input_schema: serde_json::json!({
                "type": "object", "properties": {}, "required": []
            }),
        },
        ToolDefinition {
            name: "create_strategy".into(),
            description: "Instantiate a new draft from a template. Returns { id }.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "template": {"type": "string", "description": "Template name from list_templates"},
                    "name": {"type": "string", "description": "Human-readable name"},
                    "creator": {"type": "string", "description": "Optional @handle"}
                },
                "required": ["template", "name"]
            }),
        },
        ToolDefinition {
            name: "get_strategy".into(),
            description: "Read the current draft state. Returns the StrategyBundle JSON.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "update_slot".into(),
            description: "Update a regime/intern/trader slot. Only fields with non-null values are mutated.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "slot": {"type": "string", "enum": ["regime", "intern", "trader"]},
                    "prompt": {"type": "string"},
                    "model_requirement": {"type": "string"},
                    "allowed_tools": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["id", "slot"]
            }),
        },
        ToolDefinition {
            name: "set_mechanical_param".into(),
            description: "Set a key inside bundle.mechanical_params (template-specific).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "key": {"type": "string"},
                    "value": {}
                },
                "required": ["id", "key", "value"]
            }),
        },
        ToolDefinition {
            name: "set_risk_config".into(),
            description: "Apply a risk preset (conservative/balanced/aggressive) OR an explicit RiskConfig. Mutually exclusive.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "preset": {"type": "string", "enum": ["conservative", "balanced", "aggressive"]},
                    "explicit": {"type": "object"}
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "validate_draft".into(),
            description: "Validate a strategy draft. Returns { id, ok, errors }.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_engine::agent::llm::{LlmResponse, MockDispatch};

    fn loop_with_tmp(dispatch: Arc<dyn LlmDispatch>, msg: &str) -> (WizardLoop, tempfile::TempDir) {
        let td = tempfile::tempdir().unwrap();
        let wl = WizardLoop::new(
            td.path().to_path_buf(),
            dispatch,
            ChatRequest {
                message: msg.into(),
                model: "claude-sonnet-4-6".into(),
            },
        );
        (wl, td)
    }

    async fn drain(wl: &mut WizardLoop) -> Vec<WizardEvent> {
        let mut out = vec![];
        while let Some(ev) = wl.next_event().await {
            out.push(ev);
        }
        out
    }

    #[tokio::test]
    async fn text_only_response_emits_token_then_done() {
        let mock = Arc::new(MockDispatch::echo("Sure — which template?"));
        let (mut wl, _td) = loop_with_tmp(mock, "help me build a strategy");
        let events = drain(&mut wl).await;
        assert_eq!(events.len(), 2, "events: {events:?}");
        assert!(matches!(&events[0], WizardEvent::Token { text } if text.contains("which template")));
        assert!(matches!(&events[1], WizardEvent::Done { draft_id: None }));
    }

    #[tokio::test]
    async fn tool_use_runs_authoring_verb_and_appends_text() {
        // First turn: model wants to list templates. Second turn: text reply.
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use("tu_1", "list_templates", serde_json::json!({})),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "I see 9 templates. Want trend_follower?".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _td) = loop_with_tmp(mock, "what can i build");
        let events = drain(&mut wl).await;

        // Sequence: ToolCall → ToolResult → Token → Done.
        assert!(matches!(&events[0], WizardEvent::ToolCall { tool, .. } if tool == "list_templates"));
        match &events[1] {
            WizardEvent::ToolResult { tool, result } => {
                assert_eq!(tool, "list_templates");
                assert!(result.as_array().unwrap().len() >= 8);
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
        assert!(matches!(&events[2], WizardEvent::Token { text } if text.contains("template")));
        assert!(matches!(&events[3], WizardEvent::Done { .. }));
    }

    #[tokio::test]
    async fn create_strategy_round_trip_tracks_draft_id() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_1",
                "create_strategy",
                serde_json::json!({"template": "trend_follower", "name": "btc-tf-1"}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Created. Want me to validate?".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _td) = loop_with_tmp(mock, "make me a trend follower");
        let events = drain(&mut wl).await;
        // The Done event should carry a draft_id (the ULID returned by create_strategy).
        let done = events.iter().rev().find(|e| matches!(e, WizardEvent::Done { .. }));
        match done {
            Some(WizardEvent::Done { draft_id: Some(id) }) => {
                assert!(!id.is_empty());
            }
            other => panic!("expected Done with draft_id, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_template_surfaces_as_tool_result_error() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_1",
                "create_strategy",
                serde_json::json!({"template": "nope", "name": "x"}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "That template doesn't exist. Try one of these: ...".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _td) = loop_with_tmp(mock, "make me a nope");
        let events = drain(&mut wl).await;
        let result = events
            .iter()
            .find(|e| matches!(e, WizardEvent::ToolResult { tool, .. } if tool == "create_strategy"));
        match result {
            Some(WizardEvent::ToolResult { result, .. }) => {
                assert!(
                    result.get("error").is_some(),
                    "expected error key in {result}"
                );
            }
            other => panic!("expected ToolResult with error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_tool_name_surfaces_as_tool_result_error() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use("tu_1", "fly_to_moon", serde_json::json!({})),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "ok stopping".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _td) = loop_with_tmp(mock, "go");
        let events = drain(&mut wl).await;
        let result = events
            .iter()
            .find(|e| matches!(e, WizardEvent::ToolResult { tool, .. } if tool == "fly_to_moon"));
        match result {
            Some(WizardEvent::ToolResult { result, .. }) => {
                assert!(result.get("error").is_some());
            }
            other => panic!("expected ToolResult with error, got {other:?}"),
        }
    }

    #[test]
    fn wizard_tool_defs_advertises_seven_verbs() {
        let defs = wizard_tool_defs();
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names.len(), 7);
        for v in [
            "list_templates",
            "create_strategy",
            "get_strategy",
            "update_slot",
            "set_mechanical_param",
            "set_risk_config",
            "validate_draft",
        ] {
            assert!(names.contains(&v), "missing verb {v} in {names:?}");
        }
    }
}
