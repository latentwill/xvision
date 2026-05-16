use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use xvision_agent_client::{RuntimeHealthResult, UdsTransport};

async fn start_mock_server(socket_path: PathBuf) -> tokio::task::JoinHandle<()> {
    let listener = UnixListener::bind(&socket_path).expect("bind");
    tokio::spawn(async move {
        // Single-connection mock — sufficient through Task 8. Each test
        // creates its own socket so reuse is not required. The JoinHandle
        // is detached; the tokio test runtime cancels it on teardown.
        if let Ok((conn, _)) = listener.accept().await {
            let (r, mut w) = conn.into_split();
            let mut br = BufReader::new(r);
            let mut line = String::new();
            while br.read_line(&mut line).await.unwrap_or(0) > 0 {
                let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = req["id"].clone();
                let method = req["method"].as_str().unwrap_or("");
                let resp = if method == "runtime.health" {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocol_version": "0.1.0",
                            "sidecar_version": "0.1.0",
                            "cline_sdk_version": "unbound",
                            "status": "ok"
                        }
                    })
                } else {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "unknown method" }
                    })
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
async fn calls_runtime_health_against_mock() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_mock_server(sock.clone()).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let h: RuntimeHealthResult = t
        .call::<(), _>("runtime.health", None)
        .await
        .expect("rpc");
    assert_eq!(h.protocol_version, "0.1.0");
    assert_eq!(h.cline_sdk_version, "unbound");
    assert_eq!(h.status, "ok");
}

#[tokio::test]
async fn surfaces_method_not_found_as_rpc_error() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_mock_server(sock.clone()).await;

    let t = UdsTransport::connect(&sock).await.unwrap();
    let err = t
        .call::<(), serde_json::Value>("does.not.exist", None)
        .await
        .expect_err("should fail");
    match err {
        xvision_agent_client::AgentClientError::Rpc { code, .. } => assert_eq!(code, -32601),
        other => panic!("wrong error variant: {other:?}"),
    }
}
