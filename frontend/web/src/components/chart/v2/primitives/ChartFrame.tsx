import { ReactNode, useState } from "react";

export type RangePreset = "1d" | "1w" | "1m" | "3m" | "All";

const RANGE_PRESETS: RangePreset[] = ["1d", "1w", "1m", "3m", "All"];

type Props = {
  title?: string;
  range: RangePreset;
  onRange: (r: RangePreset) => void;
  layersPanel?: ReactNode;
  dataTable?: ReactNode;
  children: ReactNode;
};

export function ChartFrame({
  title,
  range,
  onRange,
  layersPanel,
  dataTable,
  children,
}: Props) {
  const [layersOpen, setLayersOpen] = useState(false);
  const [tableOpen, setTableOpen] = useState(false);

  return (
    <div className="border border-border rounded-card bg-surface-card">
      {/* Title / controls row */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        {title && (
          <span className="text-[13px] text-text-2 font-medium mr-2">{title}</span>
        )}

        {/* Range toggles */}
        {RANGE_PRESETS.map((r) => (
          <button
            key={r}
            type="button"
            onClick={() => onRange(r)}
            className={`px-2 py-0.5 text-[12px] rounded transition-colors ${
              range === r
                ? "bg-surface-elev text-text"
                : "text-text-3 hover:text-text-2"
            }`}
          >
            {r}
          </button>
        ))}

        {/* Right controls */}
        <div className="ml-auto flex items-center gap-2">
          {layersPanel !== undefined && (
            <button
              type="button"
              aria-pressed={layersOpen}
              onClick={() => setLayersOpen((v) => !v)}
              className="text-[12px] text-text-3 hover:text-text-2 transition-colors"
            >
              Layers {layersOpen ? "▴" : "▾"}
            </button>
          )}
          {dataTable !== undefined && (
            <button
              type="button"
              aria-pressed={tableOpen}
              onClick={() => setTableOpen((v) => !v)}
              className="text-[12px] text-text-3 hover:text-text-2 transition-colors"
            >
              Data table
            </button>
          )}
        </div>
      </div>

      {/* Inline layers expand — NOT a modal/popup; renders in flow below the toolbar */}
      {layersOpen && layersPanel !== undefined && (
        <div className="border-b border-border bg-surface-elev px-4 py-3">
          {layersPanel}
        </div>
      )}

      {/* Canvas / chart area */}
      <div className="relative">{children}</div>

      {/* Data table expand */}
      {tableOpen && dataTable !== undefined && (
        <div className="border-t border-border overflow-x-auto">
          {dataTable}
        </div>
      )}
    </div>
  );
}
