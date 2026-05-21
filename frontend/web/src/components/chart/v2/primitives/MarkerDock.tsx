import { useChart2Theme } from "../hooks/useChart2Theme";
import type { V2Marker } from "../types";

type Props = {
  markers: V2Marker[];
  activeId?: string;
  onSelect?: (id: string) => void;
};

function markerIcon(kind: V2Marker["kind"]): string {
  switch (kind) {
    case "buy":
      return "▲";
    case "sell":
      return "▼";
    case "veto":
      return "⨯";
    case "hold":
      return "·";
  }
}

function markerId(marker: V2Marker, idx: number): string {
  return `${marker.kind}:${marker.decision_index ?? idx}`;
}

export function MarkerDock({ markers, activeId, onSelect }: Props) {
  const theme = useChart2Theme();

  const markerColor: Record<V2Marker["kind"], string> = {
    buy: theme.marker.buy,
    sell: theme.marker.sell,
    veto: theme.marker.veto,
    hold: theme.marker.hold,
  };

  if (markers.length === 0) {
    return (
      <div className="text-[11px] text-text-3 py-2 px-1">No markers</div>
    );
  }

  return (
    <div className="flex flex-col gap-0.5">
      {markers.map((marker, idx) => {
        const id = markerId(marker, idx);
        const isActive = activeId === id;
        const timeStr = new Date(marker.time * 1000).toLocaleTimeString();

        return (
          <button
            key={id}
            type="button"
            onClick={() => onSelect?.(id)}
            className={`flex items-start gap-2 px-2 py-1.5 rounded text-left w-full transition-colors ${
              isActive
                ? "bg-surface-elev text-text"
                : "text-text-2 hover:bg-surface-elev hover:text-text"
            }`}
          >
            {/* Kind icon */}
            <span
              className="text-[14px] leading-none mt-0.5 shrink-0 font-mono"
              style={{ color: markerColor[marker.kind] }}
              aria-label={marker.kind}
            >
              {markerIcon(marker.kind)}
            </span>

            {/* Detail */}
            <span className="min-w-0">
              <span className="block text-[11px] text-text-3">{timeStr}</span>
              {marker.text && (
                <span className="block text-[11px] text-text-2 truncate">
                  {marker.text}
                </span>
              )}
            </span>
          </button>
        );
      })}
    </div>
  );
}
