/**
 * CliJobStatusBadge — inline status chip for CLI job rows and detail views.
 *
 * Covers the v2b-remote-cli-job-safety hardening statuses:
 *   - orphaned: job was running when the dashboard restarted; process not found
 *   - output_cap_exceeded: job was killed because it produced too much output
 *   - runtime_cap_exceeded: job was killed because it ran past the runtime cap
 *
 * No popups — per CLAUDE.md. Status is surfaced inline.
 */
import type { JSX } from "react";
import type { CliJobStatus } from "../../api/cli";

interface CliJobStatusBadgeProps {
  status: CliJobStatus;
}

interface BadgeStyle {
  label: string;
  className: string;
}

const STATUS_STYLES: Record<CliJobStatus, BadgeStyle> = {
  queued: {
    label: "Queued",
    className:
      "bg-muted text-muted-foreground border border-border",
  },
  running: {
    label: "Running",
    className:
      "bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/30",
  },
  succeeded: {
    label: "Succeeded",
    className:
      "bg-green-500/10 text-green-700 dark:text-green-400 border border-green-500/30",
  },
  failed: {
    label: "Failed",
    className:
      "bg-red-500/10 text-red-700 dark:text-red-400 border border-red-500/30",
  },
  timed_out: {
    label: "Timed Out",
    className:
      "bg-orange-500/10 text-orange-700 dark:text-orange-400 border border-orange-500/30",
  },
  cancelled: {
    label: "Cancelled",
    className:
      "bg-muted text-muted-foreground border border-border",
  },
  orphaned: {
    label: "Recovered",
    className:
      "bg-yellow-500/10 text-yellow-700 dark:text-yellow-400 border border-yellow-500/30",
  },
  output_cap_exceeded: {
    label: "Output Cap",
    className:
      "bg-orange-500/10 text-orange-700 dark:text-orange-400 border border-orange-500/30",
  },
  runtime_cap_exceeded: {
    label: "Runtime Cap",
    className:
      "bg-orange-500/10 text-orange-700 dark:text-orange-400 border border-orange-500/30",
  },
};

export function CliJobStatusBadge({ status }: CliJobStatusBadgeProps): JSX.Element {
  const style = STATUS_STYLES[status] ?? {
    label: status,
    className: "bg-muted text-muted-foreground border border-border",
  };

  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${style.className}`}
    >
      {style.label}
    </span>
  );
}
