import { ReactNode, useState } from "react";

export type RangePreset =
  | "1h"
  | "4h"
  | "6h"
  | "12h"
  | "1d"
  | "1w"
  | "All";

const RANGE_PRESETS: RangePreset[] = [
  "1h",
  "4h",
  "6h",
  "12h",
  "1d",
  "1w",
  "All",
];

export const CHART_V2_ZOOM_EVENT = "xvn:chart-v2:zoom";
export const CHART_V2_RANGE_EVENT = "xvn:chart-v2:range";

type Props = {
  title?: string;
  range: RangePreset;
  onRange: (r: RangePreset) => void;
  /**
   * When `false`, the range preset buttons are not rendered.
   * Fallback for surfaces where a finite visible window can't be supported.
   * Defaults to `true`.
   */
  rangeEnabled?: boolean;
  /**
   * Override the visible preset list. Surfaces with known candle granularity
   * pass a filtered subset so presets shorter than the bar interval (which
   * all collapse to a single visible candle) aren't shown.
   */
  presets?: RangePreset[];
  layersPanel?: ReactNode;
  dataTable?: ReactNode;
  children: ReactNode;
};

export function ChartFrame({
  title,
  range,
  onRange,
  rangeEnabled = true,
  presets,
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
        {rangeEnabled &&
          (presets ?? RANGE_PRESETS).map((r) => (
            <button
              key={r}
              type="button"
              onClick={() => {
                onRange(r);
                window.dispatchEvent(
                  new CustomEvent(CHART_V2_RANGE_EVENT, { detail: r }),
                );
              }}
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
          <button
            type="button"
            aria-label="Zoom in chart"
            onClick={() =>
              window.dispatchEvent(
                new CustomEvent(CHART_V2_ZOOM_EVENT, { detail: "in" }),
              )
            }
            className="inline-flex h-6 w-6 items-center justify-center rounded border border-border text-[14px] leading-none text-text-3 hover:text-text-2 transition-colors"
          >
            +
          </button>
          <button
            type="button"
            aria-label="Zoom out chart"
            onClick={() =>
              window.dispatchEvent(
                new CustomEvent(CHART_V2_ZOOM_EVENT, { detail: "out" }),
              )
            }
            className="inline-flex h-6 w-6 items-center justify-center rounded border border-border text-[14px] leading-none text-text-3 hover:text-text-2 transition-colors"
          >
            -
          </button>
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
