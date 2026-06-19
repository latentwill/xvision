use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use xvision_ipc::LocalStream;

use crate::errors::{AgentClientError, Result};
use crate::protocol::{JsonRpcErrorBody, JsonRpcRequest, JsonRpcResponse};

/// Mutex-guarded UDS transport. Wave 1: synchronous request-response only.
pub struct UdsTransport {
    inner: Mutex<TransportInner>,
    next_id: AtomicU64,
}

struct TransportInner {
    reader: BufReader<tokio::io::ReadHalf<LocalStream>>,
    writer: tokio::io::WriteHalf<LocalStream>,
}

impl UdsTransport {
    pub async fn connect(socket_path: impl AsRef<Path>) -> Result<Self> {
        let stream = LocalStream::connect(socket_path).await?;
        let (r, w) = stream.into_split();
        Ok(Self {
            inner: Mutex::new(TransportInner {
                reader: BufReader::new(r),
                writer: w,
            }),
            next_id: AtomicU64::new(1),
        })
    }

    pub async fn call<P: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Option<P>,
    ) -> Result<R> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };

        let mut guard = self.inner.lock().await;
        let mut line = serde_json::to_vec(&req)?;
        line.push(b'\n');
        guard.writer.write_all(&line).await?;
        guard.writer.flush().await?;

        let mut buf = String::new();
        let n = guard.reader.read_line(&mut buf).await?;
        if n == 0 {
            return Err(AgentClientError::TransportClosed);
        }
        let resp: JsonRpcResponse<R> = serde_json::from_str(&buf)?;
        if let Some(err) = resp.error {
            let JsonRpcErrorBody { code, message, .. } = err;
            return Err(AgentClientError::Rpc { code, message });
        }
        resp.result.ok_or(AgentClientError::MalformedResponse)
    }
}
