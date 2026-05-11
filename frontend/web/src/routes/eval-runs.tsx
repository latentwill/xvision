import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
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

  return (
    <>
      <Topbar title="Eval" sub={subtitleFor(q)} />
      <Card>
        {q.isPending ? (
          <LoadingSkeleton />
        ) : q.isError ? (
          <ErrorState err={q.error} onRetry={() => q.refetch()} />
        ) : q.data && q.data.length === 0 ? (
          <EmptyState />
        ) : (
          <RunsTable items={q.data ?? []} />
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

function RunsTable({ items }: { items: RunSummary[] }) {
  const navigate = useNavigate();
  const [selected, setSelected] = useState<Set<string>>(new Set());

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function openRun(id: string) {
    navigate(`/eval-runs/${id}`);
  }

  function openCompare() {
    if (selected.size < 2) return;
    const ids = [...selected].join(",");
    navigate(`/eval-runs/compare?ids=${ids}`);
  }

  return (
    <>
      <div className="flex items-center justify-between px-5 py-3 border-b border-border-soft">
        <div className="text-[12px] text-text-3">
          {selected.size === 0
            ? "Tip: select two or more runs to compare them side-by-side."
            : `${selected.size} selected`}
        </div>
        <button
          type="button"
          onClick={openCompare}
          disabled={selected.size < 2}
          className="inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] border border-border text-text hover:border-text-3 disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:border-border"
        >
          Compare ({selected.size})
        </button>
      </div>
      <table className="w-full">
        <thead>
          <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
            <th className="font-normal py-2.5 pl-5 pr-2 w-8" aria-label="Select" />
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
          {items.map((row) => (
            <RunRow
              key={row.id}
              row={row}
              checked={selected.has(row.id)}
              onToggle={() => toggle(row.id)}
              onOpen={() => openRun(row.id)}
            />
          ))}
        </tbody>
      </table>
    </>
  );
}

function RunRow({
  row,
  checked,
  onToggle,
  onOpen,
}: {
  row: RunSummary;
  checked: boolean;
  onToggle: () => void;
  onOpen: () => void;
}) {
  return (
    <tr
      role="link"
      tabIndex={0}
      aria-label={`Open run ${row.id}`}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
      className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover cursor-pointer transition-colors focus:outline-none focus:bg-surface-hover"
    >
      <td
        className="py-3 pl-5 pr-2 w-8"
        // Clicks on the checkbox cell must not also navigate.
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <input
          type="checkbox"
          aria-label={`Select run ${row.id}`}
          checked={checked}
          onChange={onToggle}
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
      <td className="py-3 px-3 text-right font-mono">{fmtNumber(row.sharpe)}</td>
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
