import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Card, CardHeader } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ModelPicker } from "@/components/ModelPicker";
import { useLineageNodes, type LineageNode, type CycleProgressEvent, formatEventLabel } from "./api";
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

let nextRowId = 1;

async function submitEveningCycle(
  strategyId: string,
  mutatorModel: string,
  judgeModel: string,
): Promise<string | null> {
  try {
    const resp = await fetch("/api/autooptimizer/evening-cycle", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        strategy_id: strategyId,
        mutator_model: mutatorModel,
        judge_model: judgeModel,
      }),
    });
    if (resp.status === 404 || resp.status === 501) return "Not yet available on this server";
    if (!resp.ok) {
      const text = await resp.text();
      return errorMessageFromResponse(text) || `Error ${resp.status}`;
    }
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : "Network error";
  }
}

function errorMessageFromResponse(text: string): string {
  if (!text) return "";
  try {
    const parsed = JSON.parse(text) as { message?: unknown };
    return typeof parsed.message === "string" ? parsed.message : text;
  } catch {
    return text;
  }
}

function deriveIsRunning(events: EventRow[]): boolean {
  for (let i = events.length - 1; i >= 0; i--) {
    const et = events[i].event_type;
    if (et === "cycle_finished") return false;
    if (et === "cycle_started") return true;
  }
  return false;
}

function formatRelativeDate(isoString: string): string {
  try {
    const ms = Date.now() - new Date(isoString).getTime();
    const mins = Math.floor(ms / 60_000);
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    const days = Math.floor(hrs / 24);
    return `${days}d ago`;
  } catch {
    return isoString;
  }
}

function groupByCycleId(nodes: LineageNode[]): Map<string, LineageNode[]> {
  const map = new Map<string, LineageNode[]>();
  for (const node of nodes) {
    if (!node.cycle_id) continue;
    const bucket = map.get(node.cycle_id) ?? [];
    bucket.push(node);
    map.set(node.cycle_id, bucket);
  }
  return map;
}

function latestCreatedAt(ns: LineageNode[]): string {
  return ns.reduce<string>((max, n) => (n.created_at > max ? n.created_at : max), "");
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

function CycleLeftCard({ isRunning }: { isRunning: boolean }) {
  return (
    <div className="rounded-md border border-gold/30 bg-gradient-to-b from-gold/5 to-transparent p-5 space-y-4">
      <span className="uppercase tracking-[0.22em] text-[9.5px] text-gold font-medium block">
        Evening Run
      </span>
      <Pill tone={isRunning ? "gold" : "default"} animated={isRunning}>
        {isRunning ? "Running" : "Idle"}
      </Pill>
      <LaunchStrip />
      <ModelSelectRow />
    </div>
  );
}

function KeptNextCard() {
  const { data: nodes, isPending } = useLineageNodes();
  const weekAgo = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000);
  const keptCount = (nodes ?? []).filter(
    (n) => n.status === "active" && new Date(n.created_at) >= weekAgo,
  ).length;
  return (
    <div className="rounded-md border border-border p-5 space-y-4">
      <div>
        <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium block">
          Kept
        </span>
        <span className="font-mono text-3xl font-semibold text-gold">
          {isPending ? "…" : keptCount}
        </span>
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

function EventLogCard({
  events,
  bottomRef,
}: {
  events: EventRow[];
  bottomRef: { current: HTMLDivElement | null };
}) {
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
  const { data: nodes } = useLineageNodes();
  const activeNodes = (nodes ?? []).filter((n) => n.status === "active");
  const cycleMap = groupByCycleId(activeNodes);
  const rows = Array.from(cycleMap.entries())
    .map(([cycleId, ns]) => ({ cycleId, count: ns.length, latestAt: latestCreatedAt(ns) }))
    .sort((a, b) => b.latestAt.localeCompare(a.latestAt))
    .slice(0, 5);
  return (
    <div className="space-y-3">
      <div>
        <h2 className="text-base font-semibold text-text">Active lineages</h2>
        <p className="font-mono text-[11.5px] text-text-3 mt-0.5">
          Strategy populations currently evolving
        </p>
      </div>
      <div className="rounded-md border border-border px-5 py-4">
        {rows.length === 0 ? (
          <p className="text-[13px] text-text-3">No active lineages</p>
        ) : (
          <table className="w-full text-[13px]">
            <tbody>
              {rows.map(({ cycleId, count, latestAt }) => (
                <tr key={cycleId} className="border-b border-border last:border-0">
                  <td className="py-2 pr-4 font-mono text-[12px] text-text-2 whitespace-nowrap">
                    {cycleId.slice(0, 12)}
                  </td>
                  <td className="py-2 pr-4 text-text-3">{count} experiments</td>
                  <td className="py-2 text-text-3 font-mono text-[12px]">
                    {formatRelativeDate(latestAt)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

function RecentCyclesSection() {
  const { data: nodes } = useLineageNodes();
  const cycleMap = groupByCycleId(nodes ?? []);
  const rows = Array.from(cycleMap.entries())
    .map(([cycleId, ns]) => ({
      cycleId,
      kept: ns.filter((n) => n.status === "active").length,
      total: ns.length,
      latestAt: latestCreatedAt(ns),
    }))
    .sort((a, b) => b.latestAt.localeCompare(a.latestAt))
    .slice(0, 10);
  return (
    <div className="space-y-3">
      <div>
        <h2 className="text-base font-semibold text-text">Recent cycles</h2>
        <p className="font-mono text-[11.5px] text-text-3 mt-0.5">
          History of completed optimization cycles
        </p>
      </div>
      <div className="rounded-md border border-border px-5 py-4">
        {rows.length === 0 ? (
          <p className="text-[13px] text-text-3">
            No cycles yet — see Ladder and Provenance tabs for experiment history
          </p>
        ) : (
          <table className="w-full text-[13px]">
            <tbody>
              {rows.map(({ cycleId, kept, total, latestAt }) => (
                <tr key={cycleId} className="border-b border-border last:border-0">
                  <td className="py-2 pr-4 font-mono text-[12px] text-text-2 whitespace-nowrap">
                    {cycleId.slice(0, 12)}
                  </td>
                  <td className="py-2 pr-4 text-text-3">{kept}/{total} kept</td>
                  <td className="py-2 text-text-3 font-mono text-[12px]">
                    {formatRelativeDate(latestAt)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

export function LiveCycleView() {
  const [events, setEvents] = useState<EventRow[]>([]);
  const [connected, setConnected] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const isRunning = deriveIsRunning(events);

  useEffect(() => {
    const source = new EventSource("/api/autooptimizer/events");
    source.addEventListener("open", () => { setConnected(true); });
    source.addEventListener("message", (ev) => {
      let raw: Record<string, unknown> | null = null;
      try { raw = JSON.parse(ev.data as string) as Record<string, unknown>; } catch { return; }
      if (!raw) return;
      const eventType = [raw["event_type"], raw["type"], raw["kind"]]
        .find((v): v is string => typeof v === "string") ?? "";
      const normalized: CycleProgressEvent = {
        ...(raw as unknown as CycleProgressEvent),
        event_type: eventType,
      };
      setEvents((prev) => {
        const row: EventRow = { ...normalized, _row_id: nextRowId++ };
        const next = prev.length >= 200 ? prev.slice(1) : prev;
        return [...next, row];
      });
    });
    source.addEventListener("error", () => { setConnected(false); });
    return () => { source.close(); setConnected(false); };
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
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
        <CycleLeftCard isRunning={isRunning} />
        <EventLogCard events={events} bottomRef={bottomRef} />
        <KeptNextCard />
      </div>
      <ActiveLineagesSection />
      <RecentCyclesSection />
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
