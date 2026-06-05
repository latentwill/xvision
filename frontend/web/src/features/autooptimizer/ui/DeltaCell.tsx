type DeltaState = "done" | "running" | "queued" | "failed";

// Fix 5: static Tailwind class map — dynamic `bg-${tone}/[0.xx]` strings are
// not picked up by the JIT scanner, so the opacity classes disappear from the
// production CSS bundle.  All class strings here are literals so the scanner
// can find them.
const TINT: Record<"gold" | "danger", Record<"low" | "mid" | "high", string>> =
  {
    gold: {
      low: "bg-gold/[0.08]",
      mid: "bg-gold/[0.16]",
      high: "bg-gold/[0.26]",
    },
    danger: {
      low: "bg-danger/[0.08]",
      mid: "bg-danger/[0.16]",
      high: "bg-danger/[0.26]",
    },
  };

function bucket(abs: number): "low" | "mid" | "high" {
  return abs >= 0.67 ? "high" : abs >= 0.33 ? "mid" : "low";
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
  // Non-done states, null/undefined delta, or non-finite delta all render as
  // a neutral placeholder.  `delta === 0` is a valid result and falls through
  // to the tinted done-cell below.
  if (state !== "done" || delta == null || !Number.isFinite(delta)) {
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
  const tone = positive ? "gold" : "danger" as const;
  // |Δ| of 0.5 or more maps to full tint intensity
  const intensity = Math.min(1, Math.abs(delta) / 0.5);
  const borderClass = positive ? "border-gold/40" : "border-danger/40";
  const textClass = positive ? "text-gold" : "text-danger";
  const bg = TINT[tone][bucket(intensity)];

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
