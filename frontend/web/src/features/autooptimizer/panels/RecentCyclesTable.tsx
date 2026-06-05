import { Link } from "react-router-dom";
import { useCycleRuns, type CycleRunSummary } from "../api";

function money(n?: number | null): string {
  return n == null ? "—" : `$${n.toFixed(2)}`;
}
function tokens(n?: number | null): string {
  if (n == null) return "—";
  return n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : `${(n / 1000).toFixed(0)}k`;
}

export function RecentCyclesTable() {
  const { data, isLoading, isError } = useCycleRuns();
  const rows: CycleRunSummary[] = data ?? [];
  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <h2 className="m-0 mb-3 text-[15px] font-semibold tracking-tight">Recent cycles</h2>
      {isLoading ? (
        <p className="text-[12px] text-text-3">Loading…</p>
      ) : isError ? (
        <p className="text-[12px] text-danger">Couldn't load cycles.</p>
      ) : rows.length === 0 ? (
        <p className="text-[12px] text-text-3">No optimizer cycles have run yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-[12px]">
            <thead>
              <tr className="text-left text-text-3">
                <th className="py-1.5 pr-3 font-medium">Cycle</th>
                <th className="py-1.5 pr-3 text-right font-medium">Experiments</th>
                <th className="py-1.5 pr-3 text-right font-medium">Kept</th>
                <th className="py-1.5 pr-3 text-right font-medium">Tokens</th>
                <th className="py-1.5 text-right font-medium">$</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {rows.map((c) => (
                <tr key={c.cycle_id} className="border-t border-border-soft hover:bg-gold/[0.03]">
                  <td className="py-1.5 pr-3">
                    <Link to={`/optimizer/cycle/${encodeURIComponent(c.cycle_id)}`} className="text-text hover:text-gold">
                      {c.cycle_id}
                    </Link>
                  </td>
                  <td className="py-1.5 pr-3 text-right text-text-2">{c.node_count}</td>
                  <td className="py-1.5 pr-3 text-right text-gold">{c.active_count}</td>
                  <td className="py-1.5 pr-3 text-right text-text-2">{tokens(c.input_tokens != null && c.output_tokens != null ? c.input_tokens + c.output_tokens : null)}</td>
                  <td className="py-1.5 text-right text-text-2">{money(c.cost_usd)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
