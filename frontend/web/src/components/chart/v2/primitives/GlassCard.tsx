/**
 * GlassCard — the B4 dashboard's `.glass` utility card. Subtle
 * gradient surface + 1px translucent border + backdrop blur + inset
 * highlight on the top edge.
 *
 * Renders a `<div>` so it can wrap any content. Pass `className` to
 * extend layout (padding, grid spans, …) without overriding the
 * chrome.
 */
import type { ReactElement, ReactNode } from "react";

export interface GlassCardProps {
  children: ReactNode;
  className?: string;
}

export function GlassCard({
  children,
  className = "",
}: GlassCardProps): ReactElement {
  return (
    <div
      className={[
        "relative rounded-card overflow-hidden",
        className,
      ].join(" ")}
      style={{
        background:
          "linear-gradient(180deg, rgba(34,30,20,0.62), rgba(20,18,14,0.78))",
        border: "1px solid rgba(241,236,221,0.07)",
        backdropFilter: "blur(8px)",
        WebkitBackdropFilter: "blur(8px)",
        boxShadow: "inset 0 1px 0 rgba(255,255,255,0.02)",
      }}
      data-testid="glass-card"
    >
      {children}
    </div>
  );
}
