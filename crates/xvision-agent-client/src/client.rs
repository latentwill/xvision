use std::path::Path;

use crate::errors::{AgentClientError, Result};
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
}

impl AgentClient {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let supervisor = Supervisor::spawn(bin, socket_path).await?;
        let transport = UdsTransport::connect(&supervisor.socket_path).await?;
        let versions = Self::handshake(&transport).await?;
        Ok(Self { transport, supervisor, versions })
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
}
