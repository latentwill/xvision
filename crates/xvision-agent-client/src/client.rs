use std::path::Path;
use std::sync::Arc;

use crate::errors::{AgentClientError, Result};
use crate::tool_dispatch::{serve_callbacks, ToolDispatch};
use crate::protocol::{
    RuntimeHealthResult, ToolDescriptor, ToolRegistryGetResult, ToolRegistrySetParams,
    ToolRegistrySetResult, SUPPORTED_PROTOCOL_VERSION,
};
use crate::supervisor::Supervisor;
use crate::transport::UdsTransport;

pub struct AgentClient {
    transport: UdsTransport,
    supervisor: Supervisor,
    versions: RuntimeHealthResult,
    callback_handle: Option<tokio::task::JoinHandle<()>>,
}

impl AgentClient {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let supervisor = Supervisor::spawn(bin, socket_path, None).await?;
        let transport = UdsTransport::connect(&supervisor.socket_path).await?;
        let versions = Self::handshake(&transport).await?;
        Ok(Self { transport, supervisor, versions, callback_handle: None })
    }

    pub async fn handshake(transport: &UdsTransport) -> Result<RuntimeHealthResult> {
        let h: RuntimeHealthResult = transport.call::<(), _>("runtime.health", None).await?;
        if h.protocol_version != SUPPORTED_PROTOCOL_VERSION {
            return Err(AgentClientError::IncompatibleVersion(format!(
                "sidecar speaks protocol {}; client supports {}",
                h.protocol_version, SUPPORTED_PROTOCOL_VERSION
            )));
        }
        Ok(h)
    }

    pub fn versions(&self) -> &RuntimeHealthResult {
        &self.versions
    }

    pub async fn health(&self) -> Result<RuntimeHealthResult> {
        self.transport.call::<(), _>("runtime.health", None).await
    }

    pub async fn shutdown(self) -> Result<()> {
        if let Some(h) = &self.callback_handle {
            h.abort();
        }
        self.supervisor.shutdown().await
    }

    pub async fn register_tools(&self, tools: Vec<ToolDescriptor>) -> Result<ToolRegistrySetResult> {
        self.transport
            .call::<ToolRegistrySetParams, ToolRegistrySetResult>(
                "tool.registry.set",
                Some(ToolRegistrySetParams { tools }),
            )
            .await
    }

    pub async fn list_tools(&self) -> Result<ToolRegistryGetResult> {
        self.transport
            .call::<(), ToolRegistryGetResult>("tool.registry.get", None)
            .await
    }

    pub async fn spawn_with_callbacks(
        bin: &Path,
        socket_path: &Path,
        callback_socket_path: &Path,
        dispatch: Arc<dyn ToolDispatch>,
    ) -> Result<Self> {
        let callback_handle = serve_callbacks(callback_socket_path, dispatch).await?;

        // If anything below fails, abort the accept loop and unlink the
        // callback socket file so a retry with the same path doesn't hit
        // EADDRINUSE.
        let supervisor = match Supervisor::spawn(bin, socket_path, Some(callback_socket_path)).await {
            Ok(s) => s,
            Err(e) => {
                callback_handle.abort();
                let _ = std::fs::remove_file(callback_socket_path);
                return Err(e);
            }
        };
        let transport = match UdsTransport::connect(&supervisor.socket_path).await {
            Ok(t) => t,
            Err(e) => {
                callback_handle.abort();
                let _ = std::fs::remove_file(callback_socket_path);
                return Err(e);
            }
        };
        let versions = match Self::handshake(&transport).await {
            Ok(v) => v,
            Err(e) => {
                callback_handle.abort();
                let _ = std::fs::remove_file(callback_socket_path);
                return Err(e);
            }
        };
        Ok(Self { transport, supervisor, versions, callback_handle: Some(callback_handle) })
    }

    pub async fn invoke_tool_via_sidecar(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[derive(serde::Serialize)]
        struct P<'a> { name: &'a str, input: serde_json::Value }
        self.transport
            .call::<P, serde_json::Value>("tool.invoke", Some(P { name, input }))
            .await
    }
}
