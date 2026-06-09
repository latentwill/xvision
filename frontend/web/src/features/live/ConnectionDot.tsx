// SSE connection-health dot for a strategy pill (spec §2.4).
//
//   green  — streaming (healthy live feed)
//   amber  — reconnecting | snapshot (transient / initial)
//   red    — closed (terminal / dropped)
//
// The cockpit owns ONE real `useRunStream` (for the selected run's chart)
// and passes its `LiveStatus` to the selected pill. Non-selected pills get
// a lightweight derived status (live ⇒ snapshot/amber, terminal ⇒ closed)
// so we don't open an EventSource per pill.

import type { LiveStatus } from "@/components/chart/use-run-stream";

const TONE: Record<LiveStatus, { cls: string; label: string }> = {
  streaming: { cls: "bg-info", label: "Streaming" },
  reconnecting: { cls: "bg-warn", label: "Reconnecting" },
  snapshot: { cls: "bg-warn", label: "Connecting" },
  closed: { cls: "bg-danger", label: "Disconnected" },
};

export function ConnectionDot({ status }: { status: LiveStatus }) {
  const t = TONE[status];
  return (
    <span
      className={`inline-block h-2 w-2 shrink-0 rounded-full ${t.cls}`}
      title={t.label}
      aria-label={`Connection: ${t.label}`}
      role="img"
    />
  );
}
