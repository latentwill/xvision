import { type V2Marker } from "../types";

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
