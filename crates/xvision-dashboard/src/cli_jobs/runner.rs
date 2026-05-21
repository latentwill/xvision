use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, watch, Mutex};

use super::model::{CliJob, CliJobStatus, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_RUNTIME_SECONDS};
use super::store::{CliJobStore, FinishParams};

#[cfg(unix)]
use nix::sys::signal::Signal;

pub const DEFAULT_TIMEOUT_SECS: u64 = 300;
pub const MAX_TIMEOUT_SECS: u64 = 6 * 60 * 60;

/// How long to wait after SIGTERM before sending SIGKILL during cancellation.
const SIGTERM_GRACE_SECS: u64 = 5;

const STREAM_READ_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CliJobEvent {
    JobStarted {
        job_id: String,
        argv: Vec<String>,
    },
    StdoutChunk {
        job_id: String,
        chunk: String,
    },
    StderrChunk {
        job_id: String,
        chunk: String,
    },
    JobFinished {
        job_id: String,
        status: String,
        exit_code: Option<i64>,
        timed_out: bool,
        cancelled: bool,
        error_message: Option<String>,
    },
}

impl CliJobEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::JobStarted { .. } => "job_started",
            Self::StdoutChunk { .. } => "stdout_chunk",
            Self::StderrChunk { .. } => "stderr_chunk",
            Self::JobFinished { .. } => "job_finished",
        }
    }
}

pub struct CliJobEventBus {
    senders: tokio::sync::Mutex<HashMap<String, broadcast::Sender<CliJobEvent>>>,
}

impl Default for CliJobEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl CliJobEventBus {
    pub fn new() -> Self {
        Self {
            senders: Default::default(),
        }
    }

    pub async fn sender(&self, job_id: &str) -> broadcast::Sender<CliJobEvent> {
        let mut guard = self.senders.lock().await;
        guard
            .entry(job_id.to_string())
            .or_insert_with(|| broadcast::channel(1024).0)
            .clone()
    }

    pub async fn subscribe(&self, job_id: &str) -> broadcast::Receiver<CliJobEvent> {
        self.sender(job_id).await.subscribe()
    }

    pub async fn emit(&self, job_id: &str, event: CliJobEvent) {
        let _ = self.sender(job_id).await.send(event);
    }

    pub async fn drop_channel(&self, job_id: &str) {
        self.senders.lock().await.remove(job_id);
    }
}

#[derive(Clone)]
pub struct CliJobRunner {
    pool: SqlitePool,
    cli_command: PathBuf,
    events: Arc<CliJobEventBus>,
    cancels: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

impl CliJobRunner {
    pub fn new(pool: SqlitePool, cli_command: PathBuf) -> Self {
        Self {
            pool,
            cli_command,
            events: Arc::new(CliJobEventBus::new()),
            cancels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start(&self, job: CliJob) {
        let runner = self.clone();
        tokio::spawn(async move {
            runner.run(job).await;
        });
    }

    pub async fn subscribe(&self, job_id: &str) -> broadcast::Receiver<CliJobEvent> {
        self.events.subscribe(job_id).await
    }

    /// Signal a running job to cancel. Sends a message over the cancel watch
    /// channel; the runner loop picks it up and executes SIGTERM → SIGKILL.
    pub async fn cancel(&self, job_id: &str) {
        let cancel_tx = {
            let guard = self.cancels.lock().await;
            guard.get(job_id).cloned()
        };
        if let Some(cancel_tx) = cancel_tx {
            let _ = cancel_tx.send(true);
        }
    }

    async fn run(self, job: CliJob) {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.cancels.lock().await.insert(job.job_id.clone(), cancel_tx);

        let run_result = self.run_inner(job.clone(), cancel_rx).await;
        if let Err(error) = run_result {
            let store = CliJobStore::new(self.pool.clone());
            let message = error.to_string();
            tracing::error!(
                target: "xvision::dashboard",
                job_id = %job.job_id,
                error = %message,
                "cli job runner failed",
            );
            let _ = store
                .finish(&job.job_id, CliJobStatus::Failed, None, Some(message.clone()))
                .await;
            self.events
                .emit(
                    &job.job_id,
                    CliJobEvent::JobFinished {
                        job_id: job.job_id.clone(),
                        status: CliJobStatus::Failed.as_str().to_string(),
                        exit_code: None,
                        timed_out: false,
                        cancelled: false,
                        error_message: Some(message),
                    },
                )
                .await;
        }

        self.cancels.lock().await.remove(&job.job_id);
        self.events.drop_channel(&job.job_id).await;
    }

    async fn run_inner(&self, job: CliJob, mut cancel_rx: watch::Receiver<bool>) -> Result<()> {
        let store = CliJobStore::new(self.pool.clone());

        if matches!(store.get(&job.job_id).await?, Some(existing) if existing.cancel_requested) {
            store
                .finish(
                    &job.job_id,
                    CliJobStatus::Cancelled,
                    None,
                    Some("job cancelled before start".into()),
                )
                .await?;
            self.emit_finished(
                &job.job_id,
                CliJobStatus::Cancelled,
                None,
                false,
                true,
                Some("job cancelled before start".into()),
            )
            .await;
            return Ok(());
        }

        // Resolve per-job caps, falling back to defaults when the DB row has 0.
        let max_runtime_seconds = if job.max_runtime_seconds == 0 {
            DEFAULT_MAX_RUNTIME_SECONDS
        } else {
            job.max_runtime_seconds
        };
        let max_output_bytes = if job.max_output_bytes == 0 {
            DEFAULT_MAX_OUTPUT_BYTES
        } else {
            job.max_output_bytes
        };
        tracing::debug!(
            target: "xvision::dashboard",
            job_id = %job.job_id,
            max_runtime_seconds,
            max_output_bytes,
            timeout_secs = job.timeout_secs,
            "cli job runner starting with caps",
        );
        let mut command = Command::new(&self.cli_command);
        command
            .args(&job.argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            // Put each CLI job in its own process group so cancellation and caps
            // terminate descendants such as shell-spawned `sleep` processes that
            // inherit stdout/stderr FDs.
            command.process_group(0);
        }
        let mut child = command.spawn().with_context(|| {
            format!(
                "spawn '{}' for cli job '{}'",
                self.cli_command.display(),
                job.job_id
            )
        })?;
        // Persist the child PID for orphan-recovery after a restart.
        let child_pid = child.id();
        store.mark_running_with_pid(&job.job_id, child_pid).await?;

        self.events
            .emit(
                &job.job_id,
                CliJobEvent::JobStarted {
                    job_id: job.job_id.clone(),
                    argv: job.argv.clone(),
                },
            )
            .await;

        let stdout = child.stdout.take().context("take child stdout")?;
        let stderr = child.stderr.take().context("take child stderr")?;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let stdout_task = tokio::spawn(read_stream(stdout, StreamKind::Stdout, tx.clone()));
        let stderr_task = tokio::spawn(read_stream(stderr, StreamKind::Stderr, tx));

        let mut stdout_closed = false;
        let mut stderr_closed = false;
        let mut exit_status = None;
        let mut timed_out = false;
        let mut output_cap_exceeded = false;
        let mut runtime_cap_exceeded = false;
        let mut cancelled = false;
        let mut error_message = None;
        let mut cancel_signal: Option<String> = None;
        let mut cancelled_at: Option<String> = None;

        // Polling tick for child exit status.
        let mut ticker = tokio::time::interval(Duration::from_millis(25));
        ticker.tick().await;

        // caller-requested timeout (existing behaviour, unchanged).
        let timeout_secs = job.timeout_secs.max(1);
        let timeout = tokio::time::sleep(Duration::from_secs(timeout_secs));
        tokio::pin!(timeout);

        // dashboard-layer runtime cap (supervisor kills runaway child).
        let runtime_cap = tokio::time::sleep(Duration::from_secs(max_runtime_seconds));
        tokio::pin!(runtime_cap);

        while exit_status.is_none() || !stdout_closed || !stderr_closed {
            // If we have already killed/SIGTERMed the child (cap breach, cancel,
            // or timeout) and the child process has exited, we don't wait for
            // its stream reader tasks to finish draining — orphaned grandchildren
            // (e.g. a `sleep` spawned by a shell script) may hold the pipe FDs
            // open indefinitely. Abort the reader tasks and break.
            let killing = cancelled || output_cap_exceeded || runtime_cap_exceeded || timed_out;
            if killing && exit_status.is_some() {
                stdout_task.abort();
                stderr_task.abort();
                break;
            }

            tokio::select! {
                // Poll for child exit status on every tick. NOT gated on the
                // cap/cancel flags — after SIGTERM is sent we still need to
                // harvest the exit code once the child actually terminates.
                _ = ticker.tick(), if exit_status.is_none() => {
                    if let Some(status) = child.try_wait().context("poll cli child")? {
                        exit_status = Some(status);
                    }
                }
                maybe_msg = rx.recv(), if !stdout_closed || !stderr_closed => {
                    match maybe_msg {
                        Some(StreamMessage::Chunk { stream, chunk }) => {
                            // Check output cap before persisting.
                            if !output_cap_exceeded {
                                match stream {
                                    StreamKind::Stdout => {
                                        store.append_stdout(&job.job_id, &chunk).await?;
                                        self.events.emit(
                                            &job.job_id,
                                            CliJobEvent::StdoutChunk {
                                                job_id: job.job_id.clone(),
                                                chunk,
                                            },
                                        ).await;
                                    }
                                    StreamKind::Stderr => {
                                        store.append_stderr(&job.job_id, &chunk).await?;
                                        self.events.emit(
                                            &job.job_id,
                                            CliJobEvent::StderrChunk {
                                                job_id: job.job_id.clone(),
                                                chunk,
                                            },
                                        ).await;
                                    }
                                }
                                // Check cap after this write.
                                if exit_status.is_none() && store
                                    .output_bytes_exceed_cap(&job.job_id, max_output_bytes)
                                    .await
                                    .unwrap_or(false)
                                {
                                    output_cap_exceeded = true;
                                    error_message = Some(format!(
                                        "job exceeded output cap of {} bytes",
                                        max_output_bytes
                                    ));
                                    tracing::warn!(
                                        target: "xvision::dashboard",
                                        job_id = %job.job_id,
                                        max_output_bytes,
                                        "cli job exceeded output cap; sending SIGTERM",
                                    );
                                    send_sigkill(&mut child);
                                }
                            }
                        }
                        Some(StreamMessage::Closed(StreamKind::Stdout)) => stdout_closed = true,
                        Some(StreamMessage::Closed(StreamKind::Stderr)) => stderr_closed = true,
                        None => {
                            stdout_closed = true;
                            stderr_closed = true;
                        }
                    }
                }
                _ = &mut timeout, if exit_status.is_none() && !timed_out && !cancelled && !output_cap_exceeded && !runtime_cap_exceeded => {
                    timed_out = true;
                    error_message = Some(format!("job exceeded caller timeout of {timeout_secs}s"));
                    send_sigkill(&mut child);
                }
                _ = &mut runtime_cap, if exit_status.is_none() && !timed_out && !cancelled && !output_cap_exceeded && !runtime_cap_exceeded => {
                    runtime_cap_exceeded = true;
                    error_message = Some(format!(
                        "job exceeded dashboard runtime cap of {max_runtime_seconds}s"
                    ));
                    tracing::warn!(
                        target: "xvision::dashboard",
                        job_id = %job.job_id,
                        max_runtime_seconds,
                        "cli job exceeded runtime cap; sending SIGTERM",
                    );
                    send_sigterm_or_kill(&mut child);
                }
                changed = cancel_rx.changed(), if exit_status.is_none() && !timed_out && !cancelled && !output_cap_exceeded && !runtime_cap_exceeded => {
                    if changed.is_err() || !*cancel_rx.borrow() {
                        continue;
                    }
                    cancelled = true;
                    cancelled_at = Some(Utc::now().to_rfc3339());
                    cancel_signal = Some("SIGTERM".into());
                    error_message = Some("job cancelled".into());
                    tracing::info!(
                        target: "xvision::dashboard",
                        job_id = %job.job_id,
                        "cli job cancel requested; sending SIGTERM",
                    );
                    send_sigterm_or_kill(&mut child);
                }
            }
        }

        // Join stream reader tasks; treat cancellation (from the abort above
        // when a cap fires) as a clean exit — the child is already dead.
        match stdout_task.await {
            Ok(Ok(())) | Err(_) => {}
            Ok(Err(e)) => return Err(e).context("stdout reader error"),
        }
        match stderr_task.await {
            Ok(Ok(())) | Err(_) => {}
            Ok(Err(e)) => return Err(e).context("stderr reader error"),
        }

        // If we sent SIGTERM for cancel/cap-breach, enforce the 5-second
        // SIGTERM→SIGKILL grace period. `child.wait()` below honours
        // kill_on_drop so if the grace expires we escalate.
        if (cancelled || output_cap_exceeded || runtime_cap_exceeded) && exit_status.is_none() {
            match tokio::time::timeout(Duration::from_secs(SIGTERM_GRACE_SECS), child.wait()).await {
                Ok(Ok(status)) => exit_status = Some(status),
                Ok(Err(e)) => return Err(e).context("wait for cli child after SIGTERM"),
                Err(_) => {
                    // Grace period elapsed — escalate to SIGKILL.
                    tracing::warn!(
                        target: "xvision::dashboard",
                        job_id = %job.job_id,
                        "SIGTERM grace period elapsed; sending SIGKILL",
                    );
                    if cancelled {
                        cancel_signal = Some("SIGKILL".into());
                    }
                    send_sigkill(&mut child);
                    exit_status = Some(child.wait().await.context("wait for cli child after SIGKILL")?);
                }
            }
        }

        let status = if let Some(status) = exit_status {
            status
        } else {
            child.wait().await.context("wait for cli child exit")?
        };
        let exit_code = status.code().map(i64::from);

        let final_status = if output_cap_exceeded {
            CliJobStatus::OutputCapExceeded
        } else if runtime_cap_exceeded {
            CliJobStatus::RuntimeCapExceeded
        } else if timed_out {
            CliJobStatus::TimedOut
        } else if cancelled {
            CliJobStatus::Cancelled
        } else if status.success() {
            CliJobStatus::Succeeded
        } else {
            CliJobStatus::Failed
        };

        if matches!(final_status, CliJobStatus::Failed) && error_message.is_none() {
            error_message = Some(match exit_code {
                Some(code) => format!("xvn exited with code {code}"),
                None => "xvn terminated without an exit code".into(),
            });
        }

        store
            .finish_detailed(FinishParams {
                job_id: &job.job_id,
                status: final_status,
                exit_code,
                error_message: error_message.clone(),
                cancelled_at,
                cancel_signal,
                output_cap_exceeded,
                runtime_cap_exceeded,
            })
            .await?;

        self.emit_finished(
            &job.job_id,
            final_status,
            exit_code,
            timed_out || runtime_cap_exceeded,
            cancelled,
            error_message,
        )
        .await;

        Ok(())
    }

    async fn emit_finished(
        &self,
        job_id: &str,
        status: CliJobStatus,
        exit_code: Option<i64>,
        timed_out: bool,
        cancelled: bool,
        error_message: Option<String>,
    ) {
        self.events
            .emit(
                job_id,
                CliJobEvent::JobFinished {
                    job_id: job_id.to_string(),
                    status: status.as_str().to_string(),
                    exit_code,
                    timed_out,
                    cancelled,
                    error_message,
                },
            )
            .await;
    }
}

/// Send SIGTERM on Unix; fall back to `start_kill` (which delivers SIGKILL)
/// on platforms without `nix`. Jobs are spawned into their own process group,
/// so Unix signals target the group and terminate descendants too. The
/// 5-second grace period in the caller handles escalation to SIGKILL.
fn send_sigterm_or_kill(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        if send_signal_to_process_group(child, Signal::SIGTERM) {
            return;
        }
    }
    // Fallback (non-Unix or SIGTERM delivery failed): use tokio's kill (SIGKILL).
    let _ = child.start_kill();
}

fn send_sigkill(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        if send_signal_to_process_group(child, Signal::SIGKILL) {
            return;
        }
    }
    let _ = child.start_kill();
}

#[cfg(unix)]
fn send_signal_to_process_group(child: &mut tokio::process::Child, signal: Signal) -> bool {
    if let Some(pid) = child.id() {
        let pgid = nix::unistd::Pid::from_raw(-(pid as i32));
        match nix::sys::signal::kill(pgid, signal) {
            Ok(()) => {
                tracing::debug!(
                    target: "xvision::dashboard",
                    pid,
                    signal = ?signal,
                    "sent signal to cli job process group",
                );
                true
            }
            Err(e) => {
                tracing::warn!(
                    target: "xvision::dashboard",
                    pid,
                    signal = ?signal,
                    error = %e,
                    "process group signal delivery failed; falling back to child kill",
                );
                false
            }
        }
    } else {
        tracing::warn!(
            target: "xvision::dashboard",
            signal = ?signal,
            "child.id() returned None; process may have already exited",
        );
        false
    }
}

#[derive(Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

enum StreamMessage {
    Chunk { stream: StreamKind, chunk: String },
    Closed(StreamKind),
}

async fn read_stream<R>(
    mut reader: R,
    stream: StreamKind,
    tx: mpsc::UnboundedSender<StreamMessage>,
) -> Result<()>
where
    R: AsyncRead + Unpin,
{
    let mut buf = vec![0_u8; STREAM_READ_CHUNK_BYTES];
    loop {
        let read = reader.read(&mut buf).await.context("read cli output")?;
        if read == 0 {
            let _ = tx.send(StreamMessage::Closed(stream));
            return Ok(());
        }

        let chunk = String::from_utf8_lossy(&buf[..read]).into_owned();
        if tx.send(StreamMessage::Chunk { stream, chunk }).is_err() {
            return Ok(());
        }
    }
}
