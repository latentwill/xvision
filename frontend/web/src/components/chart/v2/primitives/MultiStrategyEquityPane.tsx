import type { EquityPoint } from "../types";
import { Legend } from "./Legend";
import { PaneStack } from "./PaneStack";
import { UplotCompareOverlayPane } from "./UplotCompareOverlayPane";

export type MultiStrategyEquityArm = {
  id: string;
  label: string;
  color: string;
  equity: EquityPoint[];
  returnPct?: number | null;
};

type Props = {
  arms: MultiStrategyEquityArm[];
  height?: number;
};

export function MultiStrategyEquityPane({ arms, height = 300 }: Props) {
  return (
    <div>
      <PaneStack syncKey="compare-ab-dashboard">
        <UplotCompareOverlayPane arms={arms} height={height} />
      </PaneStack>
      <div className="border-t border-border px-3 py-2">
        <Legend
          items={arms.map((arm) => ({
            label: `${arm.label} ${fmtReturn(arm.returnPct)}`,
            color: arm.color,
            title: arm.id,
          }))}
        />
      </div>
    </div>
  );
}

function fmtReturn(n: number | null | undefined): string {
  if (n == null) return "";
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(1)}%`;
}
