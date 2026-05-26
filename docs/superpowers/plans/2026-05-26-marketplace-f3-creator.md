# Marketplace F3 — Creator Profile + Lineage Forest

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `/marketplace/creator/:handleOrAddr` route (F3), replacing `MarketplaceCreatorStub` with a real page composed from frozen F0 primitives and the `getCreator(handleOrAddress)` seam. No new seam methods are added.

**Branch:** `feat/marketplace-f0` (F0 frozen; branch from that HEAD — DO NOT branch from `main`).

**Tech stack:** React 18, TypeScript, React-Router v6 (`useParams`), Vitest 2 + React Testing Library + jsdom, Tailwind token classes, pnpm. Inline SVG only — no uPlot, no chart library.

**Source material (all read before writing):**
- Types: `frontend/web/src/features/marketplace/data/types.ts` — `CreatorProfile`, `ForestNode`, `ForestEdge`, `AttestationActivity`, `CloneByEntry`, `ListingRow`, `BuyerCounts`
- Seam: `frontend/web/src/features/marketplace/data/MarketplaceData.ts` — `getCreator(handleOrAddress)`
- Fixture: `frontend/web/src/features/marketplace/data/fixtures/creators.ts` — `CREATORS["@ed"]`
- Provider: `frontend/web/src/features/marketplace/data/provider.tsx` — `useMarketplaceData()`
- F0 primitives: `GenArtPlaceholder`, `AssetPill`, `VerifiedBadge`, `X402Badge`, `AgentIcon`, `Sparkline` — all in `frontend/web/src/features/marketplace/components/`
- Design: `docs/design/design_handoff_marketplace_shift/bc2-creator.jsx` (the canonical reference; adapt, do not copy inline styles)
- Design README: `docs/design/design_handoff_marketplace_shift/README.md` §2 "Creator profile"
- Routing: `frontend/web/src/routes.tsx` line 62 (`MarketplaceCreatorStub`) + `frontend/web/src/features/marketplace/routes/stubs.tsx`
- Format precedent: `docs/superpowers/plans/2026-05-26-marketplace-phase-f0-foundation.md`

**Conventions:**
- Run tests from `frontend/web`: `pnpm exec vitest run <path>`. Typecheck: `pnpm typecheck`.
- Path alias `@/` → `src/`. Colocate tests as `*.test.ts(x)`.
- Token classes only — no hex literals, no inline `style={}` color values. Respect dark-mode border rules (`border-border`, never `border-white`/`border-gray-100`/`border-gray-200`).
- **No popups.** Everything inline or routed. Follow/Tip are deferred affordances — render as `disabled` buttons with a `title` tooltip. No `Dialog`/`Modal`/`Sheet`/`Popover`.
- Stage each file to its own `git add <explicit-paths>` (never `git add -A`). Commit per task.

**Scope note:** F3 builds three colocated components (`LineageForest`, `EarningsChart`) inside `routes/CreatorRoute.tsx` plus their test files. The `routes.tsx` change is a single-line import swap.

---

## File map

```
src/features/marketplace/routes/
  CreatorRoute.tsx               # main page + colocated LineageForest + EarningsChart (Tasks 1–4)
  CreatorRoute.test.tsx          # RTL tests for the full page (Task 5)
```

`src/routes.tsx` — one-line swap: `MarketplaceCreatorStub` → `CreatorRoute` (Task 6).

No new files under `data/` or `components/` — F3 consumes the frozen seam as-is.

---

## Task 1: `EarningsChart` — inline SVG area chart

**File:** `src/features/marketplace/routes/CreatorRoute.tsx` (create the file with this component only; remaining components added in Tasks 2–4)

The `EarningsChart` renders `earningsWeekly: number[]` as an SVG area chart with a gold stroke and a fade-to-transparent fill. DO NOT pull in uPlot. The design reference is `bc2-creator.jsx → EarningsChart`.

**Props:**
```ts
interface EarningsChartProps {
  data: number[];        // weekly USDC values, ordered oldest → newest
  width?: number;        // default 320
  height?: number;       // default 110
}
```

**Algorithm (from design reference):**
1. Compute `min = 0`, `max = Math.max(...data)`. (Floor at 0; allow zero-width range guard: if max === 0, treat as 1.)
2. Map each data point to `(x, y)` coordinates within the padded inner viewport (`padT = padB = 4`, `padL = padR = 0`).
3. Build two SVG paths: `d` (line: `M ... L ...`) and `dFill` (closed polygon: line path + close to bottom-left corner via bottom-right corner).
4. Render:
   - A `<defs>` block with a vertical `linearGradient` id `earn-fill-<hash>` (using a stable id derived from `data.length` to avoid collisions when two charts render on the same page): `stopColor` gold token at 30% opacity → 0% at bottom.
   - Fill `<path>` using the gradient.
   - Stroke `<path>` in `stroke="var(--gold)"` at `strokeWidth="1.8"`.
   - The SVG uses `width="100%"` + `viewBox="0 0 {width} {height}"` so it scales to its container.

**Token note:** Use CSS variable `var(--gold)` for the stroke (already used by `Sparkline`). For the gradient stop use `stopColor` with the hex `#00E676` (matching the design token value) and explicit `stopOpacity`.

- [ ] **Step 1.1: Create the file with EarningsChart**

Create `src/features/marketplace/routes/CreatorRoute.tsx` containing only the `EarningsChart` component (export it; the page component is added in Task 3).

```tsx
// src/features/marketplace/routes/CreatorRoute.tsx
// F3 — /marketplace/creator/:handleOrAddr
// Tasks fill this file: EarningsChart (T1), LineageForest (T2),
// helper components (T3), CreatorRoute page (T4).

export function EarningsChart({
  data,
  width = 320,
  height = 110,
}: {
  data: number[];
  width?: number;
  height?: number;
}) {
  if (data.length < 2) return null;
  const padT = 4, padB = 4, padL = 0, padR = 0;
  const innerW = width - padL - padR;
  const innerH = height - padT - padB;
  const max = Math.max(...data) || 1;
  const xs = data.map((_, i) => padL + (i / (data.length - 1)) * innerW);
  const ys = data.map((v) => padT + innerH - (v / max) * innerH);
  const linePts = xs.map((x, i) => `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${ys[i].toFixed(1)}`).join(" ");
  const dFill =
    linePts +
    ` L ${xs[xs.length - 1].toFixed(1)} ${(padT + innerH).toFixed(1)}` +
    ` L ${xs[0].toFixed(1)} ${(padT + innerH).toFixed(1)} Z`;
  const gradId = `earn-fill-${data.length}`;
  return (
    <svg
      width="100%"
      viewBox={`0 0 ${width} ${height}`}
      aria-hidden="true"
      className="block"
    >
      <defs>
        <linearGradient id={gradId} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="#00E676" stopOpacity="0.30" />
          <stop offset="100%" stopColor="#00E676" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={dFill} fill={`url(#${gradId})`} />
      <path
        d={linePts}
        fill="none"
        stroke="var(--gold)"
        strokeWidth="1.8"
        strokeLinejoin="round"
      />
    </svg>
  );
}
```

- [ ] **Step 1.2: Typecheck**

Run (from `frontend/web`): `pnpm typecheck`
Expected: PASS (no errors from the new file).

---

## Task 2: `LineageForest` — SVG lineage tree

**Still in `src/features/marketplace/routes/CreatorRoute.tsx`** — append this export after `EarningsChart`.

The `LineageForest` renders `forest: { nodes: ForestNode[]; edges: ForestEdge[] }` as a mixed SVG + absolutely-positioned HTML overlay (matching the design reference's hybrid approach). The SVG draws edges; HTML div tiles position gen-art node thumbnails over it.

**Props:**
```ts
interface LineageForestProps {
  nodes: ForestNode[];
  edges: ForestEdge[];
}
```

**Visual spec (from `bc2-creator.jsx → LineageForest` and README §2):**

- **Coordinate space:** The fixture nodes use `x` values 60–380, `y` values 50–230. Apply an offset (`offsetX = 100`, `offsetY = 30`) so the origin margin stays consistent. The SVG `viewBox` is `"-100 0 580 320"`; rendered `height="300"`.
- **Edge paths:** For each `ForestEdge`, draw a cubic bezier between the node centers:
  - If `from.y === to.y` (same row): a straight horizontal line.
  - Otherwise: a cubic bezier `M x1+22 y1 C x1+dx+22 y1, x2-dx-22 y2, x2-22 y2` where `dx = (x2 - x1) * 0.5`.
  - `kind === "clone"` edges: `stroke="var(--info)"` (token `--info = #5FA8FF`), `strokeWidth="1.2"`, `strokeDasharray="3 3"`, `opacity="0.7"`.
  - Solid edges (variant-of): `stroke="var(--border-strong)"`, `strokeWidth="1.4"`, no dash.
- **Row labels:** Render `<text>` labels at `x="-50"` for each distinct `strategy` group derived from the nodes. Use font `Geist Mono` (falls back to `monospace`) at `fontSize="9"`, `fill` token `var(--text-3)` color (`#5F6670`), `letterSpacing="0.18em"`. Derive labels by collecting the unique non-`clone-by`/non-`clone-from` strategy names from nodes and the y-positions of their first occurrence.
- **Node tiles:** Absolutely positioned over the SVG.
  - Position each tile at `left = ((node.x + offsetX + 100) / 580) * 100 + "%"` and `top = 18 + node.y + offsetY` px. Apply `transform: translate(-50%, -50%)`.
  - `more` nodes: render a `36×36` dashed square with `border border-dashed border-info` (Tailwind `border-info`; if the token isn't a Tailwind class, use `style={{ border: "1px dashed var(--info)" }}`), text content = the label (e.g. `+6 more`), font mono 10.5px, color `text-info`.
  - External (`external: true`) nodes: `GenArtPlaceholder` at `size={32}`, `border` token classes + `border-dashed border-info/70 opacity-80`.
  - Current / HEAD node: `GenArtPlaceholder` at `size={38}`, `border-2 border-gold` (or `style={{ border: "2px solid var(--gold)" }}`).
  - Normal history node: `GenArtPlaceholder` at `size={38}`, `border border-border`.
  - Label below tile: `fontSize: 9.5px`, mono; color: gold for HEAD, info for external/clone, `text-text-2` for history.
- **Click behavior:** Each non-`more` node with a `strategy` that is not `"clone-by"` / `"clone-from"` navigates to `/marketplace/lineage/<node.strategy>` using React Router `useNavigate`. External nodes with a strategy navigate to `/marketplace/lineage/<node.id>` (the clone's root lineage). `more` nodes are non-interactive.
- **Container:** `position: relative; overflow-x: auto` div wrapping the SVG and the absolutely-positioned tiles.
- **Legend dots** in the card header (rendered by the page, not the forest component) follow the `LegendDot` pattern from the design: an 8×8 square with appropriate border/fill + label text.

- [ ] **Step 2.1: Append LineageForest to CreatorRoute.tsx**

Append to `src/features/marketplace/routes/CreatorRoute.tsx`:

```tsx
import { useNavigate } from "react-router-dom";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import type { ForestEdge, ForestNode } from "@/features/marketplace/data/types";

export function LineageForest({
  nodes,
  edges,
}: {
  nodes: ForestNode[];
  edges: ForestEdge[];
}) {
  const navigate = useNavigate();
  const byId = Object.fromEntries(nodes.map((n) => [n.id, n]));
  const offsetX = 100, offsetY = 30;

  // Derive row labels: unique strategy names (excluding clone markers) + y of first node
  const rowLabels: { label: string; y: number }[] = [];
  const seen = new Set<string>();
  for (const n of nodes) {
    if (n.strategy === "clone-by" || n.strategy === "clone-from") continue;
    if (!seen.has(n.strategy)) {
      seen.add(n.strategy);
      rowLabels.push({ label: n.strategy.toUpperCase(), y: n.y });
    }
  }

  return (
    <div className="relative overflow-x-auto" style={{ padding: "18px 18px 22px" }}>
      <svg
        width="100%"
        height={300}
        viewBox="-100 0 580 320"
        aria-label="Lineage forest"
        className="block"
      >
        {/* Row labels */}
        {rowLabels.map(({ label, y }) => (
          <text
            key={label}
            x="-50"
            y={y + offsetY + 4}
            fontFamily="'Geist Mono', monospace"
            fontSize="9"
            fill="var(--text-3, #5F6670)"
            letterSpacing="0.18em"
          >
            {label}
          </text>
        ))}

        {/* Edges */}
        {edges.map((e, i) => {
          const na = byId[e.from];
          const nb = byId[e.to];
          if (!na || !nb) return null;
          const isClone = e.kind === "clone";
          const x1 = na.x + offsetX, y1 = na.y + offsetY;
          const x2 = nb.x + offsetX, y2 = nb.y + offsetY;
          const dx = (x2 - x1) * 0.5;
          const path =
            y1 === y2
              ? `M ${x1 + 22} ${y1} L ${x2 - 22} ${y2}`
              : `M ${x1 + 22} ${y1} C ${x1 + dx + 22} ${y1}, ${x2 - dx - 22} ${y2}, ${x2 - 22} ${y2}`;
          return (
            <path
              key={i}
              d={path}
              fill="none"
              stroke={isClone ? "var(--info, #5FA8FF)" : "var(--border-strong, #2A2A2A)"}
              strokeWidth={isClone ? 1.2 : 1.4}
              strokeDasharray={isClone ? "3 3" : undefined}
              opacity={isClone ? 0.7 : 0.9}
            />
          );
        })}
      </svg>

      {/* Node tiles — absolutely positioned over SVG */}
      {nodes.map((n) => {
        const isHead = !!n.current;
        const isExternal = !!n.external;
        const leftPct = ((n.x + offsetX + 100) / 580) * 100;
        const topPx = 18 + n.y + offsetY;
        const isCloneMarker = n.strategy === "clone-by" || n.strategy === "clone-from";
        const isClickable = !n.more;
        const handleClick = isClickable
          ? () => navigate(`/marketplace/lineage/${isCloneMarker ? n.id : n.strategy}`)
          : undefined;

        return (
          <div
            key={n.id}
            className="absolute flex flex-col items-center gap-1"
            style={{
              left: `${leftPct}%`,
              top: topPx,
              transform: "translate(-50%, -50%)",
              cursor: isClickable ? "pointer" : "default",
            }}
            onClick={handleClick}
            role={isClickable ? "button" : undefined}
            tabIndex={isClickable ? 0 : undefined}
            onKeyDown={isClickable ? (e) => { if (e.key === "Enter" || e.key === " ") handleClick!(); } : undefined}
            aria-label={isClickable ? `View lineage: ${n.label}` : undefined}
          >
            {n.more ? (
              <div
                className="flex items-center justify-center font-mono text-[10.5px]"
                style={{
                  width: 36,
                  height: 36,
                  borderRadius: 4,
                  border: "1px dashed var(--info, #5FA8FF)",
                  color: "var(--info, #5FA8FF)",
                }}
              >
                {n.label}
              </div>
            ) : (
              <GenArtPlaceholder
                seed={n.genArtSeed ?? n.id}
                size={isExternal ? 32 : 38}
                className={
                  isHead
                    ? "border-2 border-gold"
                    : isExternal
                    ? "border border-dashed border-info/70 opacity-80"
                    : "border border-border"
                }
              />
            )}
            <span
              className="font-mono whitespace-nowrap text-[9.5px]"
              style={{
                color: isHead
                  ? "var(--gold)"
                  : isExternal
                  ? "var(--info, #5FA8FF)"
                  : "var(--text-2, #9CA3AF)",
              }}
            >
              {n.label}
            </span>
          </div>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 2.2: Typecheck**

Run: `pnpm typecheck`
Expected: PASS.

---

## Task 3: Helper components (CreatorStat, CreatorStrategyCard, VerdictPill, ReputationFeedRow, CloneByRow)

**Still in `src/features/marketplace/routes/CreatorRoute.tsx`** — append these after `LineageForest`.

These are all page-local; they are NOT exported from the file (used only by `CreatorRoute`). If RTL tests need to pierce them, they do so via the rendered page output.

### 3.1 `CreatorStat`

Counter tile used in the 6-column counter flex. Props: `label: string`, `value: string | number`, `tone?: "text" | "gold"`, `sub?: React.ReactNode`.

```tsx
function CreatorStat({
  label,
  value,
  tone = "text",
  sub,
}: {
  label: string;
  value: string | number;
  tone?: "text" | "gold";
  sub?: React.ReactNode;
}) {
  return (
    <div className="py-4 pr-4 border-r border-border last:border-r-0">
      <div className="font-mono text-[9px] tracking-[0.2em] uppercase text-text-3 mb-1.5">
        {label}
      </div>
      <div
        className={`font-mono text-2xl font-semibold leading-none ${
          tone === "gold" ? "text-gold" : "text-text"
        }`}
      >
        {value}
      </div>
      {sub && (
        <div className="font-mono text-[10.5px] mt-1 text-text-3">{sub}</div>
      )}
    </div>
  );
}
```

### 3.2 `CreatorStrategyCard`

Compact strategy card for the creator's strategies grid. Clicking navigates to `/marketplace/lineage/<strategy.id>`.

Props: `strategy: ListingRow & { status: "live" | "archived" }`.

Layout (from design reference):
- Top section: 46px `GenArtPlaceholder` + strategy id (mono 12px bold) + version (mono 10px text-3) + asset pills + badges.
- Bottom section (3-column grid, border-top): `30D` return label + value (gold), `BUYERS` human + agent count, `CLONES` count.

```tsx
import { Link } from "react-router-dom";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import type { ListingRow } from "@/features/marketplace/data/types";

function CreatorStrategyCard({
  strategy,
}: {
  strategy: ListingRow & { status: "live" | "archived" };
}) {
  const pos = strategy.return30dPct >= 0;
  return (
    <Link
      to={`/marketplace/lineage/${strategy.id}`}
      className="block border border-border rounded-[5px] overflow-hidden bg-[#070707] hover:border-border-strong transition-colors"
    >
      <div className="p-[10px_12px] flex items-center gap-2.5">
        <GenArtPlaceholder seed={strategy.genArtSeed} size={46} />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5 flex-wrap">
            <span className="font-mono text-[12px] text-text font-semibold truncate">
              {strategy.id}
            </span>
            <span className="font-mono text-[10px] text-text-3">{strategy.version}</span>
          </div>
          <div className="flex gap-1 mt-1 flex-wrap">
            {strategy.assets.map((a) => (
              <AssetPill key={a} asset={a} />
            ))}
            {strategy.verification === "verified" && <VerifiedBadge />}
            {strategy.acceptsX402 && <X402Badge />}
          </div>
        </div>
      </div>
      <div className="p-[10px_12px] border-t border-border-soft grid grid-cols-3 gap-2">
        <div>
          <div className="font-mono text-[8.5px] tracking-[0.16em] uppercase text-text-3">30D</div>
          <div
            className={`font-mono text-[13px] font-semibold mt-0.5 ${
              pos ? "text-gold" : "text-danger"
            }`}
          >
            {pos ? "+" : ""}
            {strategy.return30dPct}%
          </div>
        </div>
        <div>
          <div className="font-mono text-[8.5px] tracking-[0.16em] uppercase text-text-3">BUYERS</div>
          <div className="flex items-center gap-1 mt-0.5">
            <span className="font-mono text-[12px] text-text">{strategy.buyers.humans}</span>
            <span className="inline-flex items-center gap-0.5 font-mono text-[10.5px] text-gold">
              <AgentIcon size={8} />
              {strategy.buyers.agents}
            </span>
          </div>
        </div>
        <div>
          <div className="font-mono text-[8.5px] tracking-[0.16em] uppercase text-text-3">CLONES</div>
          <div
            className={`font-mono text-[12px] mt-0.5 ${
              strategy.clones > 0 ? "text-text" : "text-text-3"
            }`}
          >
            {strategy.clones > 0 ? strategy.clones : "—"}
          </div>
        </div>
      </div>
    </Link>
  );
}
```

### 3.3 `VerdictPill`

Tone-coded pill for attestation verdicts. Used in `ReputationFeedRow`.

```tsx
import type { Verdict } from "@/features/marketplace/data/types";

const VERDICT_TONE: Record<Verdict, string> = {
  endorse: "border-gold text-gold",
  question: "border-warn text-warn",
  reject: "border-danger text-danger",
};
const VERDICT_DOT: Record<Verdict, string> = {
  endorse: "bg-gold",
  question: "bg-warn",
  reject: "bg-danger",
};

function VerdictPill({ verdict }: { verdict: Verdict }) {
  return (
    <span
      className={`inline-flex items-center gap-1 px-1.5 py-0.5 border rounded-[3px] min-w-[80px] ${VERDICT_TONE[verdict]}`}
    >
      <span className={`w-1.5 h-1.5 rounded-full ${VERDICT_DOT[verdict]}`} />
      <span className="font-mono text-[9.5px] tracking-[0.14em] font-semibold uppercase">
        {verdict}
      </span>
    </span>
  );
}
```

### 3.4 `ReputationFeedRow` + `CloneByRow`

Feed row for reputation activity (direction label, verdict pill, attester + target, timestamp):

```tsx
import type { AttestationActivity } from "@/features/marketplace/data/types";

function ReputationFeedRow({ item }: { item: AttestationActivity }) {
  const isIssued = item.direction === "issued";
  const relTime = new Date(item.at).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
  return (
    <div className="flex items-center gap-2.5 px-4 py-2.5 border-b border-border-soft last:border-b-0">
      <VerdictPill verdict={item.verdict} />
      <span
        className={`font-mono text-[9px] tracking-[0.18em] uppercase ${
          isIssued ? "text-info" : "text-text-3"
        }`}
      >
        {item.direction}
      </span>
      <span className="font-mono text-[11.5px] text-text-2 flex-1 min-w-0 truncate">
        {isIssued ? `→ ${item.on}` : `${item.attester} → ${item.on}`}
      </span>
      <span className="font-mono text-[10.5px] text-text-3 ml-auto shrink-0">
        {relTime}
      </span>
    </div>
  );
}
```

Clone-by row (who cloned, what they made, earned amount, timestamp):

```tsx
import type { CloneByEntry } from "@/features/marketplace/data/types";

function CloneByRow({ item, isLast }: { item: CloneByEntry; isLast: boolean }) {
  const relTime = new Date(item.at).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
  });
  const initial = item.handle.startsWith("@") ? item.handle[1].toUpperCase() : "?";
  return (
    <div
      className={`flex items-center gap-2.5 px-4 py-2.5 ${isLast ? "" : "border-b border-border-soft"}`}
    >
      <div
        className={`w-6 h-6 rounded-full flex items-center justify-center font-mono text-[9.5px] text-text-3 shrink-0 ${
          item.more
            ? "border border-dashed border-border-strong bg-transparent"
            : "border border-border-strong bg-surface-elev"
        }`}
      >
        {item.more ? "…" : initial}
      </div>
      <div className="flex-1 min-w-0">
        <span className="font-mono text-[11.5px] text-text">{item.handle}</span>
        {!item.more && (
          <>
            <span className="font-mono text-[11px] text-text-4 mx-1.5">cloned</span>
            <span className="font-mono text-[11px] text-text-2">{item.from}</span>
            <span className="font-mono text-[11px] text-text-4 mx-1.5">→</span>
            <span className="font-mono text-[11px] text-text-2">{item.made}</span>
          </>
        )}
      </div>
      <span className="font-mono text-[11.5px] text-gold min-w-[60px] text-right">
        ${item.earnedUsd.toLocaleString()}
      </span>
      <span className="font-mono text-[10.5px] text-text-3 min-w-[54px] text-right">
        {relTime}
      </span>
    </div>
  );
}
```

- [ ] **Step 3.1: Append all helper components to CreatorRoute.tsx**

Append in order: import additions at top of file, then `CreatorStat`, `CreatorStrategyCard`, `VerdictPill`, `ReputationFeedRow`, `CloneByRow` (with their local imports).

Imports to add at the top of `CreatorRoute.tsx`:
```tsx
import { Link, useNavigate } from "react-router-dom";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import type {
  AttestationActivity,
  CloneByEntry,
  ForestEdge,
  ForestNode,
  ListingRow,
  Verdict,
} from "@/features/marketplace/data/types";
```

- [ ] **Step 3.2: Typecheck**

Run: `pnpm typecheck`
Expected: PASS.

---

## Task 4: `CreatorRoute` — the page component

**Append to `src/features/marketplace/routes/CreatorRoute.tsx`.**

The page uses `useParams` to read `:handleOrAddr`, calls `getCreator` through `useMarketplaceData`, and renders all sections in a TanStack-Query-style manual `useEffect`+`useState` pattern (no TanStack Query dependency to add; F0 does not install it for the marketplace; use `useEffect`+local state matching the pattern used by other simple routes in this codebase — check what pattern the F0 layout uses before implementing).

**Check pattern first:**
```bash
grep -n "useState\|useEffect\|useQuery" frontend/web/src/features/marketplace/routes/MarketplaceLayout.tsx
```

If the layout is stateless (just renders `<Outlet />`), use a local `useEffect`+`useState` for data fetching in the route itself. This is already the established pattern for fixture-backed routes.

**Page layout (from design reference §2, README §Creator profile):**

```
Page root: flex col, overflow-y-auto

HERO (border-b border-border, padding "22px 28px 18px 44px")
  grid: "96px 1fr 280px", gap 22, align center
  [0] 96px GenArtPlaceholder seeded from creator.address
  [1] Handle + ENS pill + notableTag badge
      Address row: truncated address + copy stub + Mantlescan icon-link
      joined date (from joinedAt) + rep score
  [2] Action column (flex col, gap 2)
      - Follow @<handle>: disabled ghost button with tooltip (deferred affordance)
      - Row: Share (ghost, links to profile URL) + Tip (disabled, deferred)

COUNTER FLEX (border-b border-border, padding "0 28px 0 44px")
  grid: repeat(6, 1fr)
  1. Strategies — counters.strategies
  2. Lifetime earned — "$" + counters.lifetimeEarnedUsd (tone=gold)
  3. Total buyers — counters.totalBuyers.humans, sub = AgentIcon + "+N agents"
  4. Clones spawned — counters.clonesSpawned mono, sub = "upstream of $Nk"
  5. Attestations — counters.attestationsIssued mono, sub = "issued"
  6. Member since — relative date from joinedAt, mono

STRATEGIES + EARNINGS row (padding "18px 28px 0", grid "1fr 380px", gap 24)
  Left card: "Strategies" title, sub "N on chain · sorted by buyers"
    Tab row: All | Live | Archived (local state; filters strategies by status)
    Grid: 3-col, gap 12 — CreatorStrategyCard for each visible strategy
  Right card: "Earnings · weekly" title
    sub "USDC paid to wallet · 5% platform fee deducted"
    EarningsChart with earningsWeekly data
    Time range row: "32 weeks ago" ← (spacer) → "today" (mono 10.5px text-3)
    Summary stat pill: chart icon + "+$X last 7d · +$X last 30d" (from earningsSummary)

LINEAGE FOREST (padding "18px 28px 0")
  Card with title "Lineage forest"
  sub "N lineages tracked" (derive from unique strategy groups in nodes)
  Header right: legend dots (HEAD=gold, HISTORY=border, CLONE=info dashed) + Expand link stub
  Body: LineageForest component

ATTESTATIONS row (padding "18px 28px 28px", grid "1fr 1fr", gap 18)
  Left card: "Reputation" title
    sub: "N issued · M received · P questions · 0 rejects" (derive from reputationFeed)
    Filter tab row (local state): All | Received | Issued
    Body: filtered reputationFeed rows via ReputationFeedRow
  Right card: "Cloned by · downstream" title
    sub: "N clones of @handle's work · upstream of $Xk earnings"
    Body: clonedBy rows via CloneByRow
```

**Loading/error states:**
- While fetching: render a skeleton placeholder — a single `<div className="px-7 py-8 text-[13px] text-text-3">Loading creator…</div>`.
- On error (creator not found): `<div className="px-7 py-8 text-[13px] text-text-3">Creator not found.</div>`.

**Deferred affordances (Follow, Tip):**
- Follow button: `<button disabled title="Follow is a deferred affordance — on-chain follow registry not yet wired" className="..." aria-label="Follow (coming soon)">Follow {handle}</button>`. Render disabled + visually muted (opacity 50%, `cursor-not-allowed`).
- Tip button: Same pattern — `disabled` + tooltip note.

**Relative date helper** (inline pure function, not exported):
```ts
function relativeDate(iso: string): string {
  const diffMs = Date.now() - new Date(iso).getTime();
  const days = Math.floor(diffMs / 86400000);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo ago`;
  return `${Math.floor(months / 12)}y ago`;
}
```

**Address truncation helper:**
```ts
function truncAddr(addr: string): string {
  if (addr.length <= 10) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}
```

**Strategy tab filter:** Local `useState<"all" | "live" | "archived">` defaulting to `"all"`. Filter `profile.strategies` by `.status` when tab !== "all".

**Reputation feed filter:** Local `useState<"all" | "received" | "issued">` defaulting to `"all"`. Filter `profile.reputationFeed` by `.direction` when tab !== "all"`.

- [ ] **Step 4.1: Append CreatorRoute to the file**

Full export signature:
```tsx
export function CreatorRoute() { ... }
```

The function uses:
- `useParams<{ handleOrAddr: string }>()` from `react-router-dom`
- `useMarketplaceData()` from `@/features/marketplace/data/provider`
- `useState`, `useEffect` from `react`

- [ ] **Step 4.2: Typecheck**

Run: `pnpm typecheck`
Expected: PASS (full file compiles; imports resolve).

---

## Task 5: Tests — `CreatorRoute.test.tsx`

**File:** `src/features/marketplace/routes/CreatorRoute.test.tsx`

Tests render under `MarketplaceDataProvider` (with `FixtureMarketplaceData`) + `MemoryRouter` at `/marketplace/creator/@ed`. The `CREATORS["@ed"]` fixture is the data source; tests assert on rendered text and structure, not internal component shape.

- [ ] **Step 5.1: Write the failing tests**

```tsx
// src/features/marketplace/routes/CreatorRoute.test.tsx
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { CreatorRoute } from "./CreatorRoute";

function renderCreator(handleOrAddr = "@ed") {
  const client = new FixtureMarketplaceData();
  return render(
    <MarketplaceDataProvider client={client}>
      <MemoryRouter initialEntries={[`/marketplace/creator/${handleOrAddr}`]}>
        <Routes>
          <Route
            path="/marketplace/creator/:handleOrAddr"
            element={<CreatorRoute />}
          />
          {/* stub for lineage nav */}
          <Route
            path="/marketplace/lineage/:name"
            element={<div data-testid="lineage-route" />}
          />
        </Routes>
      </MemoryRouter>
    </MarketplaceDataProvider>,
  );
}

describe("CreatorRoute", () => {
  it("renders the creator handle in the hero", async () => {
    renderCreator();
    expect(await screen.findByText("@ed")).toBeInTheDocument();
  });

  it("shows the ENS name pill", async () => {
    renderCreator();
    expect(await screen.findByText("ed.xvn")).toBeInTheDocument();
  });

  it("shows the notableTag badge", async () => {
    renderCreator();
    // notableTag = "agent #0 contributor"
    expect(await screen.findByText(/agent #0 contributor/i)).toBeInTheDocument();
  });

  it("renders the truncated address", async () => {
    renderCreator();
    // address: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4" → "0xa83e…2d4" or similar
    expect(await screen.findByText(/0xa83e…/)).toBeInTheDocument();
  });

  it("Follow and Tip CTAs are disabled (deferred affordances)", async () => {
    renderCreator();
    const followBtn = await screen.findByRole("button", { name: /follow/i });
    const tipBtn = await screen.findByRole("button", { name: /tip/i });
    expect(followBtn).toBeDisabled();
    expect(tipBtn).toBeDisabled();
  });

  it("renders all 6 counter tiles with correct values", async () => {
    renderCreator();
    // strategies = 3
    expect(await screen.findByText("3")).toBeInTheDocument();
    // lifetimeEarnedUsd = 4820
    expect(await screen.findByText(/4.820|4,820/)).toBeInTheDocument();
    // attestationsIssued = 14
    expect(await screen.findByText("14")).toBeInTheDocument();
  });

  it("renders the strategies grid with correct count", async () => {
    renderCreator();
    // 3 strategies in fixture @ed
    const cards = await screen.findAllByRole("link", { name: /btc-momentum-v3|btc-grid-v2|eth-mr-v2/i });
    expect(cards.length).toBe(3);
  });

  it("strategy tab 'Live' filters to live strategies only", async () => {
    renderCreator();
    const liveTab = await screen.findByRole("button", { name: /^live$/i });
    fireEvent.click(liveTab);
    // All 3 fixture strategies have status: "live", so all 3 remain
    const cards = await screen.findAllByRole("link", {
      name: /btc-momentum-v3|btc-grid-v2|eth-mr-v2/i,
    });
    expect(cards.length).toBe(3);
  });

  it("renders the EarningsChart SVG", async () => {
    const { container } = renderCreator();
    await screen.findByText("@ed");
    const paths = container.querySelectorAll("svg path");
    // EarningsChart has 2 paths (fill + stroke)
    expect(paths.length).toBeGreaterThanOrEqual(2);
  });

  it("renders the lineage forest with node labels", async () => {
    renderCreator();
    await screen.findByText("@ed");
    expect(screen.getByText("v3.0")).toBeInTheDocument();
    expect(screen.getByText("v1.0")).toBeInTheDocument();
  });

  it("clicking a forest node navigates to its lineage route", async () => {
    renderCreator();
    // Wait for forest to render
    const nodeBtn = await screen.findByRole("button", { name: /view lineage: v3.0/i });
    fireEvent.click(nodeBtn);
    expect(screen.getByTestId("lineage-route")).toBeInTheDocument();
  });

  it("renders reputation feed rows", async () => {
    renderCreator();
    await screen.findByText("@ed");
    // fixture has 3 reputation feed entries; check a verdict pill
    expect(screen.getAllByText(/endorse|question/i).length).toBeGreaterThanOrEqual(2);
  });

  it("reputation tab 'Received' filters to received only", async () => {
    renderCreator();
    await screen.findByText("@ed");
    const receivedTab = screen.getByRole("button", { name: /^received$/i });
    fireEvent.click(receivedTab);
    // fixture has 2 received entries; "issued" entry should not appear in context rows
    const issuedLabels = screen.queryAllByText("ISSUED");
    expect(issuedLabels).toHaveLength(0);
  });

  it("renders the cloned-by list", async () => {
    renderCreator();
    await screen.findByText("@ed");
    expect(screen.getByText("@solyana")).toBeInTheDocument();
    expect(screen.getByText("@quantnext")).toBeInTheDocument();
  });

  it("renders a not-found message for an unknown handle", async () => {
    renderCreator("@ghost");
    expect(await screen.findByText(/creator not found/i)).toBeInTheDocument();
  });

  it("does not mount any dialog or modal (no-popups rule)", async () => {
    renderCreator();
    await screen.findByText("@ed");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 5.2: Run to verify tests fail (module not yet complete)**

Run: `pnpm exec vitest run src/features/marketplace/routes/CreatorRoute.test.tsx`
Expected: some tests fail — either module errors or assertions fail while the route is incomplete.

- [ ] **Step 5.3: Complete the CreatorRoute implementation from Task 4**

Iterate until all tests pass. Typical failure modes and resolutions:
- Missing `aria-label` on forest node buttons → add `aria-label` matching the test query.
- Disabled button not accessible → ensure `<button disabled>` has accessible name.
- Reputation filter leaving "ISSUED" visible → verify the direction filter also removes the direction label element.
- `lifetimeEarnedUsd` format → test uses `/4.820|4,820/` regex; format with `toLocaleString()` or a `$` + number.
- Address truncation mismatch → adjust `truncAddr` to produce output matching the regex `/0xa83e…/`.

- [ ] **Step 5.4: Run to verify all tests pass**

Run: `pnpm exec vitest run src/features/marketplace/routes/CreatorRoute.test.tsx`
Expected: PASS (16 tests).

- [ ] **Step 5.5: Run full marketplace suite to confirm no regressions**

Run: `pnpm exec vitest run src/features/marketplace`
Expected: PASS (all marketplace tests including F0 + F3).

- [ ] **Step 5.6: Typecheck**

Run: `pnpm typecheck`
Expected: PASS.

- [ ] **Step 5.7: Commit**

```bash
git add src/features/marketplace/routes/CreatorRoute.tsx
git add src/features/marketplace/routes/CreatorRoute.test.tsx
git commit -m "feat(marketplace): F3 creator profile + lineage forest"
```

---

## Task 6: Wire `routes.tsx` — single-line swap

Replace the stub lazy import with the real route. This is a **single-line** import change + nothing else.

- [ ] **Step 6.1: Update the lazy import in `src/routes.tsx`**

Find (line 62 in the frozen F0 file):
```ts
const MarketplaceCreatorStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceCreatorStub })));
```

Replace with:
```ts
const MarketplaceCreatorRoute = lazy(() => import("./features/marketplace/routes/CreatorRoute").then((m) => ({ default: m.CreatorRoute })));
```

Then update the route definition (inside the `marketplace` children array):
```tsx
// Find:
{ path: "creator/:handleOrAddr", element: page(<MarketplaceCreatorStub />) },
// Replace with:
{ path: "creator/:handleOrAddr", element: page(<MarketplaceCreatorRoute />) },
```

- [ ] **Step 6.2: Run routing smoke test**

Run: `pnpm exec vitest run src/features/marketplace/marketplace-routes.test.tsx`
Expected: PASS (existing routing smoke tests still green).

Run: `pnpm typecheck`
Expected: PASS.

- [ ] **Step 6.3: Commit**

```bash
git add src/routes.tsx
git commit -m "feat(marketplace): F3 wire CreatorRoute into router"
```

---

## Task 7: Final verification

- [ ] **Step 7.1: Full marketplace suite**

Run: `pnpm exec vitest run src/features/marketplace`
Expected: PASS — all tasks' tests green.

- [ ] **Step 7.2: Existing routing tests**

Run: `pnpm exec vitest run src/routes.test.tsx src/routes-code-splitting.test.ts`
Expected: PASS — no regressions from the route swap.

- [ ] **Step 7.3: Full typecheck**

Run: `pnpm typecheck`
Expected: PASS.

---

## Done criteria (F3 frozen)

- [ ] `pnpm exec vitest run src/features/marketplace` is fully green.
- [ ] `pnpm typecheck` passes.
- [ ] Existing `src/routes.test.tsx` + `src/routes-code-splitting.test.ts` still pass.
- [ ] `/marketplace/creator/@ed` renders: hero (96px identicon, handle, ENS, address, rep), 6-column counter flex, strategies grid (3 cards), earnings area chart, lineage forest SVG, reputation feed, cloned-by list.
- [ ] Follow and Tip buttons are `disabled` with tooltip notes — not wired to any handler.
- [ ] Lineage forest node click routes to `/marketplace/lineage/<strategy>`.
- [ ] Reputation filter tabs (All / Received / Issued) filter the feed in place without a route change.
- [ ] Strategy tabs (All / Live / Archived) filter the grid in place.
- [ ] No `Dialog`/`Modal`/`Sheet`/`Popover` introduced.
- [ ] No new seam methods added (all data flows through `getCreator`).
- [ ] `EarningsChart` is a pure inline SVG component — no uPlot, no chart library import.
- [ ] `LineageForest` renders solid edges for variant-of and dashed info-colored edges for clone; HEAD nodes have gold border; external and "+N more" nodes are ghosted/dashed.

---

## Open Questions

1. **Address resolution lookup:** `CREATORS` is keyed by `"@ed"`. Should the fixture also index by `creator.address` so navigation from a strategy card (which has the address) can resolve the profile? Current fixture only handles `"@ed"`. The `getCreator` seam accepts `handleOrAddress` — a second index entry `CREATORS[ed.address] = CREATORS["@ed"]` in `fixtures/creators.ts` would close this gap without a new seam method. Noted as a follow-up; F3 tests only target `@ed`.

2. **`formatUsd` utility:** Several display values ($4,820; upstream $2.1k) require formatting. The plan uses `toLocaleString()` inline, which is locale-sensitive and may produce different separators in CI vs dev. If a shared `formatUsd(n: number): string` util exists in this codebase, prefer it. Check `src/lib/` before implementing.

3. **Tailwind token availability for `--info`, `--gold`, `--border-soft`, `--warn`:** The design tokens are CSS variables. Tailwind classes like `text-info`, `border-gold`, `text-gold`, `text-warn`, `border-border-soft` may or may not be configured in `tailwind.config.js`. Check before assuming they are available as utility classes. If not, use inline `style={{ color: "var(--info)" }}` patterns (as Sparkline already does with `var(--gold)` and `var(--danger)`). Resolve by reading `frontend/web/tailwind.config.js` at implementation time.

4. **`bg-[#070707]` for strategy card background:** Taken directly from the design reference. If the codebase has a `bg-surface-card` token that maps to `#0A0A0A`, prefer that. The exact hex is a slight variation (`#070707` vs `#0A0A0A`); use whichever is the correct token.

5. **Lineage row labels (strategy names) derivation:** The plan derives row labels from unique strategy values in the nodes array. The fixture `@ed` only includes one explicit strategy per lineage row. In the full design (see `bc2-creator.jsx`), `ETH-MR` has an external `clone-from` node at negative x. Since `external=true` nodes are excluded from row-label derivation, the ETH-MR row label would only appear if a non-external `eth-mr` node exists. Verify this renders correctly against the `@ed` fixture before finalizing.

6. **No commit** — confirmed. This plan does not `git add` or `git commit` anything. Commits are listed as steps within tasks for the implementing worker.
