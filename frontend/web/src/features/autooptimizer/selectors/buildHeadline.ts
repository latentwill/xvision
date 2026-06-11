export type HeadlineInput = {
  state: "running" | "paused" | "cancelling" | "idle";
  activeLineages: number;
  lastCycle: { kept: number; total: number } | null;
  lastCycleAgo: string | null;
  bestFind?: { hash: string; delta: number } | null;
};

export type Headline = { title: string; subtitle: string };

export function buildHeadline(i: HeadlineInput): Headline {
  if (i.state === "running")
    return {
      title: "A run is in progress.",
      subtitle: `1 cycle running · ${i.activeLineages} active lineages.`,
    };

  if (i.state === "paused")
    return {
      title: "A run is paused.",
      subtitle: "Resume it to keep experimenting.",
    };

  if (i.state === "cancelling")
    return {
      title: "A run is cancelling.",
      subtitle: "Winding down in-flight experiments.",
    };

  // idle — check if the optimizer has ever run
  if (i.lastCycle && i.lastCycleAgo) {
    const bestPrefix = i.bestFind
      ? `Best find: ${i.bestFind.hash.slice(0, 8)} (ΔSharpe ${i.bestFind.delta >= 0 ? "+" : "−"}${Math.abs(i.bestFind.delta).toFixed(2)}) · `
      : "";
    return {
      title: `Last ran ${i.lastCycleAgo} — kept ${i.lastCycle.kept} of ${i.lastCycle.total} experiments.`,
      subtitle: `${bestPrefix}${i.activeLineages} active lineages.`,
    };
  }

  // idle + never ran
  return {
    title: "The optimizer hasn't run yet.",
    subtitle: "Launch its first cycle.",
  };
}
