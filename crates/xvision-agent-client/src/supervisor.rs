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
    // OS pid of the sidecar, captured at spawn time. `Child::id()` returns
    // `None` once the child is reaped, so we snapshot it here while the
    // process is live (U13: `eval cancel` needs this pid to SIGTERM the
    // sidecar). `None` only if the OS never assigned a pid (should not happen
    // for a successfully-spawned child).
    pid: Option<u32>,
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

        // Capture the pid now — `Child::id()` returns `None` after the child is
        // reaped, so a later read would miss it (U13).
        let pid = child.id();

        // Wait for the structured `ready` event on stderr, then spawn a
        // background task to forward all subsequent stderr to tracing so
        // Node runtime errors and panics survive in Docker logs.
        let stderr = child.stderr.take().ok_or(AgentClientError::TransportClosed)?;
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();

        let ready = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                buf.clear();
                let n = reader.read_line(&mut buf).await?;
                if n == 0 {
                    return Err(AgentClientError::TransportClosed);
                }
                let line = buf.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if v.get("event").and_then(|x| x.as_str()) == Some("ready") {
                        // Spawn background reader for post-ready stderr.
                        tokio::spawn(async move {
                            let mut buf2 = String::new();
                            loop {
                                buf2.clear();
                                match reader.read_line(&mut buf2).await {
                                    Ok(0) => return,
                                    Ok(_) => {
                                        let l = buf2.trim();
                                        if !l.is_empty() {
                                            tracing::error!(
                                                target: "xvision_agentd::stderr",
                                                "{l}"
                                            );
                                        }
                                    }
                                    Err(_) => return,
                                }
                            }
                        });
                        return Ok::<(), AgentClientError>(());
                    }
                }
                // Pre-ready stderr — log at warn so startup issues are visible.
                tracing::warn!(
                    target: "xvision_agentd::stderr",
                    "{line}"
                );
            }
        })
        .await;

        match ready {
            Ok(Ok(())) => Ok(Self {
                child: Some(child),
                pid,
                socket_path: socket_path.to_path_buf(),
            }),
            _ => {
                let _ = child.kill().await;
                Err(AgentClientError::TransportClosed)
            }
        }
    }

    /// OS pid of the sidecar, snapshotted at spawn time. `None` only if the
    /// OS never assigned a pid. Used by `eval cancel` to SIGTERM the sidecar
    /// (U13).
    pub(crate) fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub(crate) async fn shutdown(mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            child.kill().await?;
        }
        Ok(())
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            tokio::task::spawn(async move {
                let _ = child.kill().await;
            });
        }
    }
}
