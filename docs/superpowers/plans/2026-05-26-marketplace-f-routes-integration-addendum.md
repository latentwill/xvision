# Phase F Route Plans — Integration Addendum (binding)

> **Purpose:** The F1–F6 route plans were drafted in parallel against the frozen
> F0 seam. This addendum reconciles the cross-cutting concerns several of them
> raised so the five surfaces don't diverge. **Where an individual route plan's
> code conflicts with this addendum, this addendum wins** (notably for data
> fetching). Resolved against the repo on 2026-05-26.
>
> Applies to: `2026-05-26-marketplace-f1-browse.md`, `…-f2-lineage.md`,
> `…-f3-creator.md`, `…-f5-sell.md`, `…-f6-receipt.md` (and the later F4 leaderboard).

## 1. Data fetching — use TanStack Query `useQuery`

The app standard is TanStack Query (`QueryClientProvider` is mounted app-wide in
`src/App.tsx`; existing routes — `routes/scenarios-detail.tsx`, `routes/eval-runs.tsx`
— fetch with `useQuery`). **All marketplace routes fetch the seam through `useQuery`,
not bespoke `useEffect`+`useState`.** Canonical shape:

```tsx
const mp = useMarketplaceData();
const { data, isLoading, error } = useQuery({
  queryKey: ["marketplace", "listing", name],   // ["marketplace", <resource>, ...params]
  queryFn: () => mp.getListing(name),
});
```

Query keys (use exactly these prefixes for cache coherence):
- browse list: `["marketplace", "listings", filter]`
- stats: `["marketplace", "stats"]`  · slices: `["marketplace", "slices"]`
- listing detail: `["marketplace", "listing", name]`
- creator: `["marketplace", "creator", handleOrAddr]`
- leaderboard: `["marketplace", "leaderboard", sliceId]`
- receipt: `["marketplace", "receipt", tx]`
- viewer: `["marketplace", "viewer"]`

No per-resource hook layer is required (no `useListing`/`useCreatorProfile` wrappers)
— call `useQuery` inline in the route. `MarketplaceLayout` does NOT add a
`QueryClientProvider` (the app shell already provides one).

## 2. Shared test helper (build FIRST, before any route)

To keep route tests consistent and provider-correct, add one helper and have every
route test use it. **This is the first thing to build in the route execution wave**
(a tiny shared task, one owner):

`frontend/web/src/features/marketplace/test-utils.tsx`
```tsx
import { type ReactElement, type ReactNode } from "react";
import { render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { MarketplaceDataProvider } from "./data/provider";
import { FixtureMarketplaceData } from "./data/MarketplaceData";

export function renderMarketplace(
  ui: ReactElement,
  { path = "/", route = "/" }: { path?: string; route?: string } = {},
) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        <MemoryRouter initialEntries={[route]}>
          <Routes>
            <Route path={path} element={ui} />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>,
  );
}
```
Route tests render their route via `renderMarketplace(<LineageRoute />, { path: "/marketplace/lineage/:name", route: "/marketplace/lineage/btc-momentum-v3" })` and `await screen.findBy…` for the query to resolve. (Confirm `@tanstack/react-query` import path matches the app's — check `src/App.tsx`.)

## 3. Utilities & deps
- **No `date-fns`.** For `*.at` relative times, add a tiny local `relativeTime(iso)` helper in `features/marketplace/lib/time.ts` (or render absolute timestamps). Do not add a date dependency.
- **USD formatting:** reuse `src/lib/format.ts` where it fits; if a plain `formatUsd(n)` is needed, add it there (one place), don't inline `toLocaleString` per component.
- **`TxChip` is ready** — it already accepts `network` and maps the explorer. Pass `onChain.nft.network` (F2) / `receipt.network` (F6). No change needed.

## 4. Deferred affordances — render, don't invent
These have no seam method and are out of Phase F scope. Render them as disabled/no-op
with a small title/tooltip noting "coming soon", and do NOT add seam methods or modals:
Save view (F1), Follow / Tip (F3), Decrypt-now relay + Install-missing endpoint +
real Discord webhook (F6), Share composer wiring beyond opening intent URLs.

## 5. `routes.tsx` is a shared file — serialize the swaps
Each route plan ends by swapping its stub for the real component in `src/routes.tsx`
(a one-line lazy-import + one-line element change). When routes are built in parallel
(separate worktrees), these edits collide. **The controller batches/serializes the
`routes.tsx` swaps** (or applies them after each route's component lands). Build the
route component + tests first; treat the `routes.tsx` swap as the final, controller-owned
integration step.

## 6. Build parallelism — unlocks after F0 lands on main
The frozen seam lives on `feat/marketplace-f0` (PR #616), not `main`. Agent worktrees
branch off `origin/main`, so they can't see the seam until #616 merges. Therefore:
- **Now:** route builds proceed **sequentially** on `feat/marketplace-f0` via
  subagent-driven-development (one route at a time; controller commits).
- **After #616 → main:** fan out **parallel** route builds, each in its own worktree
  off `main`, owning `routes/<Name>Route.tsx` + tests; controller serializes the
  `routes.tsx` swaps (§5).

## 7. Open-question disposition (from the parallel planners)
| OQ | Disposition |
|---|---|
| useEffect vs useQuery / QueryClient placement (F2, F6) | **Resolved §1/§2** — useQuery; app provides client; tests use `renderMarketplace`. |
| `TxChip` needs `network` (F2) | **Moot** — already present. |
| `date-fns` (F2) | **No dep** — local `relativeTime` (§3). |
| `formatUsd` (F3) | Reuse/extend `src/lib/format.ts` (§3). |
| `newest` sort has no `publishedAt`; `auditedOnly` no-op; `slice` not consumed by `listListings` (F1) | Correct for F0 fixtures; tracked for Phase 1 schema. Keep the `// Phase 1` comments. |
| Address-keyed creator lookup (F3) | Fixture only keys `@ed`; navigating by address may 404 in fixtures — render the route's error state; real impl resolves both (Phase 6). |
| `transferableLicense` not on `Receipt.license` (F6) | Out of scope for the receipt UI; if needed later, enrich from the listing. |
| Mini-OG-card scale in receipt (F6) | Use a CSS `transform: scale()` in a fixed-aspect box; no `ResizeObserver`. |
| Save view / Follow / Tip / Decrypt / Install-missing | **Deferred affordances §4.** |

## 8. Build order (recommended)
`F-glue` (the §2 test helper + §3 `time.ts`) → **F1 browse** → **F2 lineage** (the
viral page) → **F3 creator** → **F6 receipt** → **F5 sell** → **F4 leaderboard**
(reuses F1's list components, so last). Each is its own subagent-driven wave against
the frozen seam, following this addendum.
