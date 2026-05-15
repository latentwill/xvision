use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, watch, Mutex};

use super::model::{CliJob, CliJobStatus};
use super::store::CliJobStore;

pub const DEFAULT_TIMEOUT_SECS: u64 = 300;
pub const MAX_TIMEOUT_SECS: u64 = 6 * 60 * 60;
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

        let mut child = Command::new(&self.cli_command)
            .args(&job.argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!(
                    "spawn '{}' for cli job '{}'",
                    self.cli_command.display(),
                    job.job_id
                )
            })?;

        store.mark_running(&job.job_id).await?;
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
        let mut cancelled = false;
        let mut error_message = None;
        let mut ticker = tokio::time::interval(Duration::from_millis(25));
        ticker.tick().await;
        let timeout_secs = job.timeout_secs.max(1);
        let timeout = tokio::time::sleep(Duration::from_secs(timeout_secs));
        tokio::pin!(timeout);

        while exit_status.is_none() || !stdout_closed || !stderr_closed {
            tokio::select! {
                _ = ticker.tick(), if exit_status.is_none() => {
                    if let Some(status) = child.try_wait().context("poll cli child")? {
                        exit_status = Some(status);
                    }
                }
                maybe_msg = rx.recv(), if !stdout_closed || !stderr_closed => {
                    match maybe_msg {
                        Some(StreamMessage::Chunk { stream, chunk }) => {
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
                        }
                        Some(StreamMessage::Closed(StreamKind::Stdout)) => stdout_closed = true,
                        Some(StreamMessage::Closed(StreamKind::Stderr)) => stderr_closed = true,
                        None => {
                            stdout_closed = true;
                            stderr_closed = true;
                        }
                    }
                }
                _ = &mut timeout, if exit_status.is_none() && !timed_out && !cancelled => {
                    timed_out = true;
                    error_message = Some(format!("job exceeded timeout of {timeout_secs}s"));
                    child.start_kill().context("kill timed out cli job")?;
                }
                changed = cancel_rx.changed(), if exit_status.is_none() && !timed_out && !cancelled => {
                    if changed.is_err() || !*cancel_rx.borrow() {
                        continue;
                    }
                    cancelled = true;
                    error_message = Some("job cancelled".into());
                    child.start_kill().context("kill cancelled cli job")?;
                }
            }
        }

        stdout_task.await.context("join stdout reader")??;
        stderr_task.await.context("join stderr reader")??;

        let status = if let Some(status) = exit_status {
            status
        } else {
            child.wait().await.context("wait for cli child exit")?
        };
        let exit_code = status.code().map(i64::from);

        let final_status = if timed_out {
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
            .finish(&job.job_id, final_status, exit_code, error_message.clone())
            .await?;
        self.emit_finished(
            &job.job_id,
            final_status,
            exit_code,
            timed_out,
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
