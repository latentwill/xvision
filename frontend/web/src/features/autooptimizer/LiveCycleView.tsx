import { type RefObject, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Card, CardHeader } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ModelPicker } from "@/components/ModelPicker";
import { apiFetch, ApiError } from "@/api/client";
import { type CycleProgressEvent, formatEventLabel } from "./api";
import {
  getStoredJudgeModel,
  getStoredMutatorModel,
  getStoredJudgeProvider,
  getStoredMutatorProvider,
  setStoredJudgeModel,
  setStoredMutatorModel,
  setStoredJudgeProvider,
  setStoredMutatorProvider,
} from "./preferences";
import { listStrategies, strategyKeys } from "@/api/strategies";
import { listProviders, settingsKeys } from "@/api/settings";

type EventRow = CycleProgressEvent & { _row_id: number };
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
  "cycle_sealed",
] as const;

async function submitEveningCycle(
  strategyId: string,
  mutatorModel: string,
  judgeModel: string,
): Promise<string | null> {
  try {
    await apiFetch<{ started: boolean; message: string }>("/api/autooptimizer/evening-cycle", {
      method: "POST",
      body: JSON.stringify({
        strategy_id: strategyId,
        mutator_model: mutatorModel,
        judge_model: judgeModel,
      }),
    });
    return null;
  } catch (e) {
    if (e instanceof ApiError) {
      if (e.status === 404 || e.status === 501) return "Not yet available on this server";
      return e.message || `Error ${e.status}`;
    }
    return e instanceof Error ? e.message : "Network error";
  }
}

function LaunchStrip() {
  const [strategyId, setStrategyId] = useState("");
  const [isRunning, setIsRunning] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const { data: strategies, isPending: strategiesLoading } = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const handleLaunch = async () => {
    const trimmed = strategyId.trim();
    if (!trimmed) { setLaunchError("Select a strategy"); return; }
    setIsRunning(true);
    setLaunchError(null);
    const err = await submitEveningCycle(
      trimmed,
      getStoredMutatorModel() ?? "claude-haiku-4-5-20251001",
      getStoredJudgeModel() ?? "claude-sonnet-4-6",
    );
    setLaunchError(err);
    setIsRunning(false);
  };
  const inp = "bg-surface border border-border rounded text-text text-[13px] px-2 py-1";
  const noStrategies = !strategiesLoading && (!strategies || strategies.length === 0);
  return (
    <div className="flex flex-col gap-2">
      <select
        value={strategyId}
        onChange={(e) => setStrategyId(e.target.value)}
        disabled={isRunning || strategiesLoading || noStrategies}
        aria-label="Strategy"
        className={`${inp} w-full`}
      >
        {strategiesLoading ? (
          <option value="">Loading…</option>
        ) : noStrategies ? (
          <option value="">No strategies</option>
        ) : (
          <>
            <option value="">— pick a strategy —</option>
            {strategies!.map((s) => (
              <option key={s.agent_id} value={s.agent_id}>{s.display_name}</option>
            ))}
          </>
        )}
      </select>
      <button
        type="button"
        onClick={() => { void handleLaunch(); }}
        disabled={isRunning}
        className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent hover:opacity-90 disabled:opacity-50"
      >
        {isRunning ? "Running…" : "Start evening run"}
      </button>
      {launchError !== null && (
        <span className="text-[13px] text-danger">{launchError}</span>
      )}
    </div>
  );
}

function ModelSelectRow() {
  const providers = useQuery({ queryKey: settingsKeys.providers(), queryFn: listProviders });
  const rows = providers.data?.providers ?? [];
  const [mutatorProvider, setMutatorProvider] = useState<string | null>(() => getStoredMutatorProvider());
  const [mutatorModel, setMutatorModel] = useState<string>(() => getStoredMutatorModel() ?? "");
  const [judgeProvider, setJudgeProvider] = useState<string | null>(() => getStoredJudgeProvider());
  const [judgeModel, setJudgeModel] = useState<string>(() => getStoredJudgeModel() ?? "");
  const sel = "bg-surface border border-border rounded text-text text-[13px] px-2 py-1";
  return (
    <div className="space-y-3 pt-3 border-t border-border">
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-text-3 text-[12px] whitespace-nowrap">Writer</span>
        <ModelPicker
          rows={rows}
          loading={providers.isLoading}
          provider={mutatorProvider}
          model={mutatorModel}
          onChange={(p, m) => { setMutatorProvider(p); setMutatorModel(m); if (p !== null) setStoredMutatorProvider(p); if (m) setStoredMutatorModel(m); }}
          className={sel}
          ariaLabel="Experiment writer model"
        />
      </div>
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-text-3 text-[12px] whitespace-nowrap">Reviewer</span>
        <ModelPicker
          rows={rows}
          loading={providers.isLoading}
          provider={judgeProvider}
          model={judgeModel}
          onChange={(p, m) => { setJudgeProvider(p); setJudgeModel(m); if (p !== null) setStoredJudgeProvider(p); if (m) setStoredJudgeModel(m); }}
          className={sel}
          ariaLabel="Reviewer model"
        />
      </div>
    </div>
  );
}

function CycleLeftCard() {
  return (
    <div className="rounded-md border border-gold/30 bg-gradient-to-b from-gold/5 to-transparent p-5 space-y-4">
      <span className="uppercase tracking-[0.22em] text-[9.5px] text-gold font-medium block">
        Evening Run
      </span>
      <Pill tone="default">No cycle running</Pill>
      <LaunchStrip />
      <ModelSelectRow />
    </div>
  );
}

function KeptNextCard() {
  return (
    <div className="rounded-md border border-border p-5 space-y-4">
      <div>
        <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium block">
          Kept
        </span>
        <span className="font-mono text-3xl font-semibold text-gold">0</span>
        <p className="text-[12px] text-text-3 mt-1">experiments kept this week</p>
      </div>
      <div className="border-t border-border pt-4">
        <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium block">
          Next
        </span>
        <p className="text-[13px] text-text-2 mt-1">No scheduled run</p>
      </div>
    </div>
  );
}

function EventLogCard({ events, bottomRef }: { events: EventRow[]; bottomRef: RefObject<HTMLDivElement> }) {
  return (
    <Card>
      <CardHeader title="Live progress · cycle events" />
      {events.length === 0 ? (
        <div className="px-5 pb-5 pt-2 text-[13px] text-text-3">
          Waiting for cycle…
        </div>
      ) : (
        <div
          className="overflow-y-auto max-h-[480px] pb-4"
          role="log"
          aria-live="polite"
          aria-label="Cycle event feed"
        >
          <table className="w-full text-[13px] border-collapse">
            <thead>
              <tr className="sticky top-0 bg-surface-card border-b border-border">
                <th className="text-left font-medium text-text-3 px-5 py-2 w-[140px]">Time</th>
                <th className="text-left font-medium text-text-3 px-5 py-2">Event</th>
                <th className="text-left font-medium text-text-3 px-5 py-2 w-[160px]">Cycle</th>
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
                  <td className="px-5 py-2 text-text">{formatEventLabel(ev)}</td>
                  <td className="px-5 py-2 text-text-3 font-mono text-[11px] truncate max-w-[160px]">
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
  );
}

function ActiveLineagesSection() {
  return (
    <div className="space-y-3">
      <div>
        <h2 className="text-base font-semibold text-text">Active lineages</h2>
        <p className="font-mono text-[11.5px] text-text-3 mt-0.5">
          Strategy populations currently evolving
        </p>
      </div>
      <div className="rounded-md border border-border px-5 py-4">
        <p className="text-[13px] text-text-3">No lineages yet</p>
      </div>
    </div>
  );
}

function RecentCyclesSection() {
  return (
    <div className="space-y-3">
      <div>
        <h2 className="text-base font-semibold text-text">Recent cycles</h2>
        <p className="font-mono text-[11.5px] text-text-3 mt-0.5">
          History of completed optimization cycles
        </p>
      </div>
      <div className="rounded-md border border-border px-5 py-4">
        <p className="text-[13px] text-text-3">
          No cycles yet — see Ladder and Provenance tabs for experiment history
        </p>
      </div>
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
    source.addEventListener("open", () => { setConnected(true); });
    source.addEventListener("message", handleMessage);
    for (const name of CYCLE_EVENT_NAMES) source.addEventListener(name, handleMessage);
    source.addEventListener("error", () => { setConnected(false); });
    return () => { source.close(); setConnected(false); };
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView?.({ behavior: "smooth" });
  }, [events.length]);

  return (
    <div className="space-y-6">
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
      <div className="grid grid-cols-1 xl:grid-cols-[300px_1fr_260px] gap-6">
        <CycleLeftCard />
        <EventLogCard events={events} bottomRef={bottomRef} />
        <KeptNextCard />
      </div>
      <ActiveLineagesSection />
      <RecentCyclesSection />
    </div>
  );
}

function formatEventTime(ts: string | undefined): string {
  if (!ts) return "—";
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

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}
