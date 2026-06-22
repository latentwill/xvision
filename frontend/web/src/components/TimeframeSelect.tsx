import { SignalSelectMenu, type SelectOption } from "@/components/primitives/SignalMenu";
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
  const options: SelectOption[] = [
    ...(!hasStandard && Number.isFinite(valueMinutes) && valueMinutes > 0
      ? [
          {
            value: String(valueMinutes),
            label: `Custom · ${formatCadence(valueMinutes)}`,
          },
        ]
      : []),
    ...STANDARD_TIMEFRAMES.map((tf) => ({
      value: String(tf.minutes),
      label: tf.label,
    })),
  ];

  return (
    <SignalSelectMenu
      ariaLabel={ariaLabel}
      value={Number.isFinite(valueMinutes) ? String(valueMinutes) : ""}
      options={options}
      onChange={(next) => onChange(Number(next))}
      disabled={disabled}
      className={className}
      minWidth={120}
    />
  );
}
