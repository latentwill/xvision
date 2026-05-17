use std::path::Path;
use std::sync::Arc;

use crate::errors::{AgentClientError, Result};
use crate::protocol::{
    EndRunParams, EndRunResult, RuntimeHealthResult, StartRunParams, StartRunResult, StepParams,
    StepResult, ToolDescriptor, ToolRegistryGetResult, ToolRegistrySetParams, ToolRegistrySetResult,
    SUPPORTED_PROTOCOL_VERSION,
};
use crate::supervisor::Supervisor;
use crate::tool_dispatch::{serve_callbacks, ToolDispatch};
use crate::transport::UdsTransport;

/// Combines a spawned `xvision-agentd` sidecar with a JSON-RPC transport.
///
/// Callers are expected to invoke [`AgentClient::shutdown`] when done.
/// `shutdown` aborts the callback listener, removes the callback socket
/// file (if any), and kills the sidecar process. The sidecar is also
/// killed via tokio's `kill_on_drop(true)` if the client is dropped
/// without `shutdown`, but the callback socket file will leak in that
/// case — call `shutdown` explicitly for production use.
pub struct AgentClient {
    transport: UdsTransport,
    supervisor: Supervisor,
    versions: RuntimeHealthResult,
    callback_handle: Option<tokio::task::JoinHandle<()>>,
    callback_socket_path: Option<std::path::PathBuf>,
}

impl AgentClient {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let supervisor = Supervisor::spawn(bin, socket_path, None).await?;
        let transport = UdsTransport::connect(&supervisor.socket_path).await?;
        let versions = Self::handshake(&transport).await?;
        Ok(Self {
            transport,
            supervisor,
            versions,
            callback_handle: None,
            callback_socket_path: None,
        })
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
        if let Some(path) = &self.callback_socket_path {
            let _ = std::fs::remove_file(path);
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

    pub async fn start_run(&self, params: StartRunParams) -> Result<StartRunResult> {
        self.transport
            .call::<StartRunParams, StartRunResult>("session.start_run", Some(params))
            .await
    }

    pub async fn step(&self, params: StepParams) -> Result<StepResult> {
        self.transport
            .call::<StepParams, StepResult>("session.step", Some(params))
            .await
    }

    pub async fn end_run(&self, params: EndRunParams) -> Result<EndRunResult> {
        self.transport
            .call::<EndRunParams, EndRunResult>("session.end_run", Some(params))
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
        Ok(Self {
            transport,
            supervisor,
            versions,
            callback_handle: Some(callback_handle),
            callback_socket_path: Some(callback_socket_path.to_path_buf()),
        })
    }

    pub async fn invoke_tool_via_sidecar(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[derive(serde::Serialize)]
        struct P<'a> {
            name: &'a str,
            input: serde_json::Value,
        }
        self.transport
            .call::<P, serde_json::Value>("tool.invoke", Some(P { name, input }))
            .await
    }
}
