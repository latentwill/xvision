export function GateBuckets({
  kept,
  suspect,
  dropped,
}: {
  kept: number;
  suspect: number;
  dropped: number;
}) {
  const total = kept + suspect + dropped;

  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <h2 className="m-0 mb-1 text-[15px] font-semibold tracking-tight">Anti-overfit gate</h2>
      <p className="mb-4 text-[11px] text-text-3">
        Kept = positive Δ-Sharpe in ≥1 bull AND ≥1 bear/shock regime · Suspect = single-regime
        evidence · Dropped = fails
      </p>
      <div className="grid grid-cols-3 gap-3">
        {/* Kept */}
        <div className="flex flex-col items-center rounded border border-gold/40 bg-gold/[0.08] px-3 py-3">
          <span className="font-mono text-2xl font-semibold text-gold">{kept}</span>
          <span className="mt-1 text-[12px] font-medium text-gold">Kept</span>
        </div>
        {/* Suspect */}
        <div className="flex flex-col items-center rounded border border-warn/40 bg-warn/[0.08] px-3 py-3">
          <span className="font-mono text-2xl font-semibold text-warn">{suspect}</span>
          <span className="mt-1 text-[12px] font-medium text-warn">Suspect</span>
        </div>
        {/* Dropped */}
        <div className="flex flex-col items-center rounded border border-danger/40 bg-danger/[0.08] px-3 py-3">
          <span className="font-mono text-2xl font-semibold text-danger">{dropped}</span>
          <span className="mt-1 text-[12px] font-medium text-danger">Dropped</span>
        </div>
      </div>
      {total > 0 && (
        <p className="mt-3 text-right text-[11px] text-text-3">
          {total} total
        </p>
      )}
    </section>
  );
}
