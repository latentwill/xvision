import { useEffect, useState, type ReactNode } from "react";
import { Pill } from "@/components/primitives/Pill";
import {
  usePauseCycle,
  useResumeCycle,
  useCancelCycle,
  type SessionSummary,
} from "../api";
import type { Activity, ActivitySource } from "../selectors/deriveActivity";
import { formatElapsed } from "../utils/time";

/** Re-render once per second so the elapsed label ticks while a run is live. */
function useElapsed(startedAtMs: number | null): string | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (startedAtMs == null) return;
    const id = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(id);
  }, [startedAtMs]);
  if (startedAtMs == null) return null;
  return formatElapsed(now - startedAtMs);
}

const TONE: Record<
  Exclude<Activity, "idle">,
  { pill: "gold" | "warn" | "danger"; border: string; bg: string; label: string }
> = {
  running: { pill: "gold", border: "border-gold/30", bg: "bg-gold/[0.05]", label: "Running" },
  paused: { pill: "warn", border: "border-warn/30", bg: "bg-warn/[0.06]", label: "Paused" },
  cancelling: {
    pill: "danger",
    border: "border-danger/30",
    bg: "bg-danger/[0.06]",
    label: "Cancelling",
  },
};

function Metric({
  value,
  label,
  tone = "text-text-2",
}: {
  value: ReactNode;
  label?: string;
  tone?: string;
}) {
  return (
    <span className="font-mono text-[12px] whitespace-nowrap">
      <span className={tone}>{value}</span>
      {label ? (
        <>
          {" "}
          <span className="text-text-3">{label}</span>
        </>
      ) : null}
    </span>
  );
}

/**
 * The prominent, unmissable live-run indicator. Rendered above the console
 * whenever the optimizer is active — the answer to "is something running right
 * now" before the operator reads anything else.
 *
 * It is a status display, not a control surface: Pause/Resume/Cancel live in the
 * headline action slot (and only exist for a controllable `status`-backed run).
 * An inferred run (`source === "events"` — e.g. a CLI run with no IPC bridge)
 * still announces itself here, honestly omitting counters it can't know.
 */
export function RunStatusBar({
  activity,
  source,
  cycleId,
  session,
  connected,
  startedAtMs,
}: {
  activity: Activity;
  source: ActivitySource;
  cycleId: string | null;
  session: SessionSummary | null;
  connected: boolean;
  startedAtMs: number | null;
}) {
  const elapsed = useElapsed(activity === "running" ? startedAtMs : null);
  const pauseMutation = usePauseCycle();
  const resumeMutation = useResumeCycle();
  const cancelMutation = useCancelCycle();
  if (activity === "idle") return null;

  const tone = TONE[activity];
  const cycleNo = session ? session.cycles_completed + 1 : null;
  const controllable = session != null && cycleId != null;

  return (
    <div
      role="status"
      aria-live="polite"
      aria-label={`Optimizer ${tone.label.toLowerCase()}`}
      className={`flex flex-wrap items-center gap-x-4 gap-y-2 rounded-md border ${tone.border} ${tone.bg} px-4 py-2.5`}
    >
      <Pill animated={activity === "running"} tone={tone.pill}>
        <span aria-hidden>●</span>
        <span className="font-semibold tracking-widest">{tone.label.toUpperCase()}</span>
      </Pill>

      {cycleId && (
        <span className="font-mono text-[12px] text-text-3">
          cycle{" "}
          <span className="text-text-2 select-all">{cycleId.slice(0, 8)}</span>
        </span>
      )}

      {cycleNo != null && <Metric value={`cycle #${cycleNo}`} />}
      {session && <Metric value={session.kept_count} label="kept" tone="text-gold" />}
      {session && session.suspect_count > 0 && (
        <Metric value={session.suspect_count} label="suspect" tone="text-warn" />
      )}
      {elapsed && <Metric value={elapsed} label="elapsed" />}

      {/* Control buttons — only for controllable status-backed runs */}
      {controllable && activity === "running" && (
        <span className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => pauseMutation.mutate(cycleId!)}
            disabled={pauseMutation.isPending}
            className="rounded border border-border px-2.5 py-1 text-[12px] text-text-2 hover:bg-surface-elev/40 transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Pause
          </button>
          <button
            type="button"
            onClick={() => cancelMutation.mutate(cycleId!)}
            disabled={cancelMutation.isPending}
            className="rounded border border-danger/40 px-2.5 py-1 text-[12px] text-danger hover:bg-danger/[0.06] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Cancel
          </button>
        </span>
      )}
      {controllable && activity === "paused" && (
        <span className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => resumeMutation.mutate(cycleId!)}
            disabled={resumeMutation.isPending}
            className="rounded bg-accent px-2.5 py-1 text-[12px] font-medium text-on-accent hover:opacity-90 transition-opacity disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Resume
          </button>
          <button
            type="button"
            onClick={() => cancelMutation.mutate(cycleId!)}
            disabled={cancelMutation.isPending}
            className="rounded border border-danger/40 px-2.5 py-1 text-[12px] text-danger hover:bg-danger/[0.06] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Cancel
          </button>
        </span>
      )}

      <span
        className="ml-auto inline-flex items-center gap-1.5 font-mono text-[11px] text-text-3"
        title={
          connected
            ? "Live event stream connected"
            : source === "events"
              ? "No live stream — reading the run from the database"
              : "Live stream offline — polling the database"
        }
      >
        <span
          className={`h-1.5 w-1.5 rounded-full ${
            connected ? "bg-gold animate-pulse" : "bg-text-4"
          }`}
          aria-hidden
        />
        {connected ? "live" : "polling"}
      </span>
    </div>
  );
}
