import { type V2Marker } from "../types";
import { type Chart2ThemeDefinition } from "../../../../theme/themes";

/**
 * Convert V2Marker[] to KlineCharts overlay descriptor objects.
 * Uses `unknown[]` to avoid importing the deep KlineCharts overlay types;
 * KlineCandlePane registers these via chart.createOverlay().
 */
export function v2MarkersToKlineOverlay(
  markers: V2Marker[],
  theme: Chart2ThemeDefinition,
): unknown[] {
  return markers.map((marker) => ({
    name: "v2Marker",
    points: [
      {
        timestamp: marker.time * 1000,
        value: marker.price ?? null,
      },
    ],
    extendData: {
      kind: marker.kind,
      text: marker.text ?? "",
      color: theme.marker[marker.kind],
    },
  }));
}

export type MarkerDockEntry = {
  id: string;
  kind: V2Marker["kind"];
  time: number;
  text: string;
};

/**
 * Convert V2Marker[] to lightweight dock entries, sorted ascending by time.
 */
export function markersToDockEntries(markers: V2Marker[]): MarkerDockEntry[] {
  return markers
    .map((m, i) => ({
      id: `marker-${m.kind}-${m.time}-${i}`,
      kind: m.kind,
      time: m.time,
      text: m.text ?? "",
    }))
    .sort((a, b) => a.time - b.time);
}
