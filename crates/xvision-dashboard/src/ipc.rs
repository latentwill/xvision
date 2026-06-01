//! Unix socket IPC bridge for autooptimizer cycle progress events (AR-3).
//!
//! The dashboard listens on a Unix domain socket; `xvn autooptimizer
//! mutate-once --ipc-socket <path>` connects and streams newline-delimited
//! JSON `CycleProgressEvent` messages. Each line is deserialized and
//! broadcast into the `AppState::autooptimizer_tx` channel, which feeds the
//! `GET /api/autooptimizer/events` SSE endpoint in real time.
//!
//! # Usage
//!
//! ```text
//! # Terminal 1 — dashboard (socket path set via ServeOpts)
//! xvn dashboard serve --autooptimizer-ipc-socket /tmp/xvn-events.sock
//!
//! # Terminal 2 — evening cycle (events stream to connected browser tabs)
//! xvn autooptimizer mutate-once <parent_hash> --ipc-socket /tmp/xvn-events.sock
//! ```
//!
//! Multiple clients may connect simultaneously; each connection is handled
//! on its own spawned task. Stale or closed sockets are cleaned up automatically.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::broadcast::Sender;

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
    // Remove stale socket file from a previous run.
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    tracing::info!(
        path = %socket_path.display(),
        "autooptimizer IPC socket listening",
    );

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
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
async fn handle_ipc_client(
    stream: tokio::net::UnixStream,
    tx: Sender<CycleProgressEvent>,
) {
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
