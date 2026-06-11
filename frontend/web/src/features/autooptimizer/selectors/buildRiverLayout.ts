import type { RiverNode } from "../api";

export type RiverPoint = { hash: string; x: number; y: number; cycleId: string | null };
export type RiverLine = { points: RiverPoint[]; champion: boolean; alive: boolean };
export type RiverStub = {
  hash: string;
  fromX: number;
  fromY: number;
  y: number;
  kind: "rejected" | "suspect";
  delta: number | null;
  cycleId: string | null;
  /** 0 = oldest … 1 = newest, by created_at; renderer maps to opacity (fade with age) */
  ageRank: number;
};
export type RiverLayout = {
  lines: RiverLine[];
  stubs: RiverStub[];
  xMax: number;
  yDomain: [number, number];
};

const DEFAULT_Y = 1.0;

function yOf(n: RiverNode): number {
  return n.child_day_score ?? DEFAULT_Y;
}

/**
 * Walk a keep-chain starting from `start`, recording rejected/quarantined
 * children as stubs. Returns the resulting RiverLine (champion/alive set later).
 */
function walkLine(
  start: RiverNode,
  x0: number,
  childrenOf: Map<string, RiverNode[]>,
  pos: Map<string, RiverPoint>,
  stubs: RiverStub[],
): RiverLine {
  const points: RiverPoint[] = [];
  let cur: RiverNode | undefined = start;
  let x = x0;
  while (cur) {
    const p: RiverPoint = { hash: cur.bundle_hash, x, y: yOf(cur), cycleId: cur.cycle_id };
    pos.set(cur.bundle_hash, p);
    points.push(p);
    const kids: RiverNode[] = childrenOf.get(cur.bundle_hash) ?? [];
    // Rejected/quarantined children become stubs off this point
    for (const k of kids) {
      if (k.status === "rejected" || k.status === "quarantined") {
        stubs.push({
          hash: k.bundle_hash,
          fromX: x,
          fromY: p.y,
          y: yOf(k),
          kind: k.status === "rejected" ? "rejected" : "suspect",
          delta: k.delta_day,
          cycleId: k.cycle_id,
          ageRank: 0, // filled in after the full walk
        });
      }
    }
    // Active children: the first (by created_at) continues this line;
    // additional active children start new sub-lines (handled by the caller
    // via the root loop — walkLine itself does not recurse further to avoid
    // deep-recursion ordering issues the plan notes).
    const activeKids = kids
      .filter((k) => k.status === "active")
      .sort((a, b) => a.created_at.localeCompare(b.created_at));
    cur = activeKids[0];
    x += 1;
  }
  return { points, champion: false, alive: true };
}

export function buildRiverLayout(nodes: RiverNode[]): RiverLayout {
  if (nodes.length === 0) return { lines: [], stubs: [], xMax: 0, yDomain: [0, 2] };

  // Index nodes and build parent → children map
  const byHash = new Map(nodes.map((n) => [n.bundle_hash, n]));
  const childrenOf = new Map<string, RiverNode[]>();
  const roots: RiverNode[] = [];

  for (const n of nodes) {
    const parentKnown = n.parent_hash != null && byHash.has(n.parent_hash);
    if (parentKnown) {
      const key = n.parent_hash!;
      const arr = childrenOf.get(key) ?? [];
      arr.push(n);
      childrenOf.set(key, arr);
    } else {
      roots.push(n);
    }
  }

  const pos = new Map<string, RiverPoint>();
  const lines: RiverLine[] = [];
  const stubs: RiverStub[] = [];

  // Process each root. At each node in the chain, extra active children
  // beyond the first start new sub-lines via walkLine.
  for (const root of roots) {
    // Walk the primary chain for this root
    const primaryLine = walkLine(root, 0, childrenOf, pos, stubs);
    lines.push(primaryLine);

    // For every point in the primary line, check for extra active children
    // that need their own sub-lines
    for (const p of primaryLine.points) {
      const n = byHash.get(p.hash)!;
      const kids = childrenOf.get(n.bundle_hash) ?? [];
      const activeKids = kids
        .filter((k) => k.status === "active")
        .sort((a, b) => a.created_at.localeCompare(b.created_at));
      // The first active child is already part of the primary chain (walked above).
      // Extra active children beyond the first get their own sub-lines.
      for (const extra of activeKids.slice(1)) {
        const subLine = walkLine(extra, p.x + 1, childrenOf, pos, stubs);
        lines.push(subLine);
      }
    }
  }

  // ageRank: normalized recency of each stub's created_at over the dataset's time span
  const times = nodes.map((n) => Date.parse(n.created_at)).filter(Number.isFinite);
  const t0 = Math.min(...times);
  const t1 = Math.max(...times);
  for (const s of stubs) {
    const stubNode = byHash.get(s.hash);
    const t = stubNode ? Date.parse(stubNode.created_at) : t0;
    s.ageRank = t1 === t0 ? 1 : (t - t0) / (t1 - t0);
  }

  // alive: a line is alive if its tip falls in the newest 25% of the overall
  // dataset time span (or all times are equal → everything is alive).
  for (const l of lines) {
    const tip = l.points.at(-1);
    if (!tip) {
      l.alive = false;
      continue;
    }
    const tipNode = byHash.get(tip.hash);
    const tipT = tipNode ? Date.parse(tipNode.created_at) : t0;
    l.alive = t1 === t0 || tipT >= t1 - (t1 - t0) * 0.25;
  }

  // yDomain: min/max of all y values with padding; guard against min===max (degenerate)
  const allY = [
    ...lines.flatMap((l) => l.points.map((p) => p.y)),
    ...stubs.map((s) => s.y),
  ];
  const yMin = Math.min(...allY);
  const yMax = Math.max(...allY);
  const pad = Math.max(0.1, (yMax - yMin) * 0.15);
  const xMax = Math.max(...lines.flatMap((l) => l.points.map((p) => p.x)), 0);

  // champion: among alive lines, the one whose tip has the highest y score
  const liveLines = lines.filter((l) => l.alive && l.points.length > 0);
  if (liveLines.length > 0) {
    const champ = liveLines.reduce((best, l) => {
      const bY = best.points.at(-1)!.y;
      const lY = l.points.at(-1)!.y;
      return lY > bY ? l : best;
    });
    champ.champion = true;
  }

  return { lines, stubs, xMax, yDomain: [yMin - pad, yMax + pad] };
}
