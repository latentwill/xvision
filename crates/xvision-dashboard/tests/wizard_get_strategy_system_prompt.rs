//! W11 — `get_strategy` chat tool must expose each agent's `system_prompt`.
//!
//! Finding #13: the `get_strategy` chat tool returned the raw `Strategy`
//! struct, which contains `agents: Vec<AgentRef>`. Each `AgentRef` only
//! carries `agent_id` and `role` — the `system_prompt` lives on the resolved
//! `Agent.slots[].system_prompt` in the agent library (SQLite). Because the
//! handler never resolved the `AgentRef` pointers to their `Agent` records,
//! the authoring agent that wrote a strategy could not read back the prompt it
//! had just authored. This is the root of finding #14 (BTC/USD vs ETH/USD
//! context bleed): the mismatch was invisible.
//!
//! These tests pin the corrected behaviour:
//!
//! 1. **Agent with system_prompt attached** — after attaching an agent with a
//!    known system_prompt to a strategy, calling `get_strategy` must include
//!    a `resolved_agents` array whose entries carry `role` and `system_prompt`
//!    (the prompt text), so the authoring agent can verify what it wrote.
//!
//! 2. **Strategy with no attached agents** — `get_strategy` on a blank draft
//!    (no `AgentRef`s) must return an empty (or absent) `resolved_agents` and
//!    must not error.
//!
//! 3. **Graceful degradation** — if a referenced agent has been deleted from
//!    the library since the strategy was authored, the tool must NOT return an
//!    error for the whole call; it may omit that entry or include a note, but
//!    the call must succeed.

use std::sync::Arc;

use tempfile::TempDir;
use xvision_dashboard::wizard_loop::{WizardEvent, WizardLoop};
use xvision_dashboard::AppState;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::agents::model::AgentSlot;
use xvision_engine::agents::store::AgentStore;
use xvision_engine::api::agents::{self as api_agents, CreateAgentRequest};
use xvision_engine::api::strategy::{self as api_strategy, AddAgentReq};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::authoring::CreateStrategyReq;
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};

// ── helpers ──────────────────────────────────────────────────────────────────

async fn boot() -> (AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init AppState");
    (state, tmp)
}

/// Drain all events from the wizard loop and return them.
async fn drain(wl: &mut WizardLoop) -> Vec<WizardEvent> {
    let mut out = vec![];
    while let Some(ev) = wl.next_event().await {
        out.push(ev);
    }
    out
}

/// Extract the first `ToolResult` event for the given tool name.
fn first_tool_result<'a>(events: &'a [WizardEvent], tool: &str) -> Option<&'a serde_json::Value> {
    events.iter().find_map(|ev| match ev {
        WizardEvent::ToolResult { tool: t, result, .. } if t == tool => Some(result),
        _ => None,
    })
}

/// Make an `ApiContext` from an `AppState` (replicates what `WizardLoop` does
/// internally so we can seed the DB before the wizard runs).
fn api_ctx(state: &AppState, tmp: &TempDir) -> ApiContext {
    ApiContext::new(
        state.pool.clone(),
        Actor::Cli { user: "test".into() },
        tmp.path().to_path_buf(),
    )
}

// ── test 1: system_prompt is present in get_strategy result ──────────────────

/// After attaching an agent with a known system_prompt to a strategy,
/// `get_strategy` must return a `resolved_agents` array that includes an
/// entry for the attached agent with `role` and `system_prompt` fields so
/// the authoring agent can verify what it wrote.
#[tokio::test]
async fn get_strategy_includes_system_prompt_per_agent() {
    let (state, tmp) = boot().await;
    let ctx = api_ctx(&state, &tmp);

    // 1. Create a strategy via the api_strategy layer (which uses ApiContext
    //    and creates the FilesystemStore internally).
    let created = api_strategy::create_strategy(
        &ctx,
        CreateStrategyReq {
            name: "eth-swing-test".into(),
            creator: Some("@tester".into()),
        },
    )
    .await
    .expect("create strategy");
    let strategy_id = created.id.clone();

    // 2. Create an agent with a distinctive system_prompt.
    let known_prompt = "You are an ETH/USD 4-hour swing trader for this strategy. Review the latest OHLCV bars, configured filters, risk controls, and current position state before making a decision. Return structured JSON with action, size_pct, confidence, and concise rationale. Trade ETH only, respect stop loss and take profit limits, and hold when evidence is weak.";
    let agent = api_agents::create(
        &ctx,
        CreateAgentRequest {
            name: "eth-trader-agent".into(),
            description: "ETH swing trader for test".into(),
            tags: vec![],
            slots: vec![AgentSlot {
                name: "trader".into(),
                provider: "anthropic".into(),
                model: "claude-sonnet-4-6".into(),
                system_prompt: known_prompt.into(),
                skill_ids: vec![],
                max_tokens: None,
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: Default::default(),
                bar_history_limit: None,
                memory_mode: Default::default(),
                noop_skip: None,
                allowed_tools: vec![],
                delta_briefing: None,
            }],
            scope_strategy_id: None,
        },
    )
    .await
    .expect("create agent");
    let agent_id = agent.agent_id.clone();

    // 3. Attach the agent to the strategy with role "trader".
    api_strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_id.clone(),
            agent_id: agent_id.clone(),
            role: "trader".into(),
            activates: None,
        },
    )
    .await
    .expect("add agent to strategy");

    // 4. Script the WizardLoop: call get_strategy, then emit a done text.
    let mock: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use("tu_1", "get_strategy", serde_json::json!({ "id": strategy_id })),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: "The strategy's trader prompt says: ETH/USD 4-hour swing trader.".into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 20,
        },
    ]));

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "show me the strategy I just created".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let result = first_tool_result(&events, "get_strategy").expect("get_strategy ToolResult must be present");

    // The result must be a well-formed object with the strategy manifest.
    assert!(
        result.get("manifest").is_some() || result.get("id").is_some(),
        "get_strategy must return a strategy object; got: {result}"
    );

    // Must have a `resolved_agents` array.
    let resolved = result
        .get("resolved_agents")
        .unwrap_or_else(|| panic!("get_strategy must include 'resolved_agents' key; got: {result}"));
    let resolved_arr = resolved.as_array().expect("resolved_agents must be a JSON array");

    assert_eq!(
        resolved_arr.len(),
        1,
        "one agent attached → one entry in resolved_agents; got: {resolved_arr:?}"
    );

    let entry = &resolved_arr[0];

    // Each entry must carry the role.
    assert_eq!(
        entry["role"].as_str().unwrap_or(""),
        "trader",
        "resolved_agents[0].role must be 'trader'; got: {entry}"
    );

    // The system_prompt must be present and match what we authored.
    let got_prompt = entry["system_prompt"]
        .as_str()
        .unwrap_or_else(|| panic!("resolved_agents[0].system_prompt must be a string; got: {entry}"));
    assert_eq!(
        got_prompt, known_prompt,
        "system_prompt must match the authored text; got: {got_prompt:?}"
    );
}

// ── test 2: blank strategy returns empty resolved_agents without error ────────

/// A strategy with no attached agents must return `resolved_agents: []` (or
/// the key absent) and must NOT produce an error or panic.
#[tokio::test]
async fn get_strategy_blank_returns_empty_resolved_agents() {
    let (state, tmp) = boot().await;
    let ctx = api_ctx(&state, &tmp);

    // Create a blank strategy (no agents attached).
    let created = api_strategy::create_strategy(
        &ctx,
        CreateStrategyReq {
            name: "blank-test".into(),
            creator: None,
        },
    )
    .await
    .expect("create blank strategy");
    let strategy_id = created.id.clone();

    let mock: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use("tu_1", "get_strategy", serde_json::json!({ "id": strategy_id })),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: "This strategy has no agents yet.".into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 20,
        },
    ]));

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "show me the blank strategy".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let result = first_tool_result(&events, "get_strategy").expect("get_strategy ToolResult must be present");

    // Must be a valid strategy object.
    assert!(
        result.get("manifest").is_some() || result.get("id").is_some(),
        "get_strategy on blank strategy must return a strategy object; got: {result}"
    );

    // resolved_agents must be empty (or absent), never an error.
    assert!(
        result.get("error").is_none(),
        "blank strategy must not produce an error key; got: {result}"
    );
    if let Some(resolved) = result.get("resolved_agents") {
        let arr = resolved
            .as_array()
            .expect("resolved_agents must be an array even when empty");
        assert!(
            arr.is_empty(),
            "blank strategy must have empty resolved_agents; got: {arr:?}"
        );
    }
    // key absent is also acceptable for blank drafts
}

// ── test 3: deleted/unresolvable agent ref degrades gracefully (no error) ─────

/// If a strategy references an agent that has since been deleted from the
/// library, get_strategy must NOT error the whole tool — the unresolvable
/// entry carries `system_prompt: null` plus an `error` note, and the call
/// still returns the strategy. Pins the graceful-degradation path
/// (wizard_loop.rs get_strategy Err branch).
#[tokio::test]
async fn get_strategy_unresolvable_agent_degrades_gracefully() {
    let (state, tmp) = boot().await;
    let ctx = api_ctx(&state, &tmp);

    let created = api_strategy::create_strategy(
        &ctx,
        CreateStrategyReq {
            name: "orphan-ref-test".into(),
            creator: Some("@tester".into()),
        },
    )
    .await
    .expect("create strategy");
    let strategy_id = created.id.clone();

    let agent = api_agents::create(
        &ctx,
        CreateAgentRequest {
            name: "doomed-agent".into(),
            description: "will be deleted".into(),
            tags: vec![],
            slots: vec![AgentSlot {
                name: "trader".into(),
                provider: "anthropic".into(),
                model: "claude-sonnet-4-6".into(),
                system_prompt: "You are the temporary ETH/USD validation trader for this strategy fixture. Evaluate bars, filters, current position state, and risk settings before deciding. Return structured JSON with action, size_pct, confidence, and concise rationale. Respect configured drawdown, stop loss, take profit, and liquidity limits; hold when evidence is weak."
                    .into(),
                skill_ids: vec![],
                max_tokens: None,
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: Default::default(),
                bar_history_limit: None,
                memory_mode: Default::default(),
                noop_skip: None,
                allowed_tools: vec![],
                delta_briefing: None,
            }],
            scope_strategy_id: None,
        },
    )
    .await
    .expect("create agent");
    let agent_id = agent.agent_id.clone();

    api_strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_id.clone(),
            agent_id: agent_id.clone(),
            role: "trader".into(),
            activates: None,
        },
    )
    .await
    .expect("add agent to strategy");

    // Orphan the strategy's AgentRef by removing the agent row at the STORE
    // level (`delete_by_id` bypasses the api in-use guard, which otherwise
    // blocks deleting an attached agent). This simulates the real cross-store
    // inconsistency the graceful-degradation branch defends against: the
    // strategy (FilesystemStore JSON) references an agent_id that is no longer
    // present in the SQLite agent library (e.g. after a DB reset/restore).
    let removed = AgentStore::new(ctx.db.clone())
        .delete_by_id(&agent_id)
        .await
        .expect("store-level delete_by_id");
    assert!(removed, "agent row should have been removed");

    let mock: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use("tu_1", "get_strategy", serde_json::json!({ "id": strategy_id })),
        LlmResponse {
            content: vec![ContentBlock::Text { text: "ok".into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 20,
        },
    ]));

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "show me the strategy".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let result = first_tool_result(&events, "get_strategy").expect("get_strategy ToolResult must be present");

    // The tool must NOT fail wholesale — it still returns the strategy object.
    assert!(
        result.get("manifest").is_some() || result.get("id").is_some(),
        "get_strategy must still return the strategy even with a dangling agent ref; got: {result}"
    );
    assert!(
        result.get("error").is_none(),
        "a dangling agent ref must NOT surface as a top-level error; got: {result}"
    );

    let resolved = result
        .get("resolved_agents")
        .and_then(|v| v.as_array())
        .expect("resolved_agents array must be present");
    assert_eq!(
        resolved.len(),
        1,
        "one (unresolvable) ref → one entry; got: {resolved:?}"
    );
    let entry = &resolved[0];
    assert_eq!(entry["role"].as_str().unwrap_or(""), "trader");
    assert!(
        entry["system_prompt"].is_null(),
        "unresolvable agent must carry system_prompt: null; got: {entry}"
    );
    assert!(
        entry.get("error").is_some(),
        "unresolvable agent entry must carry an error note; got: {entry}"
    );
}
