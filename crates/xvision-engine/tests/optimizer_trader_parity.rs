//! Parity invariant tests for the optimizer's trader wiring (Task 1.4).
//!
//! Proves that `Executor::cline_is_wired()` correctly reflects whether the
//! executor carries a live `ClineDispatchCtx` — which is the gate that routes
//! the trader slot through `execute_slot_cline` (matching live) rather than the
//! `LlmDispatch` path.
//!
//! Two assertions:
//!   1. `Executor::new()` → `cline_is_wired() == false` (no context attached).
//!   2. `Executor::new().with_cline_runtime(AgentRuntime::Cline, Some(ctx))`
//!      → `cline_is_wired() == true`.
//!
//! The positive case spawns the same `mock_agentd.js` fixture used by the
//! other cline integration tests (pure Node stdlib, no real sidecar required).

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{AgentRuntime, ProviderEntry, ProviderKind};
use xvision_engine::agent::dispatch_capability::ClineDispatchCtx;
use xvision_engine::eval::executor::backtest::Executor;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

/// Spawn the mock sidecar and return a connected `AgentClient` + the temp dir
/// that holds the socket (must be kept alive for the socket's lifetime).
async fn spawn_mock(cfg: serde_json::Value) -> (AgentClient, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&cfg).unwrap(),
    )
    .expect("write cfg");
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)");
    (client, dir)
}

fn anthropic_entry() -> ProviderEntry {
    ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-sonnet-4-6".into()],
    }
}

// ── Negative case ─────────────────────────────────────────────────────────────

#[test]
fn executor_new_cline_not_wired() {
    // A freshly-constructed Executor has no ClineDispatchCtx.
    // `should_use_cline` in the pipeline dispatch will keep the slot on
    // LlmDispatch — i.e., the default state is the safe/offline state.
    let executor = Executor::new();
    assert!(
        !executor.cline_is_wired(),
        "Executor::new() must not wire Cline by default"
    );
}

// ── Positive case ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn executor_with_cline_runtime_is_wired() {
    // Spawn the mock sidecar (no real agentd binary required — it's a pure
    // Node stdlib script used across all Stage 1 cline integration tests).
    let (client, _dir) = spawn_mock(json!({
        "decisionJson": r#"{"action":"hold","conviction":0.1,"justification":"parity test"}"#
    }))
    .await;

    let ctx = ClineDispatchCtx {
        client: Arc::new(client),
        provider_entry: anthropic_entry(),
        api_key: Some("test-key".into()),
        recording_slot_role: None,
        tool_asset_guard: None,
        as_of_guard: None,
        run_mode: xvision_engine::eval::run::RunMode::Backtest,
    };

    let executor = Executor::new().with_cline_runtime(AgentRuntime::Cline, Some(ctx));

    assert!(
        executor.cline_is_wired(),
        "Executor with a ClineDispatchCtx must report cline_is_wired() == true"
    );
}
