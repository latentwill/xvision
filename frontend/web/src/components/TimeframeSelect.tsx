import { formatCadence } from "@/lib/format";

export const STANDARD_TIMEFRAMES = [
  { label: "1m", minutes: 1 },
  { label: "5m", minutes: 5 },
  { label: "15m", minutes: 15 },
  { label: "30m", minutes: 30 },
  { label: "1h", minutes: 60 },
  { label: "4h", minutes: 240 },
  { label: "1d", minutes: 1440 },
] as const;

export function TimeframeSelect({
  valueMinutes,
  onChange,
  className,
  ariaLabel = "Time frame",
  disabled = false,
}: {
  valueMinutes: number;
  onChange: (minutes: number) => void;
  className?: string;
  ariaLabel?: string;
  disabled?: boolean;
}) {
  const hasStandard = STANDARD_TIMEFRAMES.some(
    (tf) => tf.minutes === valueMinutes,
  );
  return (
    <select
      aria-label={ariaLabel}
      value={Number.isFinite(valueMinutes) ? String(valueMinutes) : ""}
      onChange={(e) => onChange(Number(e.target.value))}
      disabled={disabled}
      className={className}
    >
      {!hasStandard && Number.isFinite(valueMinutes) && valueMinutes > 0 ? (
        <option value={String(valueMinutes)}>
          Custom · {formatCadence(valueMinutes)}
        </option>
      ) : null}
      {STANDARD_TIMEFRAMES.map((tf) => (
        <option key={tf.label} value={String(tf.minutes)}>
          {tf.label}
        </option>
      ))}
    </select>
  );
}
