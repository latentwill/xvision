import { apiFetch } from "./client";

export type CliJobStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "timed_out"
  | "cancelled";

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

export function isTerminalCliJobStatus(status: CliJobStatus | undefined) {
  return (
    status === "succeeded" ||
    status === "failed" ||
    status === "timed_out" ||
    status === "cancelled"
  );
}
