import { ReactNode, useState } from 'react';

export type RangePreset = '1d' | '1w' | '1m' | '3m' | 'All';

type Props = {
  title?: string;
  range: RangePreset;
  onRange: (r: RangePreset) => void;
  layersPanel: ReactNode;
  children: ReactNode;
  dataTable?: ReactNode;
};

export function ChartContainer({
  title,
  range,
  onRange,
  layersPanel,
  children,
  dataTable,
}: Props) {
  const [layersOpen, setLayersOpen] = useState(false);
  const [tableOpen, setTableOpen] = useState(false);
  return (
    <div className="border border-border rounded-card">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        {title && (
          <span className="text-[13px] text-text-2 font-medium mr-2">{title}</span>
        )}
        {(['1d', '1w', '1m', '3m', 'All'] as RangePreset[]).map((r) => (
          <button
            key={r}
            type="button"
            onClick={() => onRange(r)}
            className={`px-2 py-0.5 text-[12px] rounded ${
              range === r ? 'bg-surface-elev text-text' : 'text-text-3 hover:text-text-2'
            }`}
          >
            {r}
          </button>
        ))}
        <div className="ml-auto flex items-center gap-2">
          <button
            type="button"
            onClick={() => setLayersOpen((v) => !v)}
            className="text-[12px] text-text-3 hover:text-text-2"
          >
            Layers ▾
          </button>
          {dataTable ? (
            <button
              type="button"
              aria-pressed={tableOpen}
              onClick={() => setTableOpen((v) => !v)}
              className="text-[12px] text-text-3 hover:text-text-2"
            >
              Data table
            </button>
          ) : null}
        </div>
      </div>
      <div className="relative">
        {children}
        {layersOpen && (
          <div className="absolute right-2 top-2 z-10 w-64 max-h-[80vh] overflow-auto border border-border bg-surface-card rounded-card shadow-lg p-3 text-[12px]">
            {layersPanel}
            <button
              type="button"
              onClick={() => setLayersOpen(false)}
              className="mt-3 text-text-3 hover:text-text-2"
            >
              Close
            </button>
          </div>
        )}
      </div>
      {tableOpen && dataTable ? (
        <div className="border-t border-border overflow-x-auto p-3">
          {dataTable}
        </div>
      ) : null}
    </div>
  );
}
