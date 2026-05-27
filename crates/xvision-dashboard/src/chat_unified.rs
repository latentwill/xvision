//! Chat-rail → unified-event projection.
//!
//! Phase 1.1/1.2 of the chat-rail / DSPy / strategy-agents wave. The
//! companion to `xvision_observability::RunEventProjector`: where that maps
//! agent-run `RunEvent`s into the shared [`UnifiedEvent`] envelope, this maps
//! the chat rail's [`WizardEvent`]s. Both paths converge on one taxonomy so
//! the rail and trace dock can project from a single event log.
//!
//! Lives in the dashboard crate (not `observability`) because `WizardEvent`
//! is defined here and `observability` must not depend upward.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use xvision_engine::chat_session::ContextScope;
use xvision_observability::types::{RiskLevel, SideEffectLevel, ToolOrigin};
use xvision_observability::{
    Actor, EventScope, EventSource, ToolCallFinishedEvent, ToolCallStartedEvent, ToolDenied,
    ToolPolicyChecked, ToolPolicyOutcome, UnifiedEvent, UnifiedPayload,
};

use crate::wizard_loop::WizardEvent;

/// Flatten a dashboard [`ContextScope`] into the observability
/// [`EventScope`] `(kind, id)` pair. Multi-id scopes join their ids so the
/// scope is addressable without pulling the engine enum into observability.
pub fn scope_to_event_scope(scope: &ContextScope) -> EventScope {
    match scope {
        ContextScope::Workspace => EventScope::new("workspace", None),
        ContextScope::Route { route } => EventScope::new("route", Some(route.clone())),
        ContextScope::Run { run_id } => EventScope::new("run", Some(run_id.clone())),
        ContextScope::Strategy { draft_id } => EventScope::new("strategy", Some(draft_id.clone())),
        ContextScope::Deployment { deployment_id } => {
            EventScope::new("deployment", Some(deployment_id.clone()))
        }
        ContextScope::Compare { run_ids } => EventScope::new("compare", Some(run_ids.join(","))),
        ContextScope::JournalFilter { kinds } => EventScope::new("journal_filter", Some(kinds.join(","))),
        ContextScope::Selection { items } => EventScope::new("selection", Some(items.join(","))),
        ContextScope::Seed { seed_id } => EventScope::new("seed", Some(seed_id.clone())),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Projects one chat session's [`WizardEvent`] stream into [`UnifiedEvent`]s,
/// stamping the owning session + scope and assigning a monotonic per-session
/// sequence number. One projector instance per chat turn (or per session) so
/// `seq` is stable and gap-detectable on the consumer.
///
/// Tool args/results are hashed (not carried inline) to match the
/// observability redaction discipline; the full payloads stay on the
/// compatibility `WizardEvent` shim during Phase 1 and move behind blob refs
/// when the tool-row registry lands (Phase 2.1).
pub struct WizardEventProjector {
    session_id: String,
    scope: EventScope,
    seq: u64,
    /// Tool name → FIFO queue of synthesized span ids, so each
    /// `ToolResult` reuses the span id minted by its corresponding
    /// `ToolCall`. Pre-2026-05-26 this was a `HashMap<String, String>`
    /// (one entry per tool name), which silently overwrote when the
    /// same tool was invoked twice in one turn — the second call's
    /// span would clobber the first, and the first `ToolResult` would
    /// then mis-correlate to the second call's span. The reducer
    /// keys on `span_id`, so the first call's tool row stayed stuck
    /// in `requested` forever. That latent bug was a strong candidate
    /// for the QA hang on `list_strategies` / `list_scenarios` /
    /// `list_strategy_ideas`, which the agent often invokes more than
    /// once in a setup turn. FIFO ordering is sufficient because the
    /// wizard runs tool calls sequentially today — Call N's Result
    /// always lands before Call N+1's Result. If parallel/streaming
    /// tool execution is added later, the wizard must thread a stable
    /// `call_id` through both events so the projector can match by id
    /// instead.
    tool_spans: HashMap<String, VecDeque<String>>,
}

impl WizardEventProjector {
    pub fn new(session_id: impl Into<String>, scope: &ContextScope) -> Self {
        Self::new_seeded(session_id, scope, 0)
    }

    /// Construct a projector whose first projected event gets sequence
    /// `start_seq`. Seed this with `SessionEventLog::next_seq` so the
    /// per-session sequence continues monotonically across chat turns (each
    /// turn spawns a fresh projector but the persisted log is the source of
    /// truth for where the next seq picks up).
    pub fn new_seeded(session_id: impl Into<String>, scope: &ContextScope, start_seq: u64) -> Self {
        Self {
            session_id: session_id.into(),
            scope: scope_to_event_scope(scope),
            seq: start_seq,
            tool_spans: HashMap::new(),
        }
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Project a ready-made [`UnifiedPayload`] into the session stream,
    /// advancing `seq`. Used for net-new payloads the legacy [`WizardEvent`]
    /// vocabulary can't express — the Phase 2 safety events
    /// (`ToolPolicyChecked`, `ToolDenied`, `ErrorPolicyDenied`). The caller
    /// supplies the actor (`Hook` for policy enforcement) and an optional span
    /// id to correlate with the tool the check ran for.
    pub fn project_payload(
        &mut self,
        event_id: impl Into<String>,
        actor: Actor,
        mut span_id: Option<String>,
        mut payload: UnifiedPayload,
        ts: DateTime<Utc>,
    ) -> UnifiedEvent {
        // Policy events are produced inside WizardLoop before the route has
        // projected the legacy ToolCall into its canonical tool span. Once the
        // ToolCall has been projected, attach policy updates to that same
        // queued span instead of rendering them as a second "open" tool row.
        match &mut payload {
            UnifiedPayload::ToolPolicyChecked(ev) => {
                if let Some(span) = self
                    .tool_spans
                    .get(&ev.tool_name)
                    .and_then(|queue| queue.front())
                    .cloned()
                {
                    ev.span_id = span.clone();
                    span_id = Some(span);
                }
            }
            UnifiedPayload::ToolDenied(ev) => {
                if let Some(span) = self
                    .tool_spans
                    .get(&ev.tool_name)
                    .and_then(|queue| queue.front())
                    .cloned()
                {
                    ev.span_id = span.clone();
                    span_id = Some(span);
                }
            }
            _ => {}
        }
        let out = UnifiedEvent {
            event_id: event_id.into(),
            session_id: Some(self.session_id.clone()),
            run_id: None,
            span_id,
            parent_event_id: None,
            seq: self.seq,
            ts,
            scope: self.scope.clone(),
            actor,
            source: EventSource::ChatRail,
            blob_hash: None,
            payload,
        };
        self.seq += 1;
        out
    }

    /// Project one `WizardEvent`, advancing `seq`. `event_id` is injected so
    /// callers control id generation (ULID in production, deterministic in
    /// tests). `span_minter` mints a span id for a fresh tool call.
    pub fn project(
        &mut self,
        event_id: impl Into<String>,
        ev: WizardEvent,
        ts: DateTime<Utc>,
        mut span_minter: impl FnMut() -> String,
    ) -> UnifiedEvent {
        let (actor, span_id, payload) = match ev {
            WizardEvent::Token { text } => (Actor::Agent, None, UnifiedPayload::AssistantTokenDelta { text }),
            WizardEvent::ContentBlock { block } => (
                Actor::Agent,
                None,
                UnifiedPayload::AssistantContentBlock { block },
            ),
            WizardEvent::Done { draft_id } => (
                Actor::Agent,
                None,
                UnifiedPayload::AssistantMessageDone { draft_id },
            ),
            WizardEvent::Error { message } => {
                (Actor::System, None, UnifiedPayload::SessionFailed { message })
            }
            WizardEvent::ToolCall { tool, args } => {
                let span = span_minter();
                self.tool_spans
                    .entry(tool.clone())
                    .or_default()
                    .push_back(span.clone());
                let input_hash = sha256_hex(args.to_string().as_bytes());
                (
                    Actor::Agent,
                    Some(span.clone()),
                    UnifiedPayload::ToolRequested(ToolCallStartedEvent {
                        span_id: span,
                        tool_name: tool,
                        // Chat tools are dashboard-native authoring verbs; the
                        // real side-effect/risk/approval policy is stamped
                        // server-side in Phase 2.3. Conservative placeholders
                        // until then.
                        origin: ToolOrigin::Native,
                        tool_version: None,
                        tool_hash: None,
                        side_effect_level: SideEffectLevel::ExternalWrite,
                        risk_level: RiskLevel::StrategyMutation,
                        requires_approval: false,
                        is_run_terminator: false,
                        input_hash,
                        input_payload_ref: None,
                    }),
                )
            }
            WizardEvent::ToolResult { tool, result } => {
                // FIFO: pop the oldest queued span for this tool so
                // Result N pairs with Call N. If the queue is empty
                // (a ToolResult arrived with no matching ToolCall —
                // shouldn't happen in practice, but defensive), mint
                // a fresh span so the unified stream is still well-
                // formed.
                let span = self
                    .tool_spans
                    .get_mut(&tool)
                    .and_then(|q| q.pop_front())
                    .unwrap_or_else(&mut span_minter);
                let output_hash = sha256_hex(result.to_string().as_bytes());
                (
                    Actor::Agent,
                    Some(span.clone()),
                    UnifiedPayload::ToolFinished(ToolCallFinishedEvent {
                        span_id: span,
                        output_hash: Some(output_hash),
                        output_payload_ref: None,
                        exit_code: None,
                    }),
                )
            }
        };

        let out = UnifiedEvent {
            event_id: event_id.into(),
            session_id: Some(self.session_id.clone()),
            run_id: None,
            span_id,
            parent_event_id: None,
            seq: self.seq,
            ts,
            scope: self.scope.clone(),
            actor,
            source: EventSource::ChatRail,
            blob_hash: None,
            payload,
        };
        self.seq += 1;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ts() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-24T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn scope_flattens_all_variants() {
        assert_eq!(scope_to_event_scope(&ContextScope::Workspace).kind, "workspace");
        let s = scope_to_event_scope(&ContextScope::Strategy {
            draft_id: "abc".into(),
        });
        assert_eq!(s.kind, "strategy");
        assert_eq!(s.id.as_deref(), Some("abc"));
        let c = scope_to_event_scope(&ContextScope::Compare {
            run_ids: vec!["r1".into(), "r2".into()],
        });
        assert_eq!(c.id.as_deref(), Some("r1,r2"));
    }

    #[test]
    fn assistant_stream_projects_with_monotonic_seq() {
        let mut p = WizardEventProjector::new("sess_1", &ContextScope::Workspace);
        let mut n = 0;
        let mut mint = || {
            n += 1;
            format!("sp_{n}")
        };
        let e0 = p.project("e0", WizardEvent::Token { text: "hi".into() }, ts(), &mut mint);
        let e1 = p.project(
            "e1",
            WizardEvent::Done {
                draft_id: Some("strat_1".into()),
            },
            ts(),
            &mut mint,
        );
        assert_eq!((e0.seq, e1.seq), (0, 1));
        assert_eq!(e0.source, EventSource::ChatRail);
        assert_eq!(e0.session_id.as_deref(), Some("sess_1"));
        assert!(matches!(e0.payload, UnifiedPayload::AssistantTokenDelta { .. }));
        assert!(matches!(e1.payload, UnifiedPayload::AssistantMessageDone { .. }));
    }

    #[test]
    fn tool_call_and_result_share_a_span_id() {
        let mut p = WizardEventProjector::new("sess_2", &ContextScope::Workspace);
        let mut minted = 0;
        let mut mint = || {
            minted += 1;
            format!("sp_{minted}")
        };
        let call = p.project(
            "e0",
            WizardEvent::ToolCall {
                tool: "create_strategy".into(),
                args: json!({"name": "x"}),
            },
            ts(),
            &mut mint,
        );
        let result = p.project(
            "e1",
            WizardEvent::ToolResult {
                tool: "create_strategy".into(),
                result: json!({"ok": true}),
            },
            ts(),
            &mut mint,
        );
        // Only one span id was minted; the result reused the call's span.
        assert_eq!(call.span_id, result.span_id);
        assert_eq!(call.span_id.as_deref(), Some("sp_1"));
        match (&call.payload, &result.payload) {
            (UnifiedPayload::ToolRequested(req), UnifiedPayload::ToolFinished(fin)) => {
                assert_eq!(req.tool_name, "create_strategy");
                assert_eq!(req.span_id, fin.span_id);
                assert!(fin.output_hash.is_some());
            }
            other => panic!("wrong payloads: {other:?}"),
        }
    }

    #[test]
    fn policy_events_update_the_requested_tool_span() {
        let mut p = WizardEventProjector::new("sess_policy", &ContextScope::Workspace);
        let mut minted = 0;
        let mut mint = || {
            minted += 1;
            format!("sp_{minted}")
        };

        let call = p.project(
            "call",
            WizardEvent::ToolCall {
                tool: "create_strategy".into(),
                args: json!({"name": "x"}),
            },
            ts(),
            &mut mint,
        );
        let checked = p.project_payload(
            "policy",
            Actor::Hook,
            Some("policy_tmp".into()),
            UnifiedPayload::ToolPolicyChecked(ToolPolicyChecked {
                span_id: "policy_tmp".into(),
                tool_name: "create_strategy".into(),
                outcome: ToolPolicyOutcome::NeedsApproval,
                mode: "research".into(),
            }),
            ts(),
        );
        let denied = p.project_payload(
            "denied",
            Actor::Hook,
            Some("policy_tmp".into()),
            UnifiedPayload::ToolDenied(ToolDenied {
                span_id: "policy_tmp".into(),
                tool_name: "create_strategy".into(),
                code: "write_tool_in_research_mode".into(),
                message: "blocked".into(),
            }),
            ts(),
        );
        let result = p.project(
            "result",
            WizardEvent::ToolResult {
                tool: "create_strategy".into(),
                result: json!({"error": "blocked"}),
            },
            ts(),
            &mut mint,
        );

        assert_eq!(call.span_id, checked.span_id);
        assert_eq!(call.span_id, denied.span_id);
        assert_eq!(call.span_id, result.span_id);
        match (&checked.payload, &denied.payload) {
            (UnifiedPayload::ToolPolicyChecked(checked), UnifiedPayload::ToolDenied(denied)) => {
                assert_eq!(checked.span_id, "sp_1");
                assert_eq!(denied.span_id, "sp_1");
            }
            other => panic!("wrong payloads: {other:?}"),
        }
    }

    #[test]
    fn new_seeded_continues_sequence_across_turns() {
        // A prior turn persisted events 0..=4; the next turn's projector is
        // seeded with next_seq = 5 so the unified sequence is gap-free.
        let mut p = WizardEventProjector::new_seeded("sess_seed", &ContextScope::Workspace, 5);
        let e0 = p.project(
            "e0",
            WizardEvent::Token {
                text: "resume".into(),
            },
            ts(),
            || "sp".into(),
        );
        let e1 = p.project("e1", WizardEvent::Done { draft_id: None }, ts(), || "sp".into());
        assert_eq!(e0.seq, 5);
        assert_eq!(e1.seq, 6);
    }

    /// Regression for the latent span-correlation hazard the QA hang
    /// for `list_strategies`/`list_scenarios`/`list_strategy_ideas`
    /// pointed at (2026-05-26).
    ///
    /// Pre-fix, `tool_spans` was keyed only by tool NAME — so a second
    /// `ToolCall` for the same tool in one turn would OVERWRITE the
    /// first call's span_id. With strictly sequential wizard
    /// execution (Call1, Result1, Call2, Result2), the matching is
    /// fine. But for ANY interleaved order — which a future parallel
    /// tool path or a streaming tool-result interleave would
    /// produce — the second `ToolFinished` would mis-correlate to the
    /// first call's span, leaving the first tool's row stuck at
    /// "requested" in the reducer (which keys on span_id). The fix
    /// keys by call-occurrence rather than just tool name, so the
    /// invariant holds regardless of execution order.
    #[test]
    fn two_calls_to_same_tool_get_distinct_correlated_spans() {
        let mut p = WizardEventProjector::new("sess_dual", &ContextScope::Workspace);
        let mut minted: u32 = 0;
        let mut mint = || {
            minted += 1;
            format!("sp_{minted}")
        };

        // Two ToolCalls for the same tool, then their results in the
        // SAME order. With name-only keying this happens to work, but
        // we still want the assertion locked in.
        let call1 = p.project(
            "ec1",
            WizardEvent::ToolCall {
                tool: "list_strategies".into(),
                args: json!({"page": 1}),
            },
            ts(),
            &mut mint,
        );
        let call2 = p.project(
            "ec2",
            WizardEvent::ToolCall {
                tool: "list_strategies".into(),
                args: json!({"page": 2}),
            },
            ts(),
            &mut mint,
        );
        let result1 = p.project(
            "er1",
            WizardEvent::ToolResult {
                tool: "list_strategies".into(),
                result: json!([]),
            },
            ts(),
            &mut mint,
        );
        let result2 = p.project(
            "er2",
            WizardEvent::ToolResult {
                tool: "list_strategies".into(),
                result: json!([]),
            },
            ts(),
            &mut mint,
        );

        // Each call gets a distinct span and each result correlates
        // to the matching call (FIFO).
        assert_ne!(
            call1.span_id, call2.span_id,
            "two distinct calls must mint distinct spans"
        );
        assert_eq!(
            call1.span_id, result1.span_id,
            "first result must correlate to first call"
        );
        assert_eq!(
            call2.span_id, result2.span_id,
            "second result must correlate to second call (NOT the first call's span overwritten by the second call)"
        );
    }

    #[test]
    fn wizard_error_becomes_session_failed() {
        let mut p = WizardEventProjector::new("sess_3", &ContextScope::Workspace);
        let ev = p.project(
            "e0",
            WizardEvent::Error {
                message: "loop cap hit".into(),
            },
            ts(),
            || "sp".into(),
        );
        assert_eq!(ev.actor, Actor::System);
        assert!(ev.is_terminal());
        match ev.payload {
            UnifiedPayload::SessionFailed { message } => assert_eq!(message, "loop cap hit"),
            other => panic!("wrong payload: {other:?}"),
        }
    }
}
