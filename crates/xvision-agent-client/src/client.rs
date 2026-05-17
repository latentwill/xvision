use std::path::Path;
use std::sync::Arc;

use crate::errors::{AgentClientError, Result};
use crate::event_sink::{start_event_sink, EventSinkHandle, SidecarFingerprint};
use crate::protocol::{
    EndRunParams, EndRunResult, RuntimeHealthResult, StartRunParams, StartRunResult, StepParams,
    StepResult, ToolDescriptor, ToolRegistryGetResult, ToolRegistrySetParams, ToolRegistrySetResult,
    SUPPORTED_PROTOCOL_VERSION,
};
use crate::supervisor::Supervisor;
use crate::tool_dispatch::{serve_callbacks, ToolDispatch};
use crate::transport::UdsTransport;
use xvision_observability::RunEventBus;

/// Combines a spawned `xvision-agentd` sidecar with a JSON-RPC transport.
///
/// Prefer [`AgentClient::shutdown`] for explicit teardown — it awaits the
/// supervisor exit and returns any process-level error. If the client is
/// dropped without `shutdown`, the destructor still aborts the callback
/// listener and unlinks the callback socket file (best-effort,
/// synchronous), and tokio's `kill_on_drop(true)` reaps the child
/// process. The Drop fallback exists so repeated client construction in
/// long-running processes does not leak OS resources; production code
/// should still call `shutdown` explicitly to observe shutdown errors.
pub struct AgentClient {
    transport: UdsTransport,
    // Wrapped in Option so `shutdown` can take it out without a partial
    // move (which Drop-bearing structs forbid). The Drop impl below does
    // not touch the supervisor — the underlying tokio child uses
    // `kill_on_drop(true)`, so the process is reaped either way.
    supervisor: Option<Supervisor>,
    versions: RuntimeHealthResult,
    callback_handle: Option<tokio::task::JoinHandle<()>>,
    callback_socket_path: Option<std::path::PathBuf>,
    event_sink: Option<EventSinkHandle>,
}

impl AgentClient {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let supervisor = Supervisor::spawn(bin, socket_path, None, None).await?;
        let transport = UdsTransport::connect(&supervisor.socket_path).await?;
        let versions = Self::handshake(&transport).await?;
        Ok(Self {
            transport,
            supervisor: Some(supervisor),
            versions,
            callback_handle: None,
            callback_socket_path: None,
            event_sink: None,
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

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(h) = self.callback_handle.take() {
            h.abort();
        }
        if let Some(path) = self.callback_socket_path.take() {
            let _ = std::fs::remove_file(&path);
        }
        if let Some(sink) = self.event_sink.take() {
            sink.shutdown().await;
        }
        if let Some(sup) = self.supervisor.take() {
            sup.shutdown().await
        } else {
            Ok(())
        }
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
        let supervisor = match Supervisor::spawn(bin, socket_path, Some(callback_socket_path), None).await {
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
            supervisor: Some(supervisor),
            versions,
            callback_handle: Some(callback_handle),
            callback_socket_path: Some(callback_socket_path.to_path_buf()),
            event_sink: None,
        })
    }

    /// Spawn the sidecar with both a tool-callback path AND an
    /// observability event sink. Notifications from the sidecar are
    /// translated to `RunEvent`s and published to `bus` so the Phase-A
    /// `SqliteRecorder` (or any other subscriber) can persist them.
    ///
    /// The sidecar fingerprint (`sidecar_version`, `cline_sdk_version`,
    /// `protocol_version`) is captured from the IPC handshake here and
    /// threaded onto every `RunStarted` event the sink publishes.
    pub async fn spawn_with_event_sink(
        bin: &Path,
        socket_path: &Path,
        callback_socket_path: &Path,
        event_socket_path: &Path,
        dispatch: Arc<dyn ToolDispatch>,
        bus: Arc<RunEventBus>,
    ) -> Result<Self> {
        let callback_handle = serve_callbacks(callback_socket_path, dispatch).await?;

        // Bind the event listener BEFORE we tell the sidecar where the
        // socket lives — otherwise the sidecar's first emit may race the
        // accept() and silently no-op (the event-client.ts path swallows
        // connection errors).
        // Start with an empty fingerprint; populated after handshake.
        let initial_fp = SidecarFingerprint::default();
        let event_sink = match start_event_sink(event_socket_path, bus.clone(), initial_fp).await {
            Ok(h) => h,
            Err(e) => {
                callback_handle.abort();
                let _ = std::fs::remove_file(callback_socket_path);
                return Err(AgentClientError::from(e));
            }
        };

        let supervisor = match Supervisor::spawn(
            bin,
            socket_path,
            Some(callback_socket_path),
            Some(event_socket_path),
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                callback_handle.abort();
                event_sink.accept_handle.abort();
                let _ = std::fs::remove_file(callback_socket_path);
                let _ = std::fs::remove_file(event_socket_path);
                return Err(e);
            }
        };
        let transport = match UdsTransport::connect(&supervisor.socket_path).await {
            Ok(t) => t,
            Err(e) => {
                callback_handle.abort();
                event_sink.accept_handle.abort();
                let _ = std::fs::remove_file(callback_socket_path);
                let _ = std::fs::remove_file(event_socket_path);
                return Err(e);
            }
        };
        let versions = match Self::handshake(&transport).await {
            Ok(v) => v,
            Err(e) => {
                callback_handle.abort();
                event_sink.accept_handle.abort();
                let _ = std::fs::remove_file(callback_socket_path);
                let _ = std::fs::remove_file(event_socket_path);
                return Err(e);
            }
        };

        // Restart the event sink with the fingerprint populated. The
        // sidecar opens its event-client connection lazily on the first
        // emit, so by the time we re-bind here the sidecar has not yet
        // pushed anything (no session.start_run has run). We tear down
        // the placeholder sink and start a fresh one bound to the same
        // path with the fingerprint stamped in.
        let fp = SidecarFingerprint {
            sidecar_version: Some(versions.sidecar_version.clone()),
            cline_sdk_version: Some(versions.cline_sdk_version.clone()),
            protocol_version: Some(versions.protocol_version.clone()),
        };
        event_sink.accept_handle.abort();
        let _ = std::fs::remove_file(event_socket_path);
        let event_sink = match start_event_sink(event_socket_path, bus.clone(), fp).await {
            Ok(h) => h,
            Err(e) => {
                callback_handle.abort();
                let _ = std::fs::remove_file(callback_socket_path);
                return Err(AgentClientError::from(e));
            }
        };

        Ok(Self {
            transport,
            supervisor: Some(supervisor),
            versions,
            callback_handle: Some(callback_handle),
            callback_socket_path: Some(callback_socket_path.to_path_buf()),
            event_sink: Some(event_sink),
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

impl Drop for AgentClient {
    /// Best-effort cleanup if the caller never invoked [`AgentClient::shutdown`]:
    /// abort the callback accept loop and unlink the callback socket file.
    /// The sidecar process is reaped by tokio's `kill_on_drop(true)` on the
    /// supervisor's child handle, so we don't need to touch it here.
    fn drop(&mut self) {
        if let Some(h) = self.callback_handle.take() {
            h.abort();
        }
        if let Some(path) = self.callback_socket_path.take() {
            let _ = std::fs::remove_file(&path);
        }
        if let Some(sink) = self.event_sink.take() {
            sink.accept_handle.abort();
            let _ = std::fs::remove_file(&sink.socket_path);
        }
    }
}
