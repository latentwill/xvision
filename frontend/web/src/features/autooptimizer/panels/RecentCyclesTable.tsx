import { useState } from "react";
import { Link } from "react-router-dom";
import { useCycleRuns, type CycleRunSummary } from "../api";

function money(n?: number | null): string {
  return n == null ? "—" : `$${n.toFixed(2)}`;
}
function tokens(n?: number | null): string {
  if (n == null) return "—";
  return n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : `${(n / 1000).toFixed(0)}k`;
}

/** Default visible-row cap (UI3). The list must not render unbounded rows; the
 *  operator can reveal the rest with the "Show all" affordance. Also matches the
 *  page the backend serves so we don't paginate past what was fetched. */
const DEFAULT_LIMIT = 25;

/** Inner content (loading / error / table) without the surrounding card or heading.
 *  Used by `HistoryLedger` so the cycles table can share a card with the Runs view. */
export function RecentCyclesTableBody() {
  // Fetch at most one page (DEFAULT_LIMIT) from the backend — the history list
  // grew unbounded before this cap (UI3).
  const { data, isLoading, isError } = useCycleRuns({ limit: DEFAULT_LIMIT });
  const rows: CycleRunSummary[] = data ?? [];
  const [showAll, setShowAll] = useState(false);

  if (isLoading) return <p className="text-[12px] text-text-3">Loading…</p>;
  if (isError) return <p className="text-[12px] text-danger">Couldn't load cycles.</p>;
  if (rows.length === 0)
    return <p className="text-[12px] text-text-3">No optimizer cycles have run yet.</p>;

  const visible = showAll ? rows : rows.slice(0, DEFAULT_LIMIT);
  const hiddenCount = rows.length - visible.length;

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-[12px]">
        <thead>
          <tr className="text-left text-text-3">
            <th className="py-1.5 pr-3 font-medium">Cycle</th>
            <th className="py-1.5 pr-3 font-medium">Strategy</th>
            <th className="py-1.5 pr-3 text-right font-medium">Experiments</th>
            <th className="py-1.5 pr-3 text-right font-medium">Kept</th>
            <th className="py-1.5 pr-3 text-right font-medium">Tokens</th>
            <th className="py-1.5 text-right font-medium">$</th>
          </tr>
        </thead>
        <tbody className="font-mono">
          {visible.map((c) => (
            <tr key={c.cycle_id} className="border-t border-border-soft hover:bg-gold/[0.03]">
              <td className="py-1.5 pr-3">
                <Link to={`/optimizer/cycle/${encodeURIComponent(c.cycle_id)}`} className="text-text hover:text-gold">
                  {c.cycle_id}
                </Link>
              </td>
              <td className="py-1.5 pr-3">
                {c.strategy_id ? (
                  <Link
                    to={`/strategies/${encodeURIComponent(c.strategy_id)}`}
                    title={c.strategy_id}
                    className="text-text-2 hover:text-gold"
                  >
                    {c.strategy_id.length > 14 ? `${c.strategy_id.slice(0, 12)}…` : c.strategy_id}
                  </Link>
                ) : (
                  <span className="text-text-4">—</span>
                )}
              </td>
              <td className="py-1.5 pr-3 text-right text-text-2">{c.node_count}</td>
              <td className="py-1.5 pr-3 text-right text-gold">{c.active_count}</td>
              <td className="py-1.5 pr-3 text-right text-text-2">{tokens(c.input_tokens != null && c.output_tokens != null ? c.input_tokens + c.output_tokens : null)}</td>
              <td className="py-1.5 text-right text-text-2">{money(c.cost_usd)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {hiddenCount > 0 && (
        <button
          type="button"
          onClick={() => setShowAll(true)}
          className="mt-3 text-[12px] text-text-3 underline-offset-2 hover:text-gold hover:underline"
        >
          Show all ({rows.length})
        </button>
      )}
    </div>
  );
}

export function RecentCyclesTable() {
  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <h2 className="m-0 mb-3 text-[15px] font-semibold tracking-tight">Recent cycles</h2>
      <RecentCyclesTableBody />
    </section>
  );
}
