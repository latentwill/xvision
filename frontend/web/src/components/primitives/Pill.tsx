import type { HTMLAttributes, ReactNode } from "react";

type Tone = "default" | "gold" | "solid" | "danger" | "warn" | "info";

const TONE_CLASSES: Record<Tone, string> = {
  default: "border-border-soft text-text-2",
  gold: "border-gold/40 text-gold",
  solid: "bg-gold border-gold text-bg font-medium",
  danger: "border-danger/40 text-danger",
  warn: "border-warn/40 text-warn",
  info: "border-info/40 text-info",
};

export function Pill({
  tone = "default",
  animated = false,
  children,
  className = "",
  ...rest
}: {
  tone?: Tone;
  animated?: boolean;
  children: ReactNode;
} & HTMLAttributes<HTMLSpanElement>) {
  const animatedClass = animated ? "xvn-pill-animated" : "";
  const busy = animated ? { "aria-busy": true as const } : undefined;
  return (
    <span
      data-running={animated || undefined}
      className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-sm text-[11px] tracking-wide whitespace-nowrap border ${TONE_CLASSES[tone]} ${animatedClass} ${className}`}
      {...busy}
      {...rest}
    >
      {children}
    </span>
  );
}
