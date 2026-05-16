use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::errors::{AgentClientError, Result};

pub struct Supervisor {
    child: Option<Child>,
    pub socket_path: PathBuf,
}

impl Supervisor {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let mut cmd = Command::new("node");
        cmd.arg(bin)
            .arg("--socket")
            .arg(socket_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;

        // Wait for the structured `ready` event on stderr.
        let stderr = child
            .stderr
            .take()
            .ok_or(AgentClientError::TransportClosed)?;
        let mut lines = BufReader::new(stderr).lines();

        let ready = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(line) = lines.next_line().await? {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    if v.get("event").and_then(|x| x.as_str()) == Some("ready") {
                        return Ok::<(), AgentClientError>(());
                    }
                }
            }
            Err(AgentClientError::TransportClosed)
        })
        .await;

        match ready {
            Ok(Ok(())) => Ok(Self {
                child: Some(child),
                socket_path: socket_path.to_path_buf(),
            }),
            _ => {
                let _ = child.kill().await;
                Err(AgentClientError::TransportClosed)
            }
        }
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
        Ok(())
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}
