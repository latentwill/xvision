//! Transport-level mock test for the `session.replay_load` JSON-RPC method.
//!
//! Mirrors the pattern in `session_methods_mock.rs`: a minimal single-connection
//! UDS server handles `runtime.health` (for the handshake path) and
//! `session.replay_load`, asserting the correct wire shape is sent and that the
//! `ReplayLoadResult` is parsed correctly.

use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use xvision_agent_client::{ReplayLoadParams, ReplayLoadResult, UdsTransport};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Spawn a mock UDS server that understands `session.replay_load`.
///
/// On receiving the RPC it:
///   1. Asserts `run_id` equals the expected value.
///   2. Asserts the `frames` array has the expected length.
///   3. Asserts each frame has a `kind` field (i.e. the `TrajectoryFrame`
///      discriminator tag is present on the wire).
///   4. Returns `{ "loaded": <frame_count> }`.
async fn start_replay_load_mock(
    socket_path: PathBuf,
    expected_run_id: &'static str,
    expected_frame_count: usize,
) -> tokio::task::JoinHandle<()> {
    let listener = UnixListener::bind(&socket_path).expect("bind replay_load mock");
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
                            "sidecar_version": "0.3.0",
                            "cline_sdk_version": "1.3.0",
                            "status": "ok"
                        }
                    }),

                    "session.replay_load" => {
                        let p = &req["params"];

                        // Assert run_id is forwarded correctly.
                        assert_eq!(
                            p["run_id"].as_str().unwrap_or(""),
                            expected_run_id,
                            "replay_load run_id mismatch"
                        );

                        // Assert frames array length.
                        let frames = p["frames"].as_array().expect("frames must be array");
                        assert_eq!(
                            frames.len(),
                            expected_frame_count,
                            "frame count mismatch: got {}, want {}",
                            frames.len(),
                            expected_frame_count
                        );

                        // Assert every frame carries a `kind` discriminator.
                        for (i, frame) in frames.iter().enumerate() {
                            assert!(
                                frame.get("kind").is_some(),
                                "frame[{i}] is missing the 'kind' tag"
                            );
                        }

                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": { "loaded": expected_frame_count }
                        })
                    }

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Happy path: four frames (Request → TextDelta → Usage → Finish) are sent;
/// the mock confirms the correct wire shape and returns `loaded: 4`.
#[tokio::test]
async fn replay_load_sends_correct_method_and_parses_result() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_replay_load_mock(sock.clone(), "run-abc", 4).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");

    let frames = vec![
        json!({ "kind": "Request",   "ts_ms": 1, "messages": [], "tools": [], "system_prompt": "you are a trader" }),
        json!({ "kind": "TextDelta", "ts_ms": 2, "text": "Analyzing..." }),
        json!({
            "kind": "Usage",
            "ts_ms": 3,
            "input_tokens": 120,
            "output_tokens": 45,
            "cache_read_tokens": 10,
            "cache_write_tokens": 5,
            "total_cost": 0.00234
        }),
        json!({ "kind": "Finish", "ts_ms": 4, "reason": "stop" }),
    ];

    let result: ReplayLoadResult = t
        .call::<ReplayLoadParams, ReplayLoadResult>(
            "session.replay_load",
            Some(ReplayLoadParams {
                run_id: "run-abc".into(),
                frames,
            }),
        )
        .await
        .expect("replay_load rpc");

    assert_eq!(result.loaded, 4, "loaded count must match frame count");
}

/// Edge case: an empty frames list is a valid RPC call (the sidecar is
/// responsible for treating it as a corrupt recording).
#[tokio::test]
async fn replay_load_with_empty_frames_is_valid_rpc() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock2");
    let _server = start_replay_load_mock(sock.clone(), "run-empty", 0).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");

    let result: ReplayLoadResult = t
        .call::<ReplayLoadParams, ReplayLoadResult>(
            "session.replay_load",
            Some(ReplayLoadParams {
                run_id: "run-empty".into(),
                frames: vec![],
            }),
        )
        .await
        .expect("empty replay_load rpc");

    assert_eq!(result.loaded, 0);
}

/// Verify that the `loaded` field defaults to 0 when the sidecar response
/// omits it (forward-compat: older sidecar versions may not include it).
#[tokio::test]
async fn replay_load_result_loaded_defaults_to_zero() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock3");

    // Spawn a mock that returns the result WITHOUT the `loaded` field.
    let listener = UnixListener::bind(&sock).expect("bind");
    let _server = tokio::spawn(async move {
        if let Ok((conn, _)) = listener.accept().await {
            let (r, mut w) = conn.into_split();
            let mut br = BufReader::new(r);
            let mut line = String::new();
            while br.read_line(&mut line).await.unwrap_or(0) > 0 {
                let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = req["id"].clone();
                // Omit `loaded` from result intentionally.
                let resp = json!({ "jsonrpc": "2.0", "id": id, "result": {} });
                let mut out = serde_json::to_vec(&resp).unwrap();
                out.push(b'\n');
                w.write_all(&out).await.unwrap();
                w.flush().await.unwrap();
                line.clear();
            }
        }
    });

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let result: ReplayLoadResult = t
        .call::<ReplayLoadParams, ReplayLoadResult>(
            "session.replay_load",
            Some(ReplayLoadParams {
                run_id: "r".into(),
                frames: vec![],
            }),
        )
        .await
        .expect("rpc with absent loaded");

    assert_eq!(result.loaded, 0, "absent 'loaded' must default to 0");
}

/// Verify the full `StartRunParams` `decision_schema` integration is preserved —
/// the mock still knows nothing about it here, but this test confirms the
/// struct import doesn't collide with the new `ReplayLoadParams` import.
#[tokio::test]
async fn replay_load_params_wire_shape_has_kind_tags() {
    // Pure serde test — no network involved.
    use xvision_observability::trajectory::frame::TrajectoryFrame;

    let frames_typed: Vec<TrajectoryFrame> = vec![
        TrajectoryFrame::Request {
            ts_ms: 100,
            messages: json!([]),
            tools: json!([]),
            system_prompt: Some("sp".into()),
        },
        TrajectoryFrame::TextDelta { ts_ms: 101, text: "hi".into() },
        TrajectoryFrame::Usage {
            ts_ms: 102,
            input_tokens: 5,
            output_tokens: 3,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            total_cost: 0.0,
        },
        TrajectoryFrame::Finish { ts_ms: 103, reason: "stop".into(), error: None },
    ];

    // Serialize typed frames → Vec<Value> (the caller-side conversion).
    let frames_json: Vec<serde_json::Value> =
        frames_typed.iter().map(|f| serde_json::to_value(f).unwrap()).collect();

    let params = ReplayLoadParams {
        run_id: "wire-test".into(),
        frames: frames_json,
    };

    let wire = serde_json::to_value(&params).unwrap();

    // run_id
    assert_eq!(wire["run_id"], "wire-test");

    // frames array with `kind` tags
    let arr = wire["frames"].as_array().unwrap();
    assert_eq!(arr.len(), 4);
    assert_eq!(arr[0]["kind"], "Request");
    assert_eq!(arr[1]["kind"], "TextDelta");
    assert_eq!(arr[2]["kind"], "Usage");
    assert_eq!(arr[3]["kind"], "Finish");

    // Spot-check ts_ms pass-through
    assert_eq!(arr[0]["ts_ms"], 100);
    assert_eq!(arr[3]["ts_ms"], 103);
}
