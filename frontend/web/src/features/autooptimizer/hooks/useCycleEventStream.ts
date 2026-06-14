import { useSyncExternalStore } from "react";
import { type CycleProgressEvent } from "../api";

const SSE_EVENT_NAMES = [
  "cycle_started",
  "parent_selected",
  "mutation_proposed",
  "no_candidate",
  "mutation_gated",
  // Three-way gate outcomes emitted by event_kind() in autooptimizer_labels.rs:
  "mutation_gated_passed",
  "mutation_gated_suspect",
  "mutation_gated_dropped",
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

// ─── Shared single-connection store ───────────────────────────────────────────
// Every consumer (home headline, console, launch panel, river, the live-activity
// hook) reads the SAME EventSource. Opening one connection per component would
// fan out to half a dozen SSE sockets on the optimizer page and let consumers
// derive divergent running state from independently-buffered events. The store
// keeps one buffer so every surface agrees, and refcounts the connection so it
// closes when the last consumer unmounts.

let events: EventRow[] = [];
let connected = false;
let source: EventSource | null = null;
let refCount = 0;
const listeners = new Set<() => void>();

function notify() {
  for (const l of listeners) l();
}

function appendEvent(event: CycleProgressEvent) {
  const row: EventRow = { ...event, _row_id: nextRowId++ };
  const base = events.length >= 200 ? events.slice(1) : events;
  events = [...base, row];
  notify();
}

function openSource() {
  if (source) return;
  const es = new EventSource("/api/autooptimizer/events");
  source = es;
  const handleMessage = (ev: Event) => {
    const event = parseSsePayload((ev as MessageEvent).data, ev.type);
    if (event) appendEvent(event);
  };
  es.addEventListener("open", () => {
    connected = true;
    notify();
  });
  es.addEventListener("message", handleMessage);
  for (const name of SSE_EVENT_NAMES) es.addEventListener(name, handleMessage);
  es.addEventListener("error", () => {
    connected = false;
    notify();
  });
}

function closeSource() {
  source?.close();
  source = null;
  connected = false;
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  refCount += 1;
  openSource();
  return () => {
    listeners.delete(cb);
    refCount -= 1;
    if (refCount <= 0) {
      refCount = 0;
      closeSource();
    }
  };
}

const getEvents = () => events;
const getConnected = () => connected;

export function useCycleEventStream(): {
  events: EventRow[];
  connected: boolean;
  isRunning: boolean;
  activeCycleId: string | null;
} {
  const evs = useSyncExternalStore(subscribe, getEvents, getEvents);
  const isConnected = useSyncExternalStore(subscribe, getConnected, getConnected);

  // Derive running state from the event buffer — single source of truth for
  // both the heatmap and command bar so they don't diverge.
  let isRunning = false;
  let activeCycleId: string | null = null;
  for (let i = evs.length - 1; i >= 0; i--) {
    const et = evs[i].event_type ?? evs[i].type ?? evs[i].kind ?? "";
    if (et === "cycle_finished") { isRunning = false; activeCycleId = null; break; }
    if (et === "cycle_started") { isRunning = true; activeCycleId = evs[i].cycle_id ?? null; break; }
  }

  return { events: evs, connected: isConnected, isRunning, activeCycleId };
}
