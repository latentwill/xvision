//! AgentClient session-method tests against a single-connection mock UDS
//! server. The server handles `runtime.health` (for AgentClient's
//! handshake) plus the three new session methods. Pattern mirrors
//! `transport_mock.rs`.

use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use xvision_agent_client::{BudgetLimits, EndRunParams, StartRunParams, StepParams, UdsTransport};

async fn start_session_mock(socket_path: PathBuf) -> tokio::task::JoinHandle<()> {
    let listener = UnixListener::bind(&socket_path).expect("bind");
    tokio::spawn(async move {
        if let Ok((conn, _)) = listener.accept().await {
            let (r, mut w) = conn.into_split();
            let mut br = BufReader::new(r);
            let mut line = String::new();
            while br.read_line(&mut line).await.unwrap_or(0) > 0 {
                let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = req["id"].clone();
                let method = req["method"].as_str().unwrap_or("");
                let resp = match method {
                    "runtime.health" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocol_version": "0.1.0",
                            "sidecar_version": "0.2.0",
                            "cline_sdk_version": "1.2.3",
                            "status": "ok"
                        }
                    }),
                    "session.start_run" => {
                        let p = &req["params"];
                        assert_eq!(p["run_id"], "r1");
                        assert_eq!(p["provider_id"], "xvision-mock");
                        assert_eq!(p["model_id"], "mock-model");
                        assert_eq!(p["api_key"], "test");
                        assert!(p["base_url"].is_null());
                        assert_eq!(p["system_prompt"], "test");
                        assert_eq!(p["allowed_tools"], json!(["echo"]));
                        assert_eq!(p["budget_limits"]["max_input_tokens"], 1000);
                        assert_eq!(p["budget_limits"]["max_output_tokens"], 1000);
                        assert_eq!(p["budget_limits"]["max_wall_ms"], 30_000);
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": { "run_id": "r1", "started_at_ms": 42 }
                        })
                    }
                    "session.step" => {
                        let p = &req["params"];
                        assert_eq!(p["run_id"], "r1");
                        assert_eq!(p["prompt"], "hi");
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "status": "completed",
                                "output_text": "hello",
                                "iterations": 1,
                                "usage": {
                                    "input_tokens": 10,
                                    "output_tokens": 5,
                                    "cache_read_tokens": 0,
                                    "cache_write_tokens": 0
                                }
                            }
                        })
                    }
                    "session.end_run" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "ended": true }
                    }),
                    _ => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "unknown method" }
                    }),
                };
                let mut out = serde_json::to_vec(&resp).unwrap();
                out.push(b'\n');
                w.write_all(&out).await.unwrap();
                w.flush().await.unwrap();
                line.clear();
            }
        }
    })
}

#[tokio::test]
async fn start_run_round_trip() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_session_mock(sock.clone()).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let res: xvision_agent_client::StartRunResult = t
        .call::<StartRunParams, _>(
            "session.start_run",
            Some(StartRunParams {
                run_id: "r1".into(),
                provider_id: "xvision-mock".into(),
                model_id: "mock-model".into(),
                api_key: Some("test".into()),
                base_url: None,
                system_prompt: "test".into(),
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
            }),
        )
        .await
        .expect("rpc");
    assert_eq!(res.run_id, "r1");
    assert_eq!(res.started_at_ms, 42);
}

#[tokio::test]
async fn step_round_trip() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_session_mock(sock.clone()).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let res: xvision_agent_client::StepResult = t
        .call::<StepParams, _>(
            "session.step",
            Some(StepParams {
                run_id: "r1".into(),
                prompt: "hi".into(),
            }),
        )
        .await
        .expect("rpc");
    assert_eq!(res.status, "completed");
    assert_eq!(res.output_text, "hello");
    assert_eq!(res.iterations, 1);
    assert_eq!(res.usage.input_tokens, 10);
}

#[tokio::test]
async fn end_run_round_trip() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_session_mock(sock.clone()).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let res: xvision_agent_client::EndRunResult = t
        .call::<EndRunParams, _>("session.end_run", Some(EndRunParams { run_id: "r1".into() }))
        .await
        .expect("rpc");
    assert!(res.ended);
}
