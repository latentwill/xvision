type DeltaState = "done" | "running" | "queued" | "failed";

/** Map |Δ| intensity (0–1) to a Tailwind bg opacity bucket for gold or danger. */
function tintClass(positive: boolean, intensity: number): string {
  // Three buckets: low (<0.33), mid (<0.67), high (≥0.67)
  const tone = positive ? "gold" : "danger";
  if (intensity < 0.33) return `bg-${tone}/[0.08]`;
  if (intensity < 0.67) return `bg-${tone}/[0.16]`;
  return `bg-${tone}/[0.26]`;
}

export function DeltaCell({
  state,
  delta,
  sharpe,
}: {
  state: DeltaState;
  delta?: number;
  sharpe?: number;
}) {
  if (state !== "done" || delta == null) {
    const label =
      state === "running" ? "run…" : state === "failed" ? "retry" : "queued";
    const tone = state === "failed" ? "text-danger" : "text-text-3";
    return (
      <div
        className={`flex h-7 items-center justify-center rounded border border-border bg-surface-elev font-mono text-[10px] ${tone}`}
      >
        {label}
      </div>
    );
  }

  const positive = delta >= 0;
  // |Δ| of 0.5 or more maps to full tint intensity (1.0)
  const intensity = Math.min(1, Math.abs(delta) / 0.5);
  const borderClass = positive ? "border-gold/40" : "border-danger/40";
  const textClass = positive ? "text-gold" : "text-danger";
  const bg = tintClass(positive, intensity);

  return (
    <div
      className={`flex h-7 flex-col items-center justify-center rounded border ${borderClass} ${bg}`}
    >
      <span
        className={`font-mono text-[11px] font-semibold ${textClass} leading-none`}
      >
        {positive ? "+" : ""}
        {delta.toFixed(2)}
      </span>
      {sharpe != null ? (
        <span className="font-mono text-[9px] text-text-3 leading-none">
          {sharpe.toFixed(2)}
        </span>
      ) : null}
    </div>
  );
}
