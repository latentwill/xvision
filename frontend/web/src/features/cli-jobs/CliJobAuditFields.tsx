/**
 * CliJobAuditFields — inline display of v2b-remote-cli-job-safety audit fields.
 *
 * Renders: user, source, command_class, output_bytes, cap status.
 * Used in the job detail view. No popups; everything is inline text.
 *
 * Recovery state (orphaned) shows the recovered_at timestamp and
 * recovery_reason inline — same row as the status badge.
 */
import type { JSX } from "react";
import type { CliJob } from "../../api/cli";

interface CliJobAuditFieldsProps {
  job: CliJob;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function CliJobAuditFields({ job }: CliJobAuditFieldsProps): JSX.Element {
  const rows: { label: string; value: string | JSX.Element | null }[] = [
    {
      label: "User",
      value: job.job_user ?? "—",
    },
    {
      label: "Source",
      value: job.job_source ?? "—",
    },
    {
      label: "Command",
      value: job.command_class ?? "—",
    },
    {
      label: "Output",
      value: job.output_bytes != null
        ? `${formatBytes(job.output_bytes)}${job.output_cap_exceeded ? " (cap exceeded)" : ""}${job.stdout_truncated || job.stderr_truncated ? " (truncated)" : ""}`
        : "—",
    },
  ];

  if (job.status === "orphaned" && job.recovery_reason) {
    rows.push({
      label: "Recovered",
      value: job.recovery_reason === "process_not_found"
        ? "Process not found after restart"
        : job.recovery_reason,
    });
  }

  if (job.cancelled_at) {
    rows.push({
      label: "Cancelled",
      value: `${new Date(job.cancelled_at).toLocaleString()}${job.cancel_signal ? ` via ${job.cancel_signal}` : ""}`,
    });
  }

  if (job.runtime_cap_exceeded) {
    rows.push({
      label: "Runtime cap",
      value: `${job.max_runtime_seconds}s`,
    });
  }

  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
      {rows.map(({ label, value }) =>
        value == null ? null : (
          <>
            <dt key={`${label}-dt`} className="text-muted-foreground whitespace-nowrap">
              {label}
            </dt>
            <dd key={`${label}-dd`} className="text-foreground">
              {value}
            </dd>
          </>
        )
      )}
    </dl>
  );
}
