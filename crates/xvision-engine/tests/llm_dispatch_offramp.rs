//! Stage 3 Task 10 / inheritance item 6 — Cline is the unconditional routine
//! runtime, with an emergency env-gated rollback to legacy `LlmDispatch`.
//!
//! `resolve_agent_runtime` (the eval entry point's resolver) is private, so
//! this test exercises the public contract it delegates to:
//! `config::resolve_routine_runtime` + `config::emergency_llm_dispatch_enabled`.
//! The eval resolver checks `emergency_llm_dispatch_enabled()` first and
//! returns `LlmDispatch` with a loud `warn!` when set — proven here at the
//! routine-resolver level.
//!
//! The env var is process-global, so the set/unset sequence lives in one test
//! to avoid cross-test races.

use xvision_core::config::{
    emergency_llm_dispatch_enabled, resolve_routine_runtime, AgentRuntime,
    EMERGENCY_LLM_DISPATCH_ENV,
};

#[test]
fn cline_is_routine_runtime_with_env_gated_llm_dispatch_offramp() {
    // Routine path with no override → Cline, unconditionally.
    std::env::remove_var(EMERGENCY_LLM_DISPATCH_ENV);
    assert!(!emergency_llm_dispatch_enabled());
    assert_eq!(
        resolve_routine_runtime(),
        AgentRuntime::Cline,
        "Cline must be the unconditional routine runtime"
    );

    // Emergency rollback opt-in → LlmDispatch (incident lever, item 6).
    std::env::set_var(EMERGENCY_LLM_DISPATCH_ENV, "1");
    assert!(emergency_llm_dispatch_enabled());
    assert_eq!(
        resolve_routine_runtime(),
        AgentRuntime::LlmDispatch,
        "the emergency env var must route back to LlmDispatch"
    );

    // `true` is also honored; non-truthy values are NOT a rollback.
    std::env::set_var(EMERGENCY_LLM_DISPATCH_ENV, "true");
    assert_eq!(resolve_routine_runtime(), AgentRuntime::LlmDispatch);
    std::env::set_var(EMERGENCY_LLM_DISPATCH_ENV, "0");
    assert_eq!(resolve_routine_runtime(), AgentRuntime::Cline);

    std::env::remove_var(EMERGENCY_LLM_DISPATCH_ENV);
}
