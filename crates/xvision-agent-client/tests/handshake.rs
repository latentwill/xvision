use std::path::PathBuf;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use xvision_agent_client::{AgentClient, AgentClientError, UdsTransport};

async fn start_fake_sidecar(sock: PathBuf, protocol_version: &'static str) {
    let listener = UnixListener::bind(&sock).unwrap();
    tokio::spawn(async move {
        if let Ok((conn, _)) = listener.accept().await {
            let (r, mut w) = conn.into_split();
            let mut br = BufReader::new(r);
            let mut line = String::new();
            while br.read_line(&mut line).await.unwrap_or(0) > 0 {
                let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(req["method"], "runtime.health");
                let id = req["id"].clone();
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocol_version": protocol_version,
                        "sidecar_version": "0.1.0",
                        // Test fixture: any semver-shaped value is fine; the assertion below just verifies field round-trip.
                        "cline_sdk_version": "1.0.0",
                        "status": "ok"
                    }
                });
                let mut out = serde_json::to_vec(&resp).unwrap();
                out.push(b'\n');
                w.write_all(&out).await.unwrap();
                w.flush().await.unwrap();
                line.clear();
            }
        }
    });
}

#[tokio::test]
async fn handshake_accepts_matching_protocol() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    start_fake_sidecar(sock.clone(), "0.1.0").await;
    let t = UdsTransport::connect(&sock).await.unwrap();
    let h = AgentClient::handshake(&t).await.expect("handshake ok");
    assert_eq!(h.protocol_version, "0.1.0");
    assert_eq!(h.sidecar_version, "0.1.0");
    assert_eq!(h.cline_sdk_version, "1.0.0");
    assert_eq!(h.status, "ok");
}

#[tokio::test]
async fn handshake_rejects_incompatible_protocol() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    start_fake_sidecar(sock.clone(), "9.9.9").await;
    let t = UdsTransport::connect(&sock).await.unwrap();
    let err = AgentClient::handshake(&t).await.expect_err("should fail");
    assert!(matches!(err, AgentClientError::IncompatibleVersion(_)));
}
