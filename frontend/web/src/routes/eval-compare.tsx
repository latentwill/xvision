import { Link, useSearchParams } from "react-router-dom";
import { useQueries } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { evalKeys, getRun } from "@/api/eval";
import type {
  DecisionRowDto,
  EquityPoint,
  RunDetail,
  RunSummary,
} from "@/api/types.gen";

const STATUS_TONE: Record<
  string,
  "gold" | "warn" | "danger" | "default" | "info"
> = {
  completed: "gold",
  running: "info",
  queued: "default",
  failed: "danger",
  cancelled: "warn",
};

export function EvalCompareRoute() {
  const [search] = useSearchParams();
  const idsParam = search.get("ids") ?? "";
  const ids = idsParam
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  if (ids.length === 0) {
    return <EmptyCompare />;
  }

  return <CompareGrid ids={ids} />;
}

function CompareGrid({ ids }: { ids: string[] }) {
  const queries = useQueries({
    queries: ids.map((id) => ({
      queryKey: evalKeys.run(id),
      queryFn: () => getRun(id),
    })),
  });

  // Pre-compute equity scale across all runs so sparklines are visually
  // comparable across columns. Only consider runs that loaded successfully.
  const allEquity = queries
    .flatMap((q) => q.data?.equity_curve ?? [])
    .map((p) => p.equity_usd);
  const sharedMin = allEquity.length > 0 ? Math.min(...allEquity) : 0;
  const sharedMax = allEquity.length > 0 ? Math.max(...allEquity) : 1;

  return (
    <>
      <Topbar
        title="Compare runs"
        sub={`${ids.length} run${ids.length === 1 ? "" : "s"}`}
      />

      <ComparisonSummary queries={queries} />

      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Equity curves
      </h2>
      <div
        className="grid gap-5"
        style={{ gridTemplateColumns: `repeat(${ids.length}, minmax(0, 1fr))` }}
      >
        {queries.map((q, i) => (
          <Card key={ids[i]} className="p-4">
            <CompactHeader id={ids[i]} q={q} />
            {q.data ? (
              <SharedSparkline
                points={q.data.equity_curve}
                sharedMin={sharedMin}
                sharedMax={sharedMax}
              />
            ) : q.isPending ? (
              <Skeleton h={120} />
            ) : (
              <p className="m-0 text-text-3 text-[12px] py-6 text-center">
                couldn't load
              </p>
            )}
          </Card>
        ))}
      </div>

      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Decisions
      </h2>
      <div
        className="grid gap-5"
        style={{ gridTemplateColumns: `repeat(${ids.length}, minmax(0, 1fr))` }}
      >
        {queries.map((q, i) => (
          <Card key={ids[i]} className="p-4">
            <CompactHeader id={ids[i]} q={q} />
            {q.data ? (
              <DecisionsBreakdown
                decisions={q.data.decisions}
                runId={q.data.summary.id}
              />
            ) : q.isPending ? (
              <Skeleton h={80} />
            ) : (
              <p className="m-0 text-text-3 text-[12px] py-6 text-center">
                couldn't load
              </p>
            )}
          </Card>
        ))}
      </div>

      <BackLink />
    </>
  );
}

function ComparisonSummary({
  queries,
}: {
  queries: { data?: RunDetail; isPending: boolean; isError: boolean; error?: unknown }[];
}) {
  const summaries = queries.map((q) => q.data?.summary);

  // The "winner" highlights: highest Sharpe + highest total_return + lowest
  // max_drawdown. Each is a column-position highlight (or -1 when nobody has
  // finite numbers).
  const idxOfMax = (vals: (number | null | undefined)[]) => {
    let bestIdx = -1;
    let best = -Infinity;
    vals.forEach((v, i) => {
      if (v != null && Number.isFinite(v) && v > best) {
        best = v;
        bestIdx = i;
      }
    });
    return bestIdx;
  };
  const idxOfMin = (vals: (number | null | undefined)[]) => {
    let bestIdx = -1;
    let best = Infinity;
    vals.forEach((v, i) => {
      if (v != null && Number.isFinite(v) && v < best) {
        best = v;
        bestIdx = i;
      }
    });
    return bestIdx;
  };
  const sharpes = summaries.map((s) => s?.sharpe);
  const returns = summaries.map((s) => s?.total_return_pct);
  const drawdowns = summaries.map((s) => s?.max_drawdown_pct);

  const winner = {
    sharpe: idxOfMax(sharpes),
    return_: idxOfMax(returns),
    drawdown: idxOfMin(drawdowns),
  };

  return (
    <div
      className="grid gap-5"
      style={{
        gridTemplateColumns: `repeat(${queries.length}, minmax(0, 1fr))`,
      }}
    >
      {queries.map((q, i) => (
        <Card key={i} className="p-4">
          {q.isPending ? (
            <>
              <div className="h-4 w-32 bg-surface-elev rounded mb-3 animate-pulse" />
              <Skeleton h={64} />
            </>
          ) : q.isError || !q.data ? (
            <div>
              <div className="text-text-3 text-[12px] font-mono mb-3">
                run #{i + 1}
              </div>
              <p className="m-0 text-danger text-[12px]">
                {errorMessage(q.error)}
              </p>
            </div>
          ) : (
            <RunSummaryColumn
              summary={q.data.summary}
              winner={{
                sharpe: winner.sharpe === i,
                return_: winner.return_ === i,
                drawdown: winner.drawdown === i,
              }}
            />
          )}
        </Card>
      ))}
    </div>
  );
}

function RunSummaryColumn({
  summary,
  winner,
}: {
  summary: RunSummary;
  winner: { sharpe: boolean; return_: boolean; drawdown: boolean };
}) {
  const tone = STATUS_TONE[summary.status] ?? "default";
  return (
    <>
      <div className="flex items-center justify-between mb-3">
        <Link
          to={`/eval-runs/${encodeURIComponent(summary.id)}`}
          className="text-text-3 text-[12px] font-mono hover:text-text"
          title={summary.id}
        >
          {summary.id.slice(0, 12)}…
        </Link>
        <Pill tone={tone}>{summary.status}</Pill>
      </div>
      <div className="text-text-2 text-[12px] mb-3">
        scenario{" "}
        <code className="font-mono text-text">{summary.scenario_id}</code> ·{" "}
        <span className="font-mono">{summary.mode}</span>
      </div>
      <dl className="grid grid-cols-2 gap-y-2 text-[13px]">
        <Metric
          label="Sharpe"
          value={fmtNumber(summary.sharpe)}
          highlight={winner.sharpe}
        />
        <Metric
          label="Total return"
          value={fmtPct(summary.total_return_pct)}
          highlight={winner.return_}
        />
        <Metric
          label="Max DD"
          value={fmtPct(summary.max_drawdown_pct)}
          highlight={winner.drawdown}
        />
        <Metric label="Started" value={fmtTime(summary.started_at)} />
      </dl>
      {summary.error ? (
        <p className="mt-3 text-[12px] text-danger m-0">
          <code className="font-mono">{summary.error}</code>
        </p>
      ) : null}
    </>
  );
}

function Metric({
  label,
  value,
  highlight,
}: {
  label: string;
  value: string;
  highlight?: boolean;
}) {
  return (
    <>
      <dt className="text-text-3 text-[11px] uppercase tracking-wide">
        {label}
      </dt>
      <dd
        className={`m-0 font-mono ${highlight ? "text-gold" : "text-text"}`}
        title={highlight ? "Best across compared runs" : undefined}
      >
        {value}
        {highlight ? <span className="text-gold ml-1">★</span> : null}
      </dd>
    </>
  );
}

function CompactHeader({
  id,
  q,
}: {
  id: string;
  q: { data?: RunDetail };
}) {
  return (
    <div className="flex items-center justify-between mb-3">
      <Link
        to={`/eval-runs/${encodeURIComponent(id)}`}
        className="text-text-3 text-[11px] font-mono hover:text-text"
      >
        {id.slice(0, 12)}…
      </Link>
      {q.data ? (
        <Pill tone={STATUS_TONE[q.data.summary.status] ?? "default"}>
          {q.data.summary.status}
        </Pill>
      ) : null}
    </div>
  );
}

function SharedSparkline({
  points,
  sharedMin,
  sharedMax,
}: {
  points: EquityPoint[];
  sharedMin: number;
  sharedMax: number;
}) {
  if (points.length === 0) {
    return (
      <p className="m-0 text-text-3 text-[12px] text-center py-6">no samples</p>
    );
  }
  const w = 360;
  const h = 120;
  const range = sharedMax - sharedMin || 1;
  const path = points
    .map((p, i) => {
      const x = (i / (points.length - 1 || 1)) * w;
      const y = h - 4 - ((p.equity_usd - sharedMin) / range) * (h - 8);
      return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
  const last = points[points.length - 1];
  return (
    <div>
      <svg
        viewBox={`0 0 ${w} ${h}`}
        className="w-full h-[120px]"
        aria-label="Equity sparkline (shared scale across compared runs)"
      >
        <path d={path} fill="none" stroke="var(--gold)" strokeWidth="1.4" />
      </svg>
      <div className="flex items-center justify-between text-[11px] text-text-3 mt-1">
        <span>{fmtTime(points[0].timestamp)}</span>
        <span className="font-mono text-text">
          $
          {last.equity_usd.toLocaleString(undefined, {
            maximumFractionDigits: 0,
          })}
        </span>
        <span>{fmtTime(last.timestamp)}</span>
      </div>
    </div>
  );
}

function DecisionsBreakdown({
  decisions,
  runId,
}: {
  decisions: DecisionRowDto[];
  runId: string;
}) {
  if (decisions.length === 0) {
    return (
      <p className="m-0 text-text-3 text-[12px] text-center py-6">
        no decisions recorded
      </p>
    );
  }
  const counts = new Map<string, number>();
  let fills = 0;
  for (const d of decisions) {
    counts.set(d.action, (counts.get(d.action) ?? 0) + 1);
    if (d.fill_price != null) fills += 1;
  }
  const ordered = Array.from(counts.entries()).sort((a, b) => b[1] - a[1]);
  return (
    <>
      <dl className="grid grid-cols-2 gap-y-1.5 text-[12px]">
        <dt className="text-text-3 uppercase tracking-wide">Total</dt>
        <dd className="m-0 font-mono text-text">{decisions.length}</dd>
        <dt className="text-text-3 uppercase tracking-wide">Fills</dt>
        <dd className="m-0 font-mono text-text">{fills}</dd>
        {ordered.map(([action, n]) => (
          <FragmentRow key={action} label={action} value={n} />
        ))}
      </dl>
      <Link
        to={`/eval-runs/${encodeURIComponent(runId)}`}
        className="block mt-3 text-[12px] text-text-3 hover:text-text"
      >
        View full run →
      </Link>
    </>
  );
}

function FragmentRow({ label, value }: { label: string; value: number }) {
  return (
    <>
      <dt className="text-text-3 capitalize">{label.replace(/_/g, " ")}</dt>
      <dd className="m-0 font-mono text-text">{value}</dd>
    </>
  );
}

function EmptyCompare() {
  return (
    <>
      <Topbar title="Compare runs" sub="Pick runs to compare" />
      <Card className="px-6 py-16 text-center">
        <div className="font-serif italic text-[28px] text-text-3 mb-3">
          nothing to compare yet
        </div>
        <p className="m-0 max-w-md mx-auto text-text-2 leading-snug mb-5">
          Compare runs side-by-side by passing them on the URL:{" "}
          <code className="font-mono text-text">
            /eval-runs/compare?ids=&lt;a&gt;,&lt;b&gt;
          </code>
        </p>
        <Link
          to="/eval-runs"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          ← Browse runs
        </Link>
      </Card>
    </>
  );
}

function BackLink() {
  return (
    <div className="mt-8 text-[13px]">
      <Link to="/eval-runs" className="text-text-2 hover:text-text">
        ← Back to runs
      </Link>
    </div>
  );
}

function Skeleton({ h }: { h: number }) {
  return (
    <div
      className="bg-surface-elev rounded animate-pulse w-full"
      style={{ height: h }}
    />
  );
}

function fmtNumber(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "—";
  return n.toFixed(3);
}

function fmtPct(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "—";
  return `${n.toFixed(2)}%`;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
