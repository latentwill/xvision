/**
 * KpiCard — one cell in the dashboard's 5-up KPI row.
 *
 * Typography per spec §4A.1 (B1): value in Cormorant 30–32px, label in
 * the `.caps` tracked-uppercase eyebrow, foot in JetBrains Mono 11px
 * tinted to `text-3`. The `intent="danger"` variant tints the value red
 * (used by Max Drawdown). The `cornerGlow` prop is wired up here so B4
 * can apply the radial gold glow to the Total Return card.
 *
 * `KpiRow` is a thin layout wrapper that arranges N `KpiCard`s in a
 * responsive 1 → 3 → 5 column grid.
 */
import type { ReactElement, ReactNode } from "react";

export type KpiIntent = "default" | "danger";

export interface KpiCardProps {
  label: string;
  value: ReactNode;
  foot?: ReactNode;
  intent?: KpiIntent;
  /** Optional radial glow overlay (B4: gold corner glow on the Total
   *  Return card). Drawn as a 120×120 `radial-gradient` positioned at
   *  the top-right corner. Defaults to no overlay. */
  cornerGlow?: "gold" | null;
}

export function KpiCard({
  label,
  value,
  foot,
  intent = "default",
  cornerGlow = null,
}: KpiCardProps): ReactElement {
  const valueColor =
    intent === "danger" ? "text-danger" : "text-text";
  return (
    <div className="relative overflow-hidden border border-border rounded-card bg-surface-card p-[14px] min-h-[100px]">
      {cornerGlow === "gold" && (
        <div
          aria-hidden="true"
          className="pointer-events-none absolute -top-[30px] -right-[30px] w-[120px] h-[120px]"
          style={{
            background:
              "radial-gradient(closest-side, rgba(212,165,71,0.30), transparent 70%)",
          }}
        />
      )}
      <div className="caps">{label}</div>
      <div
        className={[
          "mt-2 leading-[1.05] tracking-[-0.015em] font-serif font-medium",
          "text-[30px]",
          valueColor,
        ].join(" ")}
        style={{ fontFamily: '"Cormorant Garamond", serif' }}
      >
        {value}
      </div>
      {foot != null && (
        <div
          className="mt-1.5 text-[11px] text-text-3"
          style={{ fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, monospace' }}
        >
          {foot}
        </div>
      )}
    </div>
  );
}

export interface KpiRowProps {
  children: ReactNode;
}

export function KpiRow({ children }: KpiRowProps): ReactElement {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 lg:grid-cols-5 gap-3">
      {children}
    </div>
  );
}
