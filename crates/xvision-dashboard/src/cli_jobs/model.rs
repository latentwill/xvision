use serde::{Deserialize, Serialize};

/// Default per-job supervisor cap: 1 hour. Distinct from `timeout_secs` which
/// comes from the caller's `CreateCliJobReq`. `max_runtime_seconds` is the
/// hard dashboard-side limit on the child process regardless of what the
/// caller requested.
pub const DEFAULT_MAX_RUNTIME_SECONDS: u64 = 3600;

/// Default per-job output cap: 10 MB (combined stdout + stderr bytes).
/// When the child process writes more than this, the output is truncated,
/// the process is killed, and `output_cap_exceeded` is set.
pub const DEFAULT_MAX_OUTPUT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CliJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    /// Set by the startup orphan-recovery sweep when a job's recorded PID is
    /// no longer alive after a dashboard restart. This is distinct from
    /// `Failed` so operators can distinguish "job ran but failed" from "job
    /// lost due to a dashboard crash".
    Orphaned,
    /// Set when the dashboard's output-cap supervisor terminates the child
    /// because it wrote more than `max_output_bytes`.
    OutputCapExceeded,
    /// Set when the dashboard's runtime-cap supervisor terminates the child
    /// because it ran longer than `max_runtime_seconds`.
    RuntimeCapExceeded,
}

impl CliJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::Orphaned => "orphaned",
            Self::OutputCapExceeded => "output_cap_exceeded",
            Self::RuntimeCapExceeded => "runtime_cap_exceeded",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "timed_out" => Some(Self::TimedOut),
            "cancelled" => Some(Self::Cancelled),
            "orphaned" => Some(Self::Orphaned),
            "output_cap_exceeded" => Some(Self::OutputCapExceeded),
            "runtime_cap_exceeded" => Some(Self::RuntimeCapExceeded),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::TimedOut
                | Self::Cancelled
                | Self::Orphaned
                | Self::OutputCapExceeded
                | Self::RuntimeCapExceeded
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CliJob {
    pub job_id: String,
    pub argv: Vec<String>,
    pub status: CliJobStatus,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i64>,
    pub timeout_secs: u64,
    pub timed_out: bool,
    pub cancel_requested: bool,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub error_message: Option<String>,
    // --- Audit fields (migration 028) ---
    /// OS PID of the child process while running. `None` for jobs that were
    /// queued but not yet started, or for synthetic eval-run bridge IDs.
    pub pid: Option<i64>,
    /// User identifier from AuthContext (e.g. Tailscale node name, "localhost",
    /// or "unknown:dashboard" for jobs submitted without identity info).
    pub job_user: Option<String>,
    /// Source of the job request: `"tailscale:<node>"`, `"localhost"`, or
    /// `"unknown"`.
    pub job_source: Option<String>,
    /// The `xvn` verb that was executed (first argv element), e.g. `"eval"`,
    /// `"bars"`. Used for audit logs and dashboard filtering.
    pub command_class: Option<String>,
    /// When the SIGTERM was sent during cancellation.
    pub cancelled_at: Option<String>,
    /// Which signal terminated the process: `"SIGTERM"` or `"SIGKILL"`.
    pub cancel_signal: Option<String>,
    /// When the orphan-recovery sweep discovered this job was lost.
    pub recovered_at: Option<String>,
    /// Human-readable reason for orphan recovery (e.g. `"process_not_found"`).
    pub recovery_reason: Option<String>,
    /// Dashboard-layer runtime cap (seconds). 0 = use server default.
    pub max_runtime_seconds: u64,
    /// Dashboard-layer output cap (bytes, combined stdout+stderr). 0 = use server default.
    pub max_output_bytes: u64,
    /// True when the output cap was breached and the process was killed.
    pub output_cap_exceeded: bool,
    /// True when the runtime cap was breached and the process was killed.
    pub runtime_cap_exceeded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliJobOutput {
    pub job_id: String,
    pub status: CliJobStatus,
    pub exit_code: Option<i64>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}
