import { Fragment, useState } from "react";
import { useLadder, type MutatorScore } from "../api";

function acceptRate(s: MutatorScore): number {
  return s.proposals > 0 ? s.accepted / s.proposals : 0;
}

export function ExperimentWritersPanel() {
  const { data, isLoading, isError } = useLadder();
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const rows = [...(data ?? [])].sort(
    (a, b) => b.avg_delta_sharpe - a.avg_delta_sharpe,
  );

  function toggle(key: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

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
                const key = `${s.provider}/${s.model}/${s.prompt_version}`;
                const regionId = `writer-detail-${key.replace(/\//g, "-").replace(/[^a-zA-Z0-9-]/g, "")}`;
                const rate = acceptRate(s);
                const isOpen = expanded.has(key);

                return (
                  <Fragment key={key}>
                    <tr className="border-t border-border-soft">
                      <td className="py-1.5 pr-3" colSpan={5}>
                        <button
                          type="button"
                          aria-expanded={isOpen}
                          aria-controls={regionId}
                          onClick={() => toggle(key)}
                          className="flex w-full items-center gap-2 text-left"
                        >
                          <span
                            className="shrink-0 text-[10px] text-text-3 transition-transform"
                            aria-hidden="true"
                            style={{ display: "inline-block", transform: isOpen ? "rotate(90deg)" : "rotate(0deg)" }}
                          >
                            ▸
                          </span>
                          <span className="flex flex-1 items-center gap-0">
                            <span className="text-text">{s.model}</span>
                            <span className="ml-1.5 text-[10px] text-text-3">{s.provider} · {s.prompt_version}</span>
                          </span>
                          <span className="ml-auto flex gap-6 pr-0 tabular-nums text-[12px]">
                            <span className="w-14 text-right text-text-2">{s.proposals}</span>
                            <span className="w-14 text-right text-gold">{s.accepted}</span>
                            <span className={`w-14 text-right ${rate >= 0.5 ? "text-gold" : rate >= 0.25 ? "text-text" : "text-text-3"}`}>
                              {Math.round(rate * 100)}%
                            </span>
                            <span className={`w-16 text-right ${s.avg_delta_sharpe >= 0 ? "text-gold" : "text-danger"}`}>
                              {s.avg_delta_sharpe >= 0 ? "+" : ""}{s.avg_delta_sharpe.toFixed(2)}
                            </span>
                          </span>
                        </button>
                      </td>
                    </tr>
                    {isOpen && (
                      <tr className="bg-surface-elev/40">
                        <td
                          id={regionId}
                          role="region"
                          aria-label={`${s.model} details`}
                          colSpan={5}
                          className="pb-3 pl-6 pr-3 pt-2"
                        >
                          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[11px]">
                            <dt className="text-text-3">Prompt version</dt>
                            <dd className="font-mono text-text">{s.prompt_version}</dd>

                            <dt className="text-text-3">Proposals</dt>
                            <dd className="font-mono text-text">{s.proposals}</dd>

                            <dt className="text-text-3">Accepted</dt>
                            <dd className="font-mono text-text">{s.accepted}</dd>

                            <dt className="text-text-3">Rejected (overfit)</dt>
                            <dd className="font-mono text-text">{s.rejected_overfit}</dd>

                            <dt className="text-text-3">Accept rate</dt>
                            <dd className="font-mono text-text">{Math.round(rate * 100)}%</dd>

                            <dt className="text-text-3">Avg ΔSharpe</dt>
                            <dd className={`font-mono ${s.avg_delta_sharpe >= 0 ? "text-gold" : "text-danger"}`}>
                              {s.avg_delta_sharpe >= 0 ? "+" : ""}{s.avg_delta_sharpe.toFixed(2)}
                            </dd>
                          </dl>
                          <p className="mt-2 text-[10px] text-text-3">
                            Per-experiment provenance for this writer arrives with the regime-matrix backend (Phase 2).
                          </p>
                        </td>
                      </tr>
                    )}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
