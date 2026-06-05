import { Link } from "react-router-dom";
import { useLineageNodes, formatGateVerdict, type LineageNode } from "../api";
import { HashSigil } from "../ui/HashSigil";
import { GateBadge } from "../ui/GateBadge";

export function CycleExperimentsTable({ cycleId }: { cycleId: string }) {
  const { data, isLoading, isError } = useLineageNodes({ cycleId });
  const rows: LineageNode[] = data ?? [];
  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <div className="mb-1">
        <h2 className="m-0 text-[15px] font-semibold tracking-tight">Experiments this cycle</h2>
        <p className="mt-0.5 text-[12px] text-text-3">what the optimizer tried · what was kept</p>
      </div>
      {isLoading ? (
        <p className="text-[12px] text-text-3">Loading…</p>
      ) : isError ? (
        <p className="text-[12px] text-danger">Couldn't load experiments.</p>
      ) : rows.length === 0 ? (
        <p className="text-[12px] text-text-3">No experiments recorded for this cycle.</p>
      ) : (
        <ul className="mt-2 divide-y divide-border-soft">
          {rows.map((n) => (
            <li key={n.bundle_hash} className="flex items-center gap-3 py-2">
              <HashSigil hash={n.bundle_hash} size={32} />
              <Link
                to={`/optimizer/experiment/${encodeURIComponent(n.bundle_hash)}`}
                className="font-mono text-[12px] text-text hover:text-gold"
              >
                {n.bundle_hash.slice(0, 10)}
              </Link>
              <span className="ml-auto flex items-center gap-3">
                {n.diversity_score != null ? (
                  <span className="font-mono text-[11px] text-text-3">div {n.diversity_score.toFixed(2)}</span>
                ) : null}
                <GateBadge verdict={formatGateVerdict(n.gate_verdict)} status={n.status} />
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
