import type { RegimeResult } from "../api";

function formatDelta(v: number): string {
  const sign = v >= 0 ? "+" : "";
  return `${sign}${v.toFixed(2)}`;
}

function MicroMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col">
      <span className="text-[9px] uppercase tracking-wider text-text-3">{label}</span>
      <span className="font-mono text-[12px] text-text-1">{value}</span>
    </div>
  );
}

function RegimeCard({ result }: { result: RegimeResult }) {
  const { regime_label, side, delta_sharpe, metrics_day } = result;
  const deltaClass = delta_sharpe >= 0 ? "text-gold" : "text-danger";
  const winRatePct = `${Math.round(metrics_day.win_rate * 100)}%`;
  const retVal = `${metrics_day.total_return_pct.toFixed(1)}%`;
  const ddVal = `${metrics_day.max_drawdown_pct.toFixed(1)}%`;

  return (
    <div className="rounded-md border border-border bg-surface-card p-4">
      {/* Eyebrow: regime label (CSS uppercase); side surfaced via title tooltip only */}
      <div className="mb-2">
        <span
          className="text-[10px] uppercase tracking-widest text-text-3"
          title={side}
        >
          {regime_label}
        </span>
      </div>

      {/* Big Δ-Sharpe */}
      <div className={`mb-3 font-mono text-2xl font-semibold ${deltaClass}`}>
        {formatDelta(delta_sharpe)}
      </div>

      {/* 2×2 micro-grid */}
      <div className="grid grid-cols-2 gap-x-4 gap-y-2">
        <MicroMetric label="ret" value={retVal} />
        <MicroMetric label="dd" value={ddVal} />
        <MicroMetric label="winrt" value={winRatePct} />
        <MicroMetric label="trades" value={String(metrics_day.n_trades)} />
      </div>
    </div>
  );
}

export function RegimeCards({ results }: { results: RegimeResult[] }) {
  return (
    <section>
      <h2 className="m-0 mb-0.5 text-[15px] font-semibold tracking-tight">Per-regime evaluation</h2>
      <p className="mb-4 text-[11px] text-text-3">
        Δ-Sharpe, return, drawdown, win-rate and trades per regime.
      </p>

      {results.length === 0 ? (
        <p className="text-[12px] text-text-3">
          Lights up when the optimizer runs across a configured regime set.
        </p>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {results.map((r, i) => (
            <RegimeCard key={`${r.regime_label}-${i}`} result={r} />
          ))}
        </div>
      )}
    </section>
  );
}
