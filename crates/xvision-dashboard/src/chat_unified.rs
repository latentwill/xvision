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

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use xvision_engine::chat_session::ContextScope;
use xvision_observability::types::{RiskLevel, SideEffectLevel, ToolOrigin};
use xvision_observability::{
    Actor, EventScope, EventSource, ToolCallCancelledEvent, ToolCallFinishedEvent, ToolCallStartedEvent,
    UnifiedEvent, UnifiedPayload,
};
#[cfg(test)]
use xvision_observability::{ToolDenied, ToolPolicyChecked, ToolPolicyOutcome};

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
    /// Tool spans opened by a `ToolRequested` that have not yet been closed
    /// by a `ToolFinished`, in the order they were opened. Each entry is keyed
    /// by the model's `tool_use` id, so a `ToolResult` closes EXACTLY the span
    /// its matching `ToolCall` opened — robust to repeats and to
    /// out-of-order/interleaved results.
    ///
    /// History: pre-2026-05-26 this was a `HashMap<tool_name, span_id>`, which
    /// silently overwrote when the same tool was invoked twice in one turn;
    /// the 2026-05-26 fix made it `HashMap<tool_name, VecDeque<span_id>>` (FIFO
    /// by tool name), which is correct ONLY while results arrive in call order.
    /// The reducer keys tool rows on `span_id`, so a mis-correlated
    /// `ToolFinished` leaves the other call's row stuck at "requested" — a
    /// spinner that never resolves. Keying on the `tool_use` id removes the
    /// ordering assumption entirely. Insertion order is preserved (a `Vec`,
    /// not a map) so [`terminalize_open_spans`](Self::terminalize_open_spans)
    /// emits deterministic synthetic closes.
    open_spans: Vec<OpenSpan>,
}

/// A tool span awaiting its `ToolFinished`. `tool_use_id` pairs the closing
/// `ToolResult` to this exact span; `tool_name` lets policy events (which
/// carry only the tool name) correlate to the span their `ToolCall` opened.
struct OpenSpan {
    tool_use_id: String,
    span_id: String,
    tool_name: String,
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
            open_spans: Vec::new(),
        }
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// The span id of the still-open tool span for `tool_name`, if any.
    ///
    /// Policy events (`ToolPolicyChecked` / `ToolDenied`) carry only the tool
    /// name, not the `tool_use` id, and are projected immediately after their
    /// tool's `ToolCall` (and before any later tool's call) — so at most one
    /// open span matches the name at that point. We scan from the most recent
    /// open span so that, in the pathological case of two concurrently-open
    /// spans for the same tool, the policy update attaches to the latest call
    /// (the one just projected) rather than an older still-open one.
    fn open_span_for_tool(&self, tool_name: &str) -> Option<String> {
        self.open_spans
            .iter()
            .rev()
            .find(|s| s.tool_name == tool_name)
            .map(|s| s.span_id.clone())
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
                if let Some(span) = self.open_span_for_tool(&ev.tool_name) {
                    ev.span_id = span.clone();
                    span_id = Some(span);
                }
            }
            UnifiedPayload::ToolDenied(ev) => {
                if let Some(span) = self.open_span_for_tool(&ev.tool_name) {
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
            WizardEvent::ToolCall { id, tool, args } => {
                let span = span_minter();
                self.open_spans.push(OpenSpan {
                    tool_use_id: id,
                    span_id: span.clone(),
                    tool_name: tool.clone(),
                });
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
                        input_text: None,
                    }),
                )
            }
            WizardEvent::ToolResult { id, tool: _, result } => {
                // Close EXACTLY the span the matching `ToolCall` (same
                // `tool_use` id) opened — order-independent. If no open span
                // carries this id (a `ToolResult` with no matching `ToolCall`,
                // or one already closed — shouldn't happen in practice, but
                // defensive), mint a fresh span so the unified stream is still
                // well-formed.
                let span = match self.open_spans.iter().position(|s| s.tool_use_id == id) {
                    Some(pos) => self.open_spans.remove(pos).span_id,
                    None => span_minter(),
                };
                let output_hash = sha256_hex(result.to_string().as_bytes());
                (
                    Actor::Agent,
                    Some(span.clone()),
                    UnifiedPayload::ToolFinished(ToolCallFinishedEvent {
                        span_id: span,
                        output_hash: Some(output_hash),
                        output_payload_ref: None,
                        exit_code: None,
                        output_text: None,
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

    /// Force-close every still-open tool span, returning a `ToolCancelled`
    /// [`UnifiedEvent`] for each (in the order the spans were opened) and
    /// clearing the open set.
    ///
    /// Call this right BEFORE projecting a terminal `WizardEvent`
    /// (`Done` → `AssistantMessageDone`, `Error` → `SessionFailed`): once the
    /// turn is over, any span that never received its `ToolResult` will never
    /// get one, so without an explicit close its tool row spins forever in the
    /// span-keyed reducer. Emitting a real, persisted `ToolCancelled` (rather
    /// than relying on a client-side heuristic) guarantees the durable event
    /// log itself shows the span closed, and works for every consumer of the
    /// unified stream. `ToolCancelled` — not `ToolFinished` — because we have
    /// no result: the honest status is "the turn ended before this tool
    /// reported back", not "succeeded".
    ///
    /// Each event gets a fresh `event_id` (from `event_id_minter`) and the
    /// next monotonic `seq`, and is attributed to [`Actor::System`] since the
    /// close is the harness's doing, not the model's.
    pub fn terminalize_open_spans(
        &mut self,
        mut event_id_minter: impl FnMut() -> String,
        ts: DateTime<Utc>,
    ) -> Vec<UnifiedEvent> {
        let open = std::mem::take(&mut self.open_spans);
        open.into_iter()
            .map(|s| {
                let out = UnifiedEvent {
                    event_id: event_id_minter(),
                    session_id: Some(self.session_id.clone()),
                    run_id: None,
                    span_id: Some(s.span_id.clone()),
                    parent_event_id: None,
                    seq: self.seq,
                    ts,
                    scope: self.scope.clone(),
                    actor: Actor::System,
                    source: EventSource::ChatRail,
                    blob_hash: None,
                    payload: UnifiedPayload::ToolCancelled(ToolCallCancelledEvent {
                        span_id: s.span_id,
                        reason: Some(
                            "turn ended before the tool reported a result \
                             (auto-closed to prevent a stuck tool row)"
                                .to_string(),
                        ),
                    }),
                };
                self.seq += 1;
                out
            })
            .collect()
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
                id: "tu_0".into(),
                tool: "create_strategy".into(),
                args: json!({"name": "x"}),
            },
            ts(),
            &mut mint,
        );
        let result = p.project(
            "e1",
            WizardEvent::ToolResult {
                id: "tu_0".into(),
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
                id: "tu_policy".into(),
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
                id: "tu_policy".into(),
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
                id: "tu_a".into(),
                tool: "list_strategies".into(),
                args: json!({"page": 1}),
            },
            ts(),
            &mut mint,
        );
        let call2 = p.project(
            "ec2",
            WizardEvent::ToolCall {
                id: "tu_b".into(),
                tool: "list_strategies".into(),
                args: json!({"page": 2}),
            },
            ts(),
            &mut mint,
        );
        let result1 = p.project(
            "er1",
            WizardEvent::ToolResult {
                id: "tu_a".into(),
                tool: "list_strategies".into(),
                result: json!([]),
            },
            ts(),
            &mut mint,
        );
        let result2 = p.project(
            "er2",
            WizardEvent::ToolResult {
                id: "tu_b".into(),
                tool: "list_strategies".into(),
                result: json!([]),
            },
            ts(),
            &mut mint,
        );

        // Each call gets a distinct span and each result correlates
        // to the matching call.
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

    /// The core of the id-keying fix: when two calls to the SAME tool have
    /// their results delivered OUT OF ORDER (Result B before Result A), each
    /// `ToolFinished` must still correlate to the span its own `ToolCall`
    /// opened — matched on the `tool_use` id, not the tool name. Under the
    /// previous name-FIFO keying, Result B (arriving first) would pop the
    /// FRONT of the name queue (call A's span), mis-correlating both results
    /// and stranding call A's row at "requested" in the span-keyed reducer.
    #[test]
    fn interleaved_results_correlate_to_their_call_by_id() {
        let mut p = WizardEventProjector::new("sess_interleave", &ContextScope::Workspace);
        let mut minted: u32 = 0;
        let mut mint = || {
            minted += 1;
            format!("sp_{minted}")
        };

        let call_a = p.project(
            "eca",
            WizardEvent::ToolCall {
                id: "tu_a".into(),
                tool: "list_strategies".into(),
                args: json!({"page": 1}),
            },
            ts(),
            &mut mint,
        );
        let call_b = p.project(
            "ecb",
            WizardEvent::ToolCall {
                id: "tu_b".into(),
                tool: "list_strategies".into(),
                args: json!({"page": 2}),
            },
            ts(),
            &mut mint,
        );
        // Results arrive in REVERSE order: B's result before A's.
        let result_b = p.project(
            "erb",
            WizardEvent::ToolResult {
                id: "tu_b".into(),
                tool: "list_strategies".into(),
                result: json!(["b"]),
            },
            ts(),
            &mut mint,
        );
        let result_a = p.project(
            "era",
            WizardEvent::ToolResult {
                id: "tu_a".into(),
                tool: "list_strategies".into(),
                result: json!(["a"]),
            },
            ts(),
            &mut mint,
        );

        assert_ne!(call_a.span_id, call_b.span_id);
        assert_eq!(
            call_b.span_id, result_b.span_id,
            "B's result must correlate to B's call even though it arrived first"
        );
        assert_eq!(
            call_a.span_id, result_a.span_id,
            "A's result must correlate to A's call (NOT the front-of-queue span)"
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

    // ── terminalize_open_spans (turn-end anti-strand) ──

    fn call(
        p: &mut WizardEventProjector,
        ev_id: &str,
        id: &str,
        tool: &str,
        mint: &mut impl FnMut() -> String,
    ) -> UnifiedEvent {
        p.project(
            ev_id,
            WizardEvent::ToolCall {
                id: id.into(),
                tool: tool.into(),
                args: json!({}),
            },
            ts(),
            mint,
        )
    }

    fn result(
        p: &mut WizardEventProjector,
        ev_id: &str,
        id: &str,
        tool: &str,
        mint: &mut impl FnMut() -> String,
    ) {
        p.project(
            ev_id,
            WizardEvent::ToolResult {
                id: id.into(),
                tool: tool.into(),
                result: json!({"ok": true}),
            },
            ts(),
            mint,
        );
    }

    /// A still-open span (its `ToolResult` never arrived) is force-closed with
    /// a `ToolCancelled` carrying the call's span id and an explanatory reason.
    /// A span that DID receive its result is not re-closed.
    #[test]
    fn terminalize_open_spans_closes_only_unfinished_spans() {
        let mut p = WizardEventProjector::new("sess_term", &ContextScope::Workspace);
        let mut minted = 0;
        let mut mint = || {
            minted += 1;
            format!("sp_{minted}")
        };

        let call_a = call(&mut p, "eca", "tu_a", "list_strategies", &mut mint);
        let call_b = call(&mut p, "ecb", "tu_b", "list_scenarios", &mut mint);
        // A finishes normally; B never does.
        result(&mut p, "era", "tu_a", "list_strategies", &mut mint);

        let synth = p.terminalize_open_spans(
            {
                let mut n = 0;
                move || {
                    n += 1;
                    format!("synth_{n}")
                }
            },
            ts(),
        );

        assert_eq!(synth.len(), 1, "only the unfinished span B should be closed");
        let ev = &synth[0];
        assert_eq!(ev.span_id.as_deref(), Some(call_b.span_id.as_deref().unwrap()));
        match &ev.payload {
            UnifiedPayload::ToolCancelled(c) => {
                assert_eq!(c.span_id, call_b.span_id.clone().unwrap());
                assert!(
                    c.reason
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains("turn"),
                    "reason should explain the turn ended without a result, got {:?}",
                    c.reason
                );
            }
            other => panic!("expected ToolCancelled, got {other:?}"),
        }
        // A's span must NOT appear among the synthetic closes.
        assert!(
            synth.iter().all(|e| e.span_id != call_a.span_id),
            "the already-finished span must not be force-closed"
        );

        // Calling again is a no-op — the open set was cleared.
        let again = p.terminalize_open_spans(|| "synth_x".into(), ts());
        assert!(again.is_empty(), "open spans should be cleared after terminalize");
    }

    /// With no open spans, terminalize emits nothing (a clean turn must not
    /// fabricate cancel events).
    #[test]
    fn terminalize_open_spans_is_empty_for_a_clean_turn() {
        let mut p = WizardEventProjector::new("sess_clean", &ContextScope::Workspace);
        let mut minted = 0;
        let mut mint = || {
            minted += 1;
            format!("sp_{minted}")
        };
        call(&mut p, "eca", "tu_a", "list_strategies", &mut mint);
        result(&mut p, "era", "tu_a", "list_strategies", &mut mint);

        let synth = p.terminalize_open_spans(|| "synth".into(), ts());
        assert!(synth.is_empty());
    }

    /// Multiple open spans are closed in the order they were opened, each with
    /// the next monotonic `seq` so the unified stream stays gap-free, and the
    /// synthetic close is attributed to the system (not the agent).
    #[test]
    fn terminalize_open_spans_emits_in_open_order_with_monotonic_seq() {
        let mut p = WizardEventProjector::new("sess_multi", &ContextScope::Workspace);
        let mut minted = 0;
        let mut mint = || {
            minted += 1;
            format!("sp_{minted}")
        };
        let a = call(&mut p, "eca", "tu_a", "t1", &mut mint);
        let b = call(&mut p, "ecb", "tu_b", "t2", &mut mint);
        let c = call(&mut p, "ecc", "tu_c", "t3", &mut mint);
        let seq_before = p.seq();

        let mut n = 0;
        let synth = p.terminalize_open_spans(
            move || {
                n += 1;
                format!("synth_{n}")
            },
            ts(),
        );

        let span_order: Vec<_> = synth.iter().map(|e| e.span_id.clone()).collect();
        assert_eq!(
            span_order,
            vec![a.span_id.clone(), b.span_id.clone(), c.span_id.clone()],
            "synthetic closes must follow open order"
        );
        let seqs: Vec<_> = synth.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![seq_before, seq_before + 1, seq_before + 2]);
        assert_eq!(
            p.seq(),
            seq_before + 3,
            "seq must advance past the synthetic closes"
        );
        for e in &synth {
            assert_eq!(e.actor, Actor::System);
            assert_eq!(e.session_id.as_deref(), Some("sess_multi"));
            assert!(matches!(e.payload, UnifiedPayload::ToolCancelled(_)));
        }
    }
}
