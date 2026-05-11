import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { evalKeys, listRuns } from "@/api/eval";
import type { RunSummary } from "@/api/types.gen";

const STATUS_TONE: Record<string, "gold" | "warn" | "danger" | "default" | "info"> = {
  completed: "gold",
  running: "info",
  queued: "default",
  failed: "danger",
  cancelled: "warn",
};

export function EvalRunsRoute() {
  const q = useQuery({
    queryKey: evalKeys.runs(),
    queryFn: listRuns,
  });
  const navigate = useNavigate();
  // Selection state for the Compare flow. Lifted here so the Topbar can
  // render the action button next to the run count.
  const [selected, setSelected] = useState<Set<string>>(() => new Set());

  function toggleSelected(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function clearSelection() {
    setSelected(new Set());
  }

  function openCompare() {
    if (selected.size < 2) return;
    const ids = [...selected].join(",");
    navigate(`/eval-runs/compare?ids=${ids}`);
  }

  return (
    <>
      <Topbar title="Eval" sub={subtitleFor(q)} />
      {selected.size > 0 ? (
        <div className="mb-3 flex justify-end">
          <CompareToolbar
            count={selected.size}
            onCompare={openCompare}
            onClear={clearSelection}
          />
        </div>
      ) : null}
      <Card>
        {q.isPending ? (
          <LoadingSkeleton />
        ) : q.isError ? (
          <ErrorState err={q.error} onRetry={() => q.refetch()} />
        ) : q.data && q.data.length === 0 ? (
          <EmptyState />
        ) : (
          <RunsTable
            items={q.data ?? []}
            selected={selected}
            onToggle={toggleSelected}
          />
        )}
      </Card>
    </>
  );
}

function subtitleFor(q: ReturnType<typeof useQuery>) {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load runs";
  const data = q.data as { length: number } | undefined;
  if (!data) return "";
  const n = data.length;
  return `${n} ${n === 1 ? "run" : "runs"}`;
}

function CompareToolbar({
  count,
  onCompare,
  onClear,
}: {
  count: number;
  onCompare: () => void;
  onClear: () => void;
}) {
  const ready = count >= 2;
  return (
    <div className="flex items-center gap-2">
      <span className="text-[12px] text-text-2">
        {count} selected
      </span>
      <button
        type="button"
        onClick={onClear}
        className="text-[12px] text-text-3 hover:text-text underline-offset-2 hover:underline"
      >
        clear
      </button>
      <button
        type="button"
        onClick={onCompare}
        disabled={!ready}
        title={ready ? "" : "Select at least two runs to compare"}
        className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[13px] font-medium border transition-colors ${
          ready
            ? "border-gold text-gold hover:bg-gold/10"
            : "border-border text-text-3 cursor-not-allowed opacity-60"
        }`}
      >
        Compare {ready ? `(${count})` : ""} →
      </button>
    </div>
  );
}

function RunsTable({
  items,
  selected,
  onToggle,
}: {
  items: RunSummary[];
  selected: Set<string>;
  onToggle: (id: string) => void;
}) {
  const navigate = useNavigate();

  function go(id: string) {
    navigate(`/eval-runs/${id}`);
  }

  return (
    <table className="w-full">
      <thead>
        <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
          <th className="font-normal py-2.5 pl-5 pr-2 w-8"></th>
          <th className="font-normal py-2.5 px-3">ID</th>
          <th className="font-normal py-2.5 px-3">Strategy</th>
          <th className="font-normal py-2.5 px-3">Scenario</th>
          <th className="font-normal py-2.5 px-3">Mode</th>
          <th className="font-normal py-2.5 px-3">Status</th>
          <th className="font-normal py-2.5 px-3 text-right">Sharpe</th>
          <th className="font-normal py-2.5 px-3 text-right">Max DD</th>
          <th className="font-normal py-2.5 px-3 text-right">Return</th>
          <th className="font-normal py-2.5 px-5">Started</th>
        </tr>
      </thead>
      <tbody>
        {items.map((row) => {
          const isChecked = selected.has(row.id);
          return (
            <tr
              key={row.id}
              role="link"
              tabIndex={0}
              onClick={() => go(row.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  go(row.id);
                }
              }}
              className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover focus:bg-surface-hover focus:outline-none transition-colors cursor-pointer"
            >
              <td
                className="py-3 pl-5 pr-2 w-8"
                // Stop the checkbox cell from triggering row navigation. The
                // input below stops its own events too, but covering the
                // surrounding cell makes the entire 32px tap target safe.
                onClick={(e) => e.stopPropagation()}
              >
                <input
                  type="checkbox"
                  aria-label={`Select run ${row.id.slice(0, 8)}`}
                  checked={isChecked}
                  onChange={(e) => {
                    e.stopPropagation();
                    onToggle(row.id);
                  }}
                  onClick={(e) => e.stopPropagation()}
                  onKeyDown={(e) => e.stopPropagation()}
                  className="accent-gold cursor-pointer"
                />
              </td>
              <td className="py-3 px-3 font-mono text-text text-[12px]">
                {row.id.slice(0, 12)}…
              </td>
              <td className="py-3 px-3 font-mono text-text-2 text-[12px]">
                {row.strategy_bundle_hash.slice(0, 8)}
              </td>
              <td className="py-3 px-3 text-text-2">{row.scenario_id}</td>
              <td className="py-3 px-3 text-text-2">{row.mode}</td>
              <td className="py-3 px-3">
                <StatusPill status={row.status} />
              </td>
              <td className="py-3 px-3 text-right font-mono">
                {fmtNumber(row.sharpe)}
              </td>
              <td className="py-3 px-3 text-right font-mono">
                {fmtPct(row.max_drawdown_pct)}
              </td>
              <td className="py-3 px-3 text-right font-mono">
                {fmtPct(row.total_return_pct)}
              </td>
              <td className="py-3 px-5 text-text-3 text-[12px]">
                {fmtTime(row.started_at)}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function StatusPill({ status }: { status: string }) {
  const tone = STATUS_TONE[status] ?? "default";
  return (
    <Pill tone={tone}>
      <span className="w-1.5 h-1.5 rounded-full" style={dotColor(tone)} />
      {status}
    </Pill>
  );
}

function dotColor(tone: "gold" | "warn" | "danger" | "default" | "info") {
  return {
    gold: { background: "var(--gold)" },
    warn: { background: "var(--warn)" },
    danger: { background: "var(--danger)" },
    info: { background: "var(--info)" },
    default: { background: "var(--text-3)" },
  }[tone];
}

function fmtNumber(n: number | null | undefined): string {
  if (n == null) return "—";
  return n.toFixed(2);
}

function fmtPct(n: number | null | undefined): string {
  if (n == null) return "—";
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(2)}%`;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function LoadingSkeleton() {
  return (
    <div className="px-5 py-4 space-y-3" aria-busy>
      {Array.from({ length: 4 }).map((_, i) => (
        <div key={i} className="flex items-center gap-4 py-2">
          <div className="h-4 w-32 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-24 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-20 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-16 rounded bg-surface-elev animate-pulse" />
        </div>
      ))}
    </div>
  );
}

function EmptyState() {
  return (
    <div className="px-6 py-16 text-center text-text-2">
      <div className="font-serif italic text-[28px] text-text-3 mb-3">
        no runs yet
      </div>
      <p className="m-0 max-w-md mx-auto leading-snug">
        Eval runs created via{" "}
        <code className="text-text font-mono">xvn ab-compare</code> or the
        eval engine will appear here. Run something to get started.
      </p>
    </div>
  );
}

function ErrorState({ err, onRetry }: { err: unknown; onRetry: () => void }) {
  const detail =
    err instanceof ApiError
      ? `${err.code}: ${err.message}`
      : err instanceof Error
        ? err.message
        : String(err);
  return (
    <div className="px-6 py-12 text-center">
      <div className="font-serif italic text-[24px] text-danger mb-3">
        couldn't load runs
      </div>
      <p className="m-0 mb-5 max-w-md mx-auto text-text-2 leading-snug">
        <code className="text-danger font-mono text-[12px]">{detail}</code>
      </p>
      <button
        onClick={onRetry}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
      >
        Retry
      </button>
    </div>
  );
}
