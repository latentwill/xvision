import type { ReactNode } from "react";

export type MListRowBadgeColor = "gold" | "warn" | "danger" | "info" | "muted";

export type MListRowProps = {
  title: ReactNode;
  subtitle?: ReactNode;
  meta?: ReactNode;
  badge?: ReactNode;
  badgeColor?: MListRowBadgeColor;
  rightTop?: ReactNode;
  rightSub?: ReactNode;
  rightTone?: "default" | "gold" | "warn" | "danger" | "info";
  onClick?: () => void;
};

const BADGE_CLASSES: Record<MListRowBadgeColor, string> = {
  gold: "text-gold border-gold/40 bg-gold/10",
  warn: "text-warn border-warn/45 bg-warn/10",
  danger: "text-danger border-danger/40 bg-danger/10",
  info: "text-info border-info/45 bg-info/10",
  muted: "text-text-3 border-border-soft bg-transparent",
};

const RIGHT_TONE_CLASSES: Record<NonNullable<MListRowProps["rightTone"]>, string> = {
  default: "text-text",
  gold: "text-gold",
  warn: "text-warn",
  danger: "text-danger",
  info: "text-info",
};

export function MListRow({
  title,
  subtitle,
  meta,
  badge,
  badgeColor = "muted",
  rightTop,
  rightSub,
  rightTone = "default",
  onClick,
}: MListRowProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-start justify-between gap-3 w-full px-3.5 py-3 bg-surface-card border border-border rounded-lg text-left font-sans active:bg-surface-hover"
    >
      <div className="flex-1 min-w-0 flex flex-col gap-1">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="font-mono font-medium text-[13.5px] text-text truncate">
            {title}
          </span>
          {badge && (
            <span
              className={`inline-flex items-center gap-1.5 h-[18px] px-[7px] rounded-[3px] font-mono text-[9.5px] tracking-[0.08em] uppercase border ${BADGE_CLASSES[badgeColor]}`}
            >
              {badge}
            </span>
          )}
        </div>
        {subtitle && (
          <div className="font-mono text-[12px] text-text-2 truncate">
            {subtitle}
          </div>
        )}
        {meta && (
          <div className="font-mono text-[11px] text-text-3 mt-0.5 truncate">
            {meta}
          </div>
        )}
      </div>
      {(rightTop || rightSub) && (
        <div className="flex flex-col items-end gap-0.5 shrink-0">
          {rightTop && (
            <div
              className={`font-serif font-medium text-[15px] tracking-tight ${RIGHT_TONE_CLASSES[rightTone]}`}
            >
              {rightTop}
            </div>
          )}
          {rightSub && (
            <div className="font-mono text-[11px] text-text-3">{rightSub}</div>
          )}
        </div>
      )}
    </button>
  );
}
