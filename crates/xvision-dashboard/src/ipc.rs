//! Unix socket IPC bridge for autooptimizer cycle progress events (AR-3).
//!
//! The dashboard listens on a Unix domain socket; `xvn optimize run
//! --ipc-socket <path>` connects and streams newline-delimited JSON
//! `CycleProgressEvent` messages. Each line is deserialized and broadcast into
//! the `AppState::autooptimizer_tx` channel, which feeds the
//! `GET /api/autooptimizer/events` SSE endpoint in real time — so a CLI-driven
//! optimizer run shows up live on the dashboard /optimizer page (GH #968).
//!
//! # Usage
//!
//! ```text
//! # Terminal 1 — dashboard (socket path set via ServeOpts)
//! xvn dashboard serve --autooptimizer-ipc-socket /tmp/xvn-optimizer.sock
//!
//! # Terminal 2 — optimizer run (events stream to connected browser tabs).
//! # The CLI auto-connects to /tmp/xvn-optimizer.sock when it exists, so the
//! # explicit --ipc-socket is only needed for a non-default path.
//! xvn optimize run --strategy <id> --ipc-socket /tmp/xvn-optimizer.sock
//! ```
//!
//! Multiple clients may connect simultaneously; each connection is handled
//! on its own spawned task. Stale or closed sockets are cleaned up automatically.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast::Sender;
use xvision_ipc::{LocalListener, LocalStream};

use xvision_engine::autooptimizer::progress::CycleProgressEvent;

/// Spawn a background task that listens on `socket_path` for incoming
/// newline-delimited JSON `CycleProgressEvent` messages and broadcasts them
/// into `tx`.
///
/// The task runs for the lifetime of the server process. The
/// `JoinHandle` is intentionally dropped by the caller.
///
/// If the socket file already exists (e.g. after an unclean shutdown)
/// it is removed before binding so `bind` does not fail.
pub fn spawn_autooptimizer_subscriber(
    socket_path: PathBuf,
    tx: Sender<CycleProgressEvent>,
) -> anyhow::Result<()> {
    // `LocalListener::bind` best-effort removes a stale unix socket from a
    // previous run; on windows it arms the first named-pipe instance.
    let mut listener = LocalListener::bind(&socket_path)?;
    tracing::info!(
        path = %socket_path.display(),
        "autooptimizer IPC socket listening",
    );

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok(stream) => {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        handle_ipc_client(stream, tx).await;
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "autooptimizer IPC: accept error; listener closing",
                    );
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Read newline-delimited JSON `CycleProgressEvent` from one connected client
/// and broadcast each event into `tx`. Returns when the client disconnects.
async fn handle_ipc_client(stream: LocalStream, tx: Sender<CycleProgressEvent>) {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim().to_owned();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<CycleProgressEvent>(&line) {
                    Ok(event) => {
                        // `send` only fails if there are no receivers yet,
                        // which is fine — we just ignore the error.
                        let _ = tx.send(event);
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            line_prefix = &line[..line.len().min(80)],
                            "autooptimizer IPC: could not deserialize CycleProgressEvent; skipping line",
                        );
                    }
                }
            }
            Ok(None) => {
                // Client closed the connection cleanly.
                break;
            }
            Err(e) => {
                tracing::warn!(error = %e, "autooptimizer IPC: read error; dropping client");
                break;
            }
        }
    }
}
