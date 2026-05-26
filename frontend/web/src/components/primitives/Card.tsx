import type { HTMLAttributes, ReactNode } from "react";

export function Card({
  children,
  className = "",
  ...rest
}: { children: ReactNode } & HTMLAttributes<HTMLDivElement>) {
  // `min-w-0` is the load-bearing default: cards live inside flex / grid
  // tracks and their unbreakable inner content (mono IDs, code blocks, long
  // titles) would otherwise push the track wider than its allotted width and
  // overlap the next column. Callers can still override via className.
  return (
    <div
      className={`min-w-0 bg-surface-card border border-border rounded-card ${className}`}
      {...rest}
    >
      {children}
    </div>
  );
}

export function CardHeader({
  title,
  actions,
  className = "",
}: {
  title: ReactNode;
  actions?: ReactNode;
  className?: string;
}) {
  // `min-w-0` on the flex container + `min-w-0 truncate` on the title block
  // keep long titles from blowing past the right-hand actions cluster.
  return (
    <div
      className={`flex items-center justify-between gap-3 px-5 pt-4 pb-3 min-w-0 ${className}`}
    >
      <h2 className="min-w-0 m-0 font-sans font-medium text-[22px] tracking-tight truncate">
        {title}
      </h2>
      {actions != null ? (
        <div className="flex items-center gap-2 shrink-0">{actions}</div>
      ) : null}
    </div>
  );
}
