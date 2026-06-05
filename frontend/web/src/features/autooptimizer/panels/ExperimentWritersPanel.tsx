import { useLadder, type MutatorScore } from "../api";

function acceptRate(s: MutatorScore): number {
  return s.proposals > 0 ? s.accepted / s.proposals : 0;
}

export function ExperimentWritersPanel() {
  const { data, isLoading, isError } = useLadder();
  const rows = [...(data ?? [])].sort(
    (a, b) => b.avg_delta_sharpe - a.avg_delta_sharpe,
  );

  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <div className="mb-3 flex items-center justify-between">
        <div>
          <h2 className="m-0 text-[15px] font-semibold tracking-tight">Experiment writers</h2>
          <p className="mt-0.5 text-[12px] text-text-3">
            which writer model proposes the best-accepted experiments
          </p>
        </div>
      </div>

      {isLoading ? (
        <p className="text-[12px] text-text-3">Loading…</p>
      ) : isError ? (
        <p className="text-[12px] text-danger">Couldn't load writer ladder.</p>
      ) : rows.length === 0 ? (
        <p className="text-[12px] text-text-3">No experiment writers have run yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-[12px]">
            <thead>
              <tr className="text-left text-text-3">
                <th className="py-1.5 pr-3 font-medium">Writer</th>
                <th className="py-1.5 pr-3 text-right font-medium">Proposals</th>
                <th className="py-1.5 pr-3 text-right font-medium">Accepted</th>
                <th className="py-1.5 pr-3 text-right font-medium">Accept %</th>
                <th className="py-1.5 text-right font-medium">Avg ΔSharpe</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {rows.map((s) => {
                const rate = acceptRate(s);
                return (
                  <tr key={`${s.provider}/${s.model}/${s.prompt_version}`} className="border-t border-border-soft">
                    <td className="py-1.5 pr-3">
                      <span className="text-text">{s.model}</span>
                      <span className="ml-1.5 text-[10px] text-text-3">{s.provider} · {s.prompt_version}</span>
                    </td>
                    <td className="py-1.5 pr-3 text-right text-text-2">{s.proposals}</td>
                    <td className="py-1.5 pr-3 text-right text-gold">{s.accepted}</td>
                    <td className={`py-1.5 pr-3 text-right ${rate >= 0.5 ? "text-gold" : rate >= 0.25 ? "text-text" : "text-text-3"}`}>
                      {Math.round(rate * 100)}%
                    </td>
                    <td className={`py-1.5 text-right ${s.avg_delta_sharpe >= 0 ? "text-gold" : "text-danger"}`}>
                      {s.avg_delta_sharpe >= 0 ? "+" : ""}{s.avg_delta_sharpe.toFixed(2)}
                    </td>
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
