import { useCallback, useEffect, useState } from "react";
import { type CycleProgressEvent } from "../api";

const SSE_EVENT_NAMES = [
  "cycle_started",
  "parent_selected",
  "mutation_proposed",
  "no_candidate",
  "mutation_gated",
  "honesty_check_run",
  "judge_finding",
  "cycle_finished",
  "lagged",
] as const;

export type EventRow = CycleProgressEvent & { _row_id: number };

let nextRowId = 1;

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

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function parseSsePayload(raw: unknown, fallbackKind: string): CycleProgressEvent | null {
  if (typeof raw !== "string" || raw.trim() === "") return null;
  const parsed = parseJsonObject(raw);
  if (!parsed) return null;
  if ("dropped" in parsed) return null;

  const data = isRecord(parsed.data) ? parsed.data : parsed;
  const kind =
    stringValue(data.event_type) ??
    stringValue(data.type) ??
    stringValue(parsed.kind) ??
    fallbackKind;
  if (kind === "message") return null;
  return {
    ...data,
    event_type: kind,
    kind,
    display_label: stringValue(parsed.display_label) ?? stringValue(data.display_label),
    ts: stringValue(data.ts) ?? new Date().toISOString(),
    cycle_id: stringValue(data.cycle_id),
    bundle_hash: stringValue(data.bundle_hash),
    parent_hash: stringValue(data.parent_hash),
    child_hash: stringValue(data.child_hash),
  };
}

export function useCycleEventStream(): {
  events: EventRow[];
  connected: boolean;
  isRunning: boolean;
  activeCycleId: string | null;
} {
  const [events, setEvents] = useState<EventRow[]>([]);
  const [connected, setConnected] = useState(false);

  const appendEvent = useCallback((event: CycleProgressEvent) => {
    setEvents((prev) => {
      const row: EventRow = { ...event, _row_id: nextRowId++ };
      const next = prev.length >= 200 ? prev.slice(1) : prev;
      return [...next, row];
    });
  }, []);

  useEffect(() => {
    const source = new EventSource("/api/autooptimizer/events");
    const handleMessage = (ev: Event) => {
      const event = parseSsePayload((ev as MessageEvent).data, ev.type);
      if (event) appendEvent(event);
    };
    source.addEventListener("open", () => { setConnected(true); });
    source.addEventListener("message", handleMessage);
    for (const name of SSE_EVENT_NAMES) source.addEventListener(name, handleMessage);
    source.addEventListener("error", () => { setConnected(false); });
    return () => {
      source.removeEventListener("message", handleMessage);
      for (const eventName of SSE_EVENT_NAMES) {
        source.removeEventListener(eventName, handleMessage);
      }
      source.close();
      setConnected(false);
    };
  }, [appendEvent]);

  // Derive running state from the event buffer — single source of truth for
  // both the heatmap and command bar so they don't diverge.
  let isRunning = false;
  let activeCycleId: string | null = null;
  for (let i = events.length - 1; i >= 0; i--) {
    const et = events[i].event_type ?? events[i].type ?? events[i].kind ?? "";
    if (et === "cycle_finished") { isRunning = false; activeCycleId = null; break; }
    if (et === "cycle_started") { isRunning = true; activeCycleId = events[i].cycle_id ?? null; break; }
  }

  return { events, connected, isRunning, activeCycleId };
}
