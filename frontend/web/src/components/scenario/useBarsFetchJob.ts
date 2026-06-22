import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  createCliJob,
  getCliJob,
  getCliJobOutput,
  isTerminalCliJobStatus,
  type CliJob,
} from "@/api/cli";

type QueryKey = readonly unknown[];

export type BarsFetchSpec = {
  asset: string;
  granularity: string;
  from: string;
  to: string;
  invalidateQueryKeys: QueryKey[];
};

export function useBarsFetchJob(spec: BarsFetchSpec | null) {
  const qc = useQueryClient();
  const [jobId, setJobId] = useState<string | null>(null);
  const invalidatedJobIds = useRef<Set<string>>(new Set());

  const argv = useMemo(() => {
    if (!spec) return null;
    return buildBarsFetchArgv(spec);
  }, [spec]);

  const create = useMutation({
    mutationFn: async () => {
      if (!argv) throw new Error("bars fetch is not available for this scenario");
      return createCliJob({ argv });
    },
    onSuccess: (job) => {
      setJobId(job.job_id);
    },
  });

  const job = useQuery({
    queryKey: ["cli-job", jobId],
    queryFn: () => getCliJob(jobId ?? ""),
    enabled: !!jobId,
    refetchInterval: (query) => {
      const data = query.state.data as CliJob | undefined;
      return isTerminalCliJobStatus(data?.status) ? false : 1_000;
    },
  });

  const terminal = isTerminalCliJobStatus(job.data?.status);
  const output = useQuery({
    queryKey: ["cli-job-output", jobId],
    queryFn: () => getCliJobOutput(jobId ?? ""),
    enabled: !!jobId && terminal,
    retry: false,
  });

  useEffect(() => {
    if (!jobId || !terminal || invalidatedJobIds.current.has(jobId)) return;
    invalidatedJobIds.current.add(jobId);
    for (const queryKey of spec?.invalidateQueryKeys ?? []) {
      qc.invalidateQueries({ queryKey });
    }
  }, [jobId, qc, spec?.invalidateQueryKeys, terminal]);

  const outputText = output.data
    ? [output.data.stdout.trim(), output.data.stderr.trim()]
        .filter(Boolean)
        .join("\n")
    : null;
  const errorText =
    create.error instanceof Error
      ? create.error.message
      : job.data?.error_message ?? null;

  return {
    start: () => create.mutate(),
    canStart: !!argv && !create.isPending && !isRunning(job.data?.status),
    isActive: create.isPending || isRunning(job.data?.status),
    statusText: statusLabel(job.data?.status, create.isPending),
    outputText,
    errorText,
    job: job.data,
  };
}

function buildBarsFetchArgv(
  spec: Omit<BarsFetchSpec, "invalidateQueryKeys">,
) {
  return [
    "bars",
    "fetch",
    "--asset",
    spec.asset,
    "--granularity",
    spec.granularity,
    "--from",
    spec.from,
    "--to",
    spec.to,
  ];
}

export function scenarioGranularityToCli(granularity: string) {
  switch (granularity) {
    // Legacy backend enum strings (PascalCase). The backend's BarGranularity
    // Serialize impl now emits canonical CLI form ("1h", "4h", …) but older
    // rows in the DB were stored/serialised in PascalCase and must still be
    // handled so the "Indicator timeframe" menu always has a matching
    // option. Without this guard the picker renders with no selected label
    // (bead xvision-o24j).
    case "Minute1":
      return "1m";
    case "Minute5":
      return "5m";
    case "Minute15":
      return "15m";
    case "Hour1":
      return "1h";
    case "Hour4":
      return "4h";
    case "Hour6":
      return "6h";
    case "Day1":
      return "1d";
    case "Week1":
      return "1w";
    default:
      // Canonical CLI form ("1m", "5m", "15m", "1h", "4h", "6h", "1d",
      // "1w") passes through unchanged — the backend already emits these for
      // new rows.
      return granularity;
  }
}

function isRunning(status: string | undefined) {
  return status === "queued" || status === "running";
}

function statusLabel(status: string | undefined, creating: boolean) {
  if (creating) return "Queueing bars fetch...";
  switch (status) {
    case "queued":
      return "Bars fetch queued...";
    case "running":
      return "Fetching bars...";
    case "succeeded":
      return "Bars fetch completed.";
    case "failed":
      return "Bars fetch failed.";
    case "timed_out":
      return "Bars fetch timed out.";
    case "cancelled":
      return "Bars fetch cancelled.";
    default:
      return null;
  }
}
