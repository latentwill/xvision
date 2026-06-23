import { useQuery } from "@tanstack/react-query";
import { Pill } from "@/components/primitives/Pill";
import { getHealth, healthKeys } from "@/api/health";
import type { HealthStatus } from "@/api/types.gen";

const TONE_FOR: Record<HealthStatus, "gold" | "warn" | "danger"> = {
  ok: "gold",
  degraded: "warn",
  down: "danger",
};

const DOT_FOR: Record<HealthStatus, string> = {
  ok: "bg-gold",
  degraded: "bg-warn",
  down: "bg-danger",
};

const LABEL_FOR: Record<HealthStatus, string> = {
  ok: "engine ok",
  degraded: "degraded",
  down: "engine down",
};

export function HealthPill() {
  // The pill polls /api/health every 15s — frequent enough to catch a stopped
  // alpaca / llm probe in the same minute, infrequent enough to be ignorable
  // server-side. Server caches nothing yet; revisit if it ever does.
  const q = useQuery({
    queryKey: healthKeys.report(),
    queryFn: getHealth,
    refetchInterval: 15_000,
    refetchOnWindowFocus: true,
  });

  if (q.isPending) {
    return (
      <Pill>
        <span className="w-1.5 h-1.5 rounded-full bg-text-3" />
        checking…
      </Pill>
    );
  }

  if (q.isError || !q.data) {
    return (
      <Pill tone="danger" title="dashboard couldn't reach engine">
        <span className="w-1.5 h-1.5 rounded-full bg-danger" />
        offline
      </Pill>
    );
  }

  const { status, probes = [] } = q.data;
  // Compose a hover popover summary. Native `title=` is enough for v1; richer
  // popovers come with the chat-rail / command-palette plans.
  const summary = probes
    .map((p) => {
      const detail = p.detail ? ` (${p.detail})` : "";
      const ms = p.latency_ms != null ? ` ${p.latency_ms}ms` : "";
      return `${p.name}: ${p.status}${detail}${ms}`;
    })
    .join("\n");

  return (
    <Pill tone={TONE_FOR[status]} title={summary}>
      <span className={`w-1.5 h-1.5 rounded-full ${DOT_FOR[status]}`} />
      {LABEL_FOR[status]}
    </Pill>
  );
}
