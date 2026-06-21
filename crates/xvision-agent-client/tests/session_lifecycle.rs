//! End-to-end Wave 2 integration test.
//!
//! Spawns the real sidecar with the test mock model script installed,
//! registers a tool, starts a run, calls step (which causes the
//! sidecar's Agent to call the registered tool via the callback socket),
//! verifies the round-trip, and ends the run.
//!
//! Gated by `XVISION_RUN_SIDECAR_TESTS=1` to keep CI from spawning Node
//! by default. Build the sidecar first:
//!     pnpm --dir xvision-agentd build

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

use xvision_agent_client::{
    AgentClient, BudgetLimits, EndRunParams, SideEffectLevel, StartRunParams, StepParams, ToolDescriptor,
    ToolDispatch, ToolDispatchError,
};

#[derive(Default)]
struct EchoDispatch {
    invocations: Mutex<Vec<(String, serde_json::Value)>>,
}

#[async_trait]
impl ToolDispatch for EchoDispatch {
    async fn invoke(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, ToolDispatchError> {
        self.invocations
            .lock()
            .expect("record echo invocation")
            .push((name.to_string(), input.clone()));

        if name != "echo" {
            return Err(ToolDispatchError::UnknownTool(name.into()));
        }
        let msg = input
            .get("msg")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolDispatchError::Failed("echo input missing string msg".into()))?;
        Ok(json!({ "echoed": msg }))
    }
}

fn agentd_bin() -> PathBuf {
    std::env::var("XVISION_AGENTD_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("xvision-agentd/dist/index.js")
        })
}

#[tokio::test]
async fn full_session_round_trip() {
    if std::env::var("XVISION_RUN_SIDECAR_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping: XVISION_RUN_SIDECAR_TESTS != 1");
        return;
    }

    let sidecar_path = agentd_bin();
    assert!(
        sidecar_path.exists(),
        "sidecar not built at {:?}. Run `pnpm --dir xvision-agentd build` first or set XVISION_AGENTD_PATH.",
        sidecar_path
    );

    // Mock script lives in the sidecar; gate via env var before spawn.
    // Supervisor::spawn inherits the parent process env, so this set
    // propagates into the spawned node process.
    std::env::set_var("XVISION_TEST_MOCK_PROVIDER", "1");

    let dir = TempDir::new().expect("tempdir");
    let socket_path = dir.path().join("xvision-agentd.sock");
    let callback_path = dir.path().join("xvision-callbacks.sock");

    let dispatch = Arc::new(EchoDispatch::default());
    let client =
        AgentClient::spawn_with_callbacks(&sidecar_path, &socket_path, &callback_path, dispatch.clone())
            .await
            .expect("spawn sidecar");

    // Step 1: register the echo tool via the Wave-1 register_tools path.
    client
        .register_tools(vec![ToolDescriptor {
            name: "echo".into(),
            version: "1.0.0".into(),
            description: "echoes its input back".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "msg": { "type": "string" } },
                "required": ["msg"]
            }),
            output_schema: json!({ "type": "object" }),
            timeout_ms: 5000,
            side_effect_level: SideEffectLevel::Pure,
            requires_approval: false,
        }])
        .await
        .expect("register_tools");

    // Step 2: start_run.
    let started = client
        .start_run(StartRunParams {
            run_id: "wave2-it-1".into(),
            provider_id: "xvision-mock".into(),
            model_id: "mock-model".into(),
            api_key: Some("test".into()),
            base_url: None,
            system_prompt: "you are a test agent".into(),
            allowed_tools: vec!["echo".into()],
            budget_limits: BudgetLimits {
                max_input_tokens: 1000,
                max_output_tokens: 1000,
                max_wall_ms: 30_000,
            },
            decision_schema: None,
            record: false,
            slot_role: None,
            reasoning_effort: None,
        })
        .await
        .expect("start_run");
    assert_eq!(started.run_id, "wave2-it-1");

    // Step 3: step. Mock script (set in xvision-agentd/src/index.ts when
    // XVISION_TEST_MOCK_PROVIDER=1): echo tool call then "done".
    let stepped = timeout(
        Duration::from_secs(20),
        client.step(StepParams {
            run_id: "wave2-it-1".into(),
            prompt: "go".into(),
        }),
    )
    .await
    .expect("step timed out")
    .expect("step");

    assert_eq!(stepped.status, "completed");
    assert!(
        stepped.output_text.contains("done"),
        "expected output_text to contain 'done', got: {:?}",
        stepped.output_text
    );
    let invocations = dispatch
        .invocations
        .lock()
        .expect("read echo invocations")
        .clone();
    assert_eq!(
        invocations,
        vec![("echo".to_string(), json!({ "msg": "from-sidecar" }))],
        "expected one echo callback with the sidecar payload"
    );

    // Step 4: end_run.
    let ended = client
        .end_run(EndRunParams {
            run_id: "wave2-it-1".into(),
        })
        .await
        .expect("end_run");
    assert!(ended.ended);

    client.shutdown().await.expect("shutdown");
}
