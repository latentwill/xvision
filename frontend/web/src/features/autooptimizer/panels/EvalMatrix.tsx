import { Link } from "react-router-dom";
import type { CycleNodeDetail } from "../api";
import { HashSigil } from "../ui/HashSigil";
import { DeltaCell } from "../ui/DeltaCell";

export function EvalMatrix({ nodes }: { nodes: CycleNodeDetail[] }) {
  // Derive stable first-seen union of regime labels across all nodes.
  const regimeLabels: string[] = [];
  const seen = new Set<string>();
  for (const node of nodes) {
    for (const r of node.regime_results) {
      if (!seen.has(r.regime_label)) {
        seen.add(r.regime_label);
        regimeLabels.push(r.regime_label);
      }
    }
  }

  const hasRegimes = regimeLabels.length > 0;

  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <div className="mb-3">
        <h2 className="m-0 text-[15px] font-semibold tracking-tight">Eval matrix</h2>
        <p className="mt-0.5 text-[12px] text-text-3">
          {hasRegimes
            ? "Δ-Sharpe vs parent · per regime"
            : "Lights up when the optimizer runs across a configured regime set."}
        </p>
      </div>

      {hasRegimes && (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-[12px]">
            <thead>
              <tr>
                <th className="pb-2 pr-4 text-left font-medium text-text-3">
                  Experiment
                </th>
                {regimeLabels.map((label) => (
                  <th
                    key={label}
                    className="pb-2 px-2 text-center font-medium text-text-3 min-w-[72px]"
                  >
                    {label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-border-soft">
              {nodes.map((node) => {
                // Build a map from regime_label → RegimeResult for O(1) lookup.
                const byLabel = new Map(
                  node.regime_results.map((r) => [r.regime_label, r]),
                );
                return (
                  <tr key={node.bundle_hash}>
                    <td className="py-2 pr-4">
                      <div className="flex items-center gap-2">
                        <HashSigil hash={node.bundle_hash} size={24} />
                        <Link
                          to={`/optimizer/experiment/${encodeURIComponent(node.bundle_hash)}`}
                          className="font-mono text-[12px] text-text hover:text-gold"
                        >
                          {node.bundle_hash.slice(0, 10)}
                        </Link>
                      </div>
                    </td>
                    {regimeLabels.map((label) => {
                      const r = byLabel.get(label);
                      return (
                        <td key={label} className="py-2 px-2">
                          {r != null ? (
                            <DeltaCell
                              state="done"
                              delta={r.delta_sharpe}
                              sharpe={r.metrics_day.sharpe}
                            />
                          ) : (
                            <DeltaCell state="queued" />
                          )}
                        </td>
                      );
                    })}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
