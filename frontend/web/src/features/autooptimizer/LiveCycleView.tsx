// LiveCycleView — subscribes to GET /api/autooptimizer/events (SSE) and
// renders incoming CycleProgressEvent items with operator-facing labels.
//
// Operator labels follow the two-name convention from the terminology lock:
// event_type values stay developer-internal; display_label or the local map
// produces the plain-language label surfaced to the operator.

import { useEffect, useRef, useState } from "react";
import { Card, CardHeader } from "@/components/primitives/Card";
import { type CycleProgressEvent, formatEventLabel } from "./api";

type EventRow = CycleProgressEvent & { _row_id: number };

let nextRowId = 1;

export function LiveCycleView() {
  const [events, setEvents] = useState<EventRow[]>([]);
  const [connected, setConnected] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const source = new EventSource("/api/autooptimizer/events");

    source.addEventListener("open", () => {
      setConnected(true);
    });

    // The stream sends `data: <json>\n\n` lines without an event name,
    // so we listen on the default `message` event.
    source.addEventListener("message", (ev) => {
      let parsed: CycleProgressEvent | null = null;
      try {
        parsed = JSON.parse(ev.data as string) as CycleProgressEvent;
      } catch {
        return;
      }
      if (!parsed) return;
      setEvents((prev) => {
        const row: EventRow = { ...parsed!, _row_id: nextRowId++ };
        // Keep at most 200 events; older ones are dropped from the top.
        const next = prev.length >= 200 ? prev.slice(1) : prev;
        return [...next, row];
      });
    });

    source.addEventListener("error", () => {
      setConnected(false);
    });

    return () => {
      source.close();
      setConnected(false);
    };
  }, []);

  // Auto-scroll to the newest event.
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [events.length]);

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <span
          className={[
            "inline-block w-2 h-2 rounded-full",
            connected ? "bg-green-500" : "bg-text-3",
          ].join(" ")}
          aria-label={connected ? "Connected" : "Disconnected"}
        />
        <span className="text-[13px] text-text-2">
          {connected ? "Live" : "Waiting for connection…"}
        </span>
      </div>

      <Card>
        <CardHeader title="Cycle events" />

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
