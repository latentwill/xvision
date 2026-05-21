import { apiFetch } from "./client";

export type CliJobStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "timed_out"
  | "cancelled"
  | "orphaned"
  | "output_cap_exceeded"
  | "runtime_cap_exceeded";

export type CreateCliJobRequest = {
  argv: string[];
  timeout_secs?: number;
};

export type CreateCliJobResponse = {
  job_id: string;
  status: CliJobStatus;
};

export type CliJob = {
  job_id: string;
  argv: string[];
  status: CliJobStatus;
  created_at: string;
  started_at: string | null;
  finished_at: string | null;
  exit_code: number | null;
  timed_out: boolean;
  cancel_requested: boolean;
  stdout_bytes: number;
  stderr_bytes: number;
  stdout_truncated: boolean;
  stderr_truncated: boolean;
  error_message: string | null;
  // Audit fields (v2b-remote-cli-job-safety)
  pid: number | null;
  job_user: string | null;
  job_source: string | null;
  command_class: string | null;
  cancelled_at: string | null;
  cancel_signal: string | null;
  recovered_at: string | null;
  recovery_reason: string | null;
  max_runtime_seconds: number;
  max_output_bytes: number;
  output_cap_exceeded: boolean;
  runtime_cap_exceeded: boolean;
  output_bytes: number;
};

export type CliJobOutput = {
  job_id: string;
  status: CliJobStatus;
  exit_code: number | null;
  stdout: string;
  stderr: string;
  stdout_bytes: number;
  stderr_bytes: number;
  stdout_truncated: boolean;
  stderr_truncated: boolean;
};

export function createCliJob(
  body: CreateCliJobRequest,
): Promise<CreateCliJobResponse> {
  return apiFetch<CreateCliJobResponse>("/api/cli/jobs", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export function getCliJob(jobId: string): Promise<CliJob> {
  return apiFetch<CliJob>(`/api/cli/jobs/${encodeURIComponent(jobId)}`);
}

export function getCliJobOutput(jobId: string): Promise<CliJobOutput> {
  return apiFetch<CliJobOutput>(
    `/api/cli/jobs/${encodeURIComponent(jobId)}/output`,
  );
}

export function cancelCliJob(jobId: string): Promise<CliJob> {
  return apiFetch<CliJob>(`/api/cli/jobs/${encodeURIComponent(jobId)}`, {
    method: "DELETE",
  });
}

export function isTerminalCliJobStatus(status: CliJobStatus | undefined) {
  return (
    status === "succeeded" ||
    status === "failed" ||
    status === "timed_out" ||
    status === "cancelled" ||
    status === "orphaned" ||
    status === "output_cap_exceeded" ||
    status === "runtime_cap_exceeded"
  );
}

/** Returns true when the job ended abnormally (killed, orphaned, cap-breached). */
export function isAbnormalCliJobStatus(status: CliJobStatus | undefined) {
  return (
    status === "failed" ||
    status === "timed_out" ||
    status === "cancelled" ||
    status === "orphaned" ||
    status === "output_cap_exceeded" ||
    status === "runtime_cap_exceeded"
  );
}
