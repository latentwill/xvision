use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use xvision_ipc::LocalListener;

/// Implemented by anything that can resolve a tool name to a JSON-in/JSON-out
/// callable. The engine crate provides an impl over its existing
/// `ToolRegistry` in a later wave; Wave 1 tests provide their own.
#[async_trait]
pub trait ToolDispatch: Send + Sync + 'static {
    async fn invoke(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, ToolDispatchError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolDispatchError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("tool failed: {0}")]
    Failed(String),
}

#[derive(Debug, Deserialize)]
struct InvokeRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: u64,
    method: String,
    params: InvokeParams,
}

#[derive(Debug, Deserialize)]
struct InvokeParams {
    name: String,
    input: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct RpcOk {
    jsonrpc: &'static str,
    id: u64,
    result: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct RpcErr {
    jsonrpc: &'static str,
    id: u64,
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: i64,
    message: String,
}

pub async fn serve_callbacks(
    socket_path: &Path,
    dispatch: Arc<dyn ToolDispatch>,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    // `LocalListener::bind` best-effort unlinks a stale unix socket left by a
    // crashed prior process (the caller picks unique paths, typically under a
    // TempDir); on windows it creates the first named-pipe instance.
    let mut listener = LocalListener::bind(socket_path)?;
    let handle = tokio::spawn(async move {
        loop {
            let Ok(conn) = listener.accept().await else {
                continue;
            };
            let dispatch = dispatch.clone();
            tokio::spawn(async move {
                let (r, mut w) = conn.into_split();
                let mut br = BufReader::new(r);
                let mut line = String::new();
                while br.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let bytes = match serde_json::from_str::<InvokeRequest>(&line) {
                        Ok(req) if req.method == "tool.invoke" => {
                            match dispatch.invoke(&req.params.name, req.params.input).await {
                                Ok(out) => serde_json::to_vec(&RpcOk {
                                    jsonrpc: "2.0",
                                    id: req.id,
                                    result: out,
                                })
                                .unwrap_or_default(),
                                Err(ToolDispatchError::UnknownTool(n)) => serde_json::to_vec(&RpcErr {
                                    jsonrpc: "2.0",
                                    id: req.id,
                                    error: ErrorBody {
                                        code: -32001,
                                        message: format!("unknown tool: {n}"),
                                    },
                                })
                                .unwrap_or_default(),
                                Err(ToolDispatchError::Failed(m)) => serde_json::to_vec(&RpcErr {
                                    jsonrpc: "2.0",
                                    id: req.id,
                                    error: ErrorBody {
                                        code: -32001,
                                        message: m,
                                    },
                                })
                                .unwrap_or_default(),
                            }
                        }
                        Ok(req) => serde_json::to_vec(&RpcErr {
                            jsonrpc: "2.0",
                            id: req.id,
                            error: ErrorBody {
                                code: -32601,
                                message: format!("unknown method: {}", req.method),
                            },
                        })
                        .unwrap_or_default(),
                        Err(e) => {
                            // Parse error has no parsed id. Use 0 as a sentinel — the
                            // sidecar treats this as an unsolicited error response.
                            let mut bytes = serde_json::to_vec(&RpcErr {
                                jsonrpc: "2.0",
                                id: 0,
                                error: ErrorBody {
                                    code: -32700,
                                    message: e.to_string(),
                                },
                            })
                            .unwrap_or_default();
                            bytes.push(b'\n');
                            let _ = w.write_all(&bytes).await;
                            let _ = w.flush().await;
                            line.clear();
                            continue;
                        }
                    };
                    let _ = w.write_all(&bytes).await;
                    let _ = w.write_all(b"\n").await;
                    let _ = w.flush().await;
                    line.clear();
                }
            });
        }
    });
    Ok(handle)
}
