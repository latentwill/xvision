import type { InlineTone } from "./types";

export type ToneColors = {
  stroke: string;
  fill: string;
  text: string;
};

export function toneColors(tone: InlineTone | null | undefined): ToneColors {
  switch (tone) {
    case "gold":
      return { stroke: "#e2b84b", fill: "rgba(226,184,75,0.16)", text: "text-gold" };
    case "info":
      return { stroke: "#7aa7ff", fill: "rgba(122,167,255,0.14)", text: "text-sky-300" };
    case "warn":
      return { stroke: "#f0a84a", fill: "rgba(240,168,74,0.14)", text: "text-amber-300" };
    case "danger":
      return { stroke: "#f87171", fill: "rgba(248,113,113,0.14)", text: "text-rose-300" };
    case "muted":
      return { stroke: "#7a7569", fill: "rgba(122,117,105,0.12)", text: "text-text-3" };
    case "default":
    default:
      return { stroke: "#d2cec4", fill: "rgba(210,206,196,0.12)", text: "text-text" };
  }
}

export const SERIES_TONES: InlineTone[] = ["gold", "info", "warn", "danger"];
