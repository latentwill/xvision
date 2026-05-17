use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::errors::{AgentClientError, Result};

pub(crate) struct Supervisor {
    // Option so Drop can take() ownership and avoid double-kill after
    // explicit shutdown. `take()` leaves None; Drop's guard sees None
    // and skips the second kill.
    child: Option<Child>,
    pub(crate) socket_path: PathBuf,
}

impl Supervisor {
    pub(crate) async fn spawn(
        bin: &Path,
        socket_path: &Path,
        callback_socket_path: Option<&Path>,
        event_socket_path: Option<&Path>,
    ) -> Result<Self> {
        let mut cmd = Command::new("node");
        cmd.arg(bin).arg("--socket").arg(socket_path);
        if let Some(cb) = callback_socket_path {
            cmd.arg("--callback-socket").arg(cb);
        }
        if let Some(ev) = event_socket_path {
            cmd.arg("--event-socket").arg(ev);
        }
        cmd.stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;

        // Wait for the structured `ready` event on stderr.
        let stderr = child.stderr.take().ok_or(AgentClientError::TransportClosed)?;
        let mut lines = BufReader::new(stderr).lines();
        // Once we find the ready line, `lines` is dropped and stderr is no
        // longer drained. Post-ready stderr from the sidecar (Node runtime
        // errors, panics) buffers in the pipe and is not surfaced here.
        // Task 7+ will spawn a background reader that forwards to tracing.

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

        // Wave 1: coarse error mapping. Timeout, EOF, and missing stderr all
        // collapse to TransportClosed. Task 7 will introduce
        // SidecarSpawnFailed { reason } so callers can distinguish them.
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

    pub(crate) async fn shutdown(mut self) -> Result<()> {
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
