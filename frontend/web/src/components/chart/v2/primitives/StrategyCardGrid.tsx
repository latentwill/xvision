import type { ReactNode } from "react";

type Props = {
  count: number;
  children: ReactNode;
};

export function strategyGridColumns(count: number): number {
  if (count <= 2) return 2;
  if (count <= 4) return 4;
  if (count <= 6) return 3;
  return 4;
}

export function StrategyCardGrid({ count, children }: Props) {
  const columns = strategyGridColumns(count);
  return (
    <div
      className="grid gap-3"
      style={{
        gridTemplateColumns: `repeat(auto-fit, minmax(min(190px, 100%), 1fr))`,
      }}
      data-testid="strategy-card-grid"
      data-columns={columns}
    >
      {children}
    </div>
  );
}
