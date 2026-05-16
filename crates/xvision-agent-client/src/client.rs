use std::path::Path;

use crate::errors::Result;
use crate::protocol::RuntimeHealthResult;
use crate::supervisor::Supervisor;
use crate::transport::UdsTransport;

pub struct AgentClient {
    transport: UdsTransport,
    supervisor: Supervisor,
}

impl AgentClient {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let supervisor = Supervisor::spawn(bin, socket_path).await?;
        let transport = UdsTransport::connect(&supervisor.socket_path).await?;
        Ok(Self { transport, supervisor })
    }

    pub async fn health(&self) -> Result<RuntimeHealthResult> {
        self.transport.call::<(), _>("runtime.health", None).await
    }

    pub async fn shutdown(self) -> Result<()> {
        self.supervisor.shutdown().await
    }
}
