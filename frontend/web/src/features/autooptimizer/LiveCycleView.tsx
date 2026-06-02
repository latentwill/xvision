// LiveCycleView — subscribes to GET /api/autooptimizer/events (SSE) and
// renders incoming CycleProgressEvent items with operator-facing labels.
//
// Operator labels follow the two-name convention from the terminology lock:
// event_type values stay developer-internal; display_label or the local map
// produces the plain-language label surfaced to the operator.

import { useEffect, useRef, useState } from "react";
import { Card, CardHeader } from "@/components/primitives/Card";
import { createCliJob } from "@/api/cli";
import {
  type CycleProgressEvent,
  formatEventLabel,
  normalizeCycleProgressEnvelope,
} from "./api";

type EventRow = CycleProgressEvent & {
  _row_id: number;
  event_type: string;
  ts: string;
};
type NormalizedEvent = Omit<EventRow, "_row_id">;

let nextRowId = 1;

const CYCLE_EVENT_NAMES = [
  "cycle_started",
  "parent_selected",
  "mutation_proposed",
  "mutation_gated",
  "mutation_gated_passed",
  "mutation_gated_dropped",
  "honesty_check_run",
  "judge_finding",
  "cycle_finished",
  "cycle_sealed",
] as const;

type LaunchStripProps = {
  onEvent: (event: CycleProgressEvent) => void;
};

function LaunchStrip({ onEvent }: LaunchStripProps) {
  const [strategyId, setStrategyId] = useState("");
  const [budget, setBudget] = useState("");
  const [useMock, setUseMock] = useState(true);
  const [isRunning, setIsRunning] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);

  const handleLaunch = async () => {
    const trimmedStrategyId = strategyId.trim();
    if (!trimmedStrategyId) {
      setLaunchError("Strategy ID is required");
      return;
    }
    setIsRunning(true);
    setLaunchError(null);
    try {
      const argv = buildEveningCycleArgv(trimmedStrategyId, budget, useMock);
      const job = await createCliJob({
        argv,
        timeout_secs: 3600,
      });
      setJobId(job.job_id);
      onEvent({
        event_type: "job_started",
        display_label: "Optimizer job queued",
        cycle_id: argv[argv.indexOf("--session-id") + 1],
        ts: new Date().toISOString(),
      });
      followCliJob(job.job_id, onEvent, setLaunchError, setIsRunning);
    } catch (e) {
      setLaunchError(e instanceof Error ? e.message : "Network error");
      setIsRunning(false);
    }
  };

  const inp =
    "bg-surface border border-border rounded text-text text-[13px] px-2 py-1";

  return (
    <div className="flex items-center gap-3 flex-wrap">
      <input
        type="text"
        placeholder="Strategy ID"
        value={strategyId}
        onChange={(e) => setStrategyId(e.target.value)}
        disabled={isRunning}
        className={`${inp} w-[180px]`}
      />
      <input
        type="number"
        placeholder="5.00"
        value={budget}
        onChange={(e) => setBudget(e.target.value)}
        disabled={isRunning}
        step="0.01"
        min="0"
        className={`${inp} w-[80px]`}
      />
      <label className="inline-flex items-center gap-2 text-[13px] text-text-2">
        <input
          type="checkbox"
          checked={useMock}
          onChange={(e) => setUseMock(e.target.checked)}
          disabled={isRunning}
          className="h-4 w-4"
        />
        Mock
      </label>
      <button
        type="button"
        onClick={() => {
          void handleLaunch();
        }}
        disabled={isRunning}
        className="rounded bg-accent px-3 py-1 text-[13px] font-medium text-on-accent hover:opacity-90 disabled:opacity-50"
      >
        {isRunning ? "Running…" : "Start evening run"}
      </button>
      {launchError !== null && (
        <span className="text-[13px] text-danger">{launchError}</span>
      )}
      {jobId !== null && launchError === null && (
        <span className="text-[12px] text-text-3 font-mono">job {jobId}</span>
      )}
    </div>
  );
}

export function LiveCycleView() {
  const [events, setEvents] = useState<EventRow[]>([]);
  const [connected, setConnected] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  const appendEvent = (event: CycleProgressEvent) => {
    const normalized = normalizeCycleEvent(event);
    if (!normalized) return;
    setEvents((prev) => {
      const row: EventRow = { ...normalized, _row_id: nextRowId++ };
      const next = prev.length >= 200 ? prev.slice(1) : prev;
      return [...next, row];
    });
  };

  useEffect(() => {
    const source = new EventSource("/api/autooptimizer/events");
    const handleMessage = (ev: Event) => {
      const event = parseSsePayload((ev as MessageEvent).data, ev.type);
      if (event) appendEvent(event);
    };

    source.addEventListener("open", () => {
      setConnected(true);
    });

    source.addEventListener("message", handleMessage);
    for (const name of CYCLE_EVENT_NAMES) {
      source.addEventListener(name, handleMessage);
    }

    source.addEventListener("error", () => {
      setConnected(false);
    });

    return () => {
      source.removeEventListener("message", handleMessage);
      for (const name of CYCLE_EVENT_NAMES) {
        source.removeEventListener(name, handleMessage);
      }
      source.close();
      setConnected(false);
    };
  }, []);

  // Auto-scroll to the newest event.
  useEffect(() => {
    bottomRef.current?.scrollIntoView?.({ behavior: "smooth" });
  }, [events.length]);

  return (
    <div className="space-y-4">
      <LaunchStrip onEvent={appendEvent} />

      <div className="grid gap-3 md:grid-cols-3">
        <LiveStat
          label="Stream"
          value={connected ? "Live" : "Idle"}
          accent={connected ? "good" : "muted"}
        />
        <LiveStat label="Events" value={String(events.length)} />
        <LiveStat
          label="Latest cycle"
          value={[...events].reverse().find((event) => event.cycle_id)?.cycle_id ?? "—"}
          mono
        />
      </div>

      <Card>
        <CardHeader title="Evening run stream" />

        {events.length === 0 ? (
          <div className="px-5 pb-5 pt-2 text-[13px] text-text-3">
            Waiting for cycle…
          </div>
        ) : (
          <div
            className="overflow-y-auto max-h-[520px] pb-4"
            role="log"
            aria-live="polite"
            aria-label="Cycle event feed"
          >
            <table className="w-full text-[13px] border-collapse">
              <thead>
                <tr className="sticky top-0 bg-surface-card border-b border-border">
                  <th className="text-left font-medium text-text-3 px-5 py-2 w-[180px]">
                    Time
                  </th>
                  <th className="text-left font-medium text-text-3 px-5 py-2">
                    Event
                  </th>
                  <th className="text-left font-medium text-text-3 px-5 py-2 w-[200px]">
                    Cycle
                  </th>
                </tr>
              </thead>
              <tbody>
                {events.map((ev) => (
                  <tr
                    key={ev._row_id}
                    className="border-b border-border last:border-0 hover:bg-surface-elev/40"
                  >
                    <td className="px-5 py-2 text-text-3 font-mono text-[12px] whitespace-nowrap">
                      {formatEventTime(ev.ts)}
                    </td>
                    <td className="px-5 py-2 text-text">
                      {formatEventLabel(ev)}
                    </td>
                    <td className="px-5 py-2 text-text-3 font-mono text-[11px] truncate max-w-[200px]">
                      {ev.cycle_id ?? "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div ref={bottomRef} />
          </div>
        )}
      </Card>
    </div>
  );
}

function formatEventTime(ts: string): string {
  try {
    const d = new Date(ts);
    return d.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return ts;
  }
}

function LiveStat({
  label,
  value,
  accent = "muted",
  mono = false,
}: {
  label: string;
  value: string;
  accent?: "good" | "muted";
  mono?: boolean;
}) {
  const dot = accent === "good" ? "bg-green-500" : "bg-text-3";
  return (
    <div className="rounded border border-border bg-surface-card px-4 py-3">
      <div className="flex items-center gap-2 text-[12px] text-text-3">
        <span className={`inline-block h-2 w-2 rounded-full ${dot}`} />
        {label}
      </div>
      <div
        className={`mt-1 truncate text-[15px] text-text ${
          mono ? "font-mono text-[12px]" : "font-medium"
        }`}
      >
        {value}
      </div>
    </div>
  );
}

function buildEveningCycleArgv(strategyId: string, budget: string, useMock: boolean): string[] {
  const sessionId = `ui-${Date.now().toString(36)}-${randomSuffix()}`;
  const argv = ["optimizer", "evening-cycle", "--session-id", sessionId];
  if (useMock) argv.push("--mock");
  const cleanStrategy = strategyId.trim();
  if (cleanStrategy) argv.push("--strategy", cleanStrategy);
  const cleanBudget = budget.trim();
  if (cleanBudget) argv.push("--budget", cleanBudget);
  return argv;
}

function randomSuffix(): string {
  const bytes = new Uint8Array(4);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function followCliJob(
  jobId: string,
  onEvent: (event: CycleProgressEvent) => void,
  setLaunchError: (value: string | null) => void,
  setIsRunning: (value: boolean) => void,
) {
  const source = new EventSource(`/api/cli/jobs/${encodeURIComponent(jobId)}/events`);
  source.addEventListener("stdout_chunk", (ev) => {
    const data = parseJsonObject((ev as MessageEvent).data);
    const chunk = typeof data?.chunk === "string" ? data.chunk : "";
    for (const line of chunk.split(/\r?\n/)) {
      const event = parseSsePayload(line, "stdout_chunk");
      if (event) onEvent(event);
    }
  });
  source.addEventListener("stderr_chunk", (ev) => {
    const data = parseJsonObject((ev as MessageEvent).data);
    const chunk = typeof data?.chunk === "string" ? data.chunk.trim() : "";
    if (chunk) setLaunchError(chunk);
  });
  source.addEventListener("job_finished", (ev) => {
    const data = parseJsonObject((ev as MessageEvent).data);
    const status = typeof data?.status === "string" ? data.status : "finished";
    onEvent({
      event_type: "job_finished",
      display_label: status === "succeeded" ? "Optimizer job finished" : `Optimizer job ${status}`,
      ts: new Date().toISOString(),
    });
    if (status !== "succeeded") {
      setLaunchError(`Optimizer job ${status}`);
    }
    setIsRunning(false);
    source.close();
  });
  source.addEventListener("error", () => {
    setIsRunning(false);
    source.close();
  });
}

function parseSsePayload(raw: unknown, fallbackKind: string): CycleProgressEvent | null {
  if (typeof raw !== "string" || raw.trim() === "") return null;
  const normalized =
    normalizeCycleProgressEnvelope(raw) ??
    normalizeCycleProgressEnvelope({
      kind: fallbackKind,
      data: parseJsonObject(raw),
    });
  if (!normalized) return null;
  const kind = normalized.event_type ?? normalized.kind;
  if (kind === "stdout_chunk" || kind === "message") return null;
  return normalized;
}

function normalizeCycleEvent(event: CycleProgressEvent): NormalizedEvent | null {
  const eventType = event.event_type ?? event.type ?? event.kind;
  if (!eventType) return null;
  return {
    ...event,
    event_type: eventType,
    kind: event.kind ?? eventType,
    ts: event.ts ?? new Date().toISOString(),
  };
}

function parseJsonObject(raw: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(raw) as unknown;
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
