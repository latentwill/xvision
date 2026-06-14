# EJ QA re-verification — findings (2026-06-14)

**Scope:** Re-verify the EJ QA punch list against the **current build** (origin/main
`f1dca11`). The QA was run on the deployed EJ node, whose image lagged main; most
items were fixed on **2026-06-12** and are present on main now. This doc records
the status of each reported item with code evidence so we know what still needs work.

**Method:** static verification against `origin/main` source (frontend + backend),
git history for fix commits, and existing tests. The working tree was fast-forwarded
from the QA-era HEAD (47 commits behind) to `f1dca11` before checking. No app build
was run (box is RAM-constrained); findings are code-level + git-history confirmed.

> **Resolution (2026-06-14, branch `qa/2026-06-14-ej-polish-fixes`):** the items
> still open on main — **#2** (Skills columns), **#4** (Stage Pattern UX), **#5**
> (dark-mode white form controls), **#6** (marketplace sell loose end) — have been
> fixed in this branch with tests. Typecheck clean; 533 frontend tests pass. See
> the per-item "Fix applied" notes below and the "Remaining work" section.

---

## Summary

| # | Item | Page(s) | Status | Action |
|---|------|---------|--------|--------|
| 1 | Columns toggle leaves body cells, only hides title | Eval, Agents, Scenarios | ✅ Fixed on main (`0bc34fa`) | none |
| 2 | Columns toggle leaves body cells, only hides title | **Skills** (Settings) | 🔧 Fixed in this branch | thread `visibleKeys` into `SkillRow` |
| 3 | Decision mode → mechanistic 404 (`/api/strategy/:id/mechanistic`) | Strategy detail | ✅ Fixed on main (`926484c`) | none |
| 4 | Flywheel "Stage Pattern" not clickable | Agents → Memory | 🔧 Polished in this branch | always-visible precondition hint |
| 5 | Indicator timeframe = blank white rectangle (dark mode) | Scenarios detail | 🔧 Fixed in this branch | dead `bg-surface` → `bg-surface-elev` |
| 6 | "List your strategy" — saving public description failed | Marketplace → Sell | 🔧 Hardened in this branch | surface real API errors; clearer copy |
| 7 | "Share this acquisition" copy-link copies extra blurb | Marketplace → Receipt/Lineage | ✅ Fixed on main (`07d64d4`) | none |

Net: **3 already fixed on main**, **4 fixed/hardened in this branch** (#2, #4, #5, #6).
Plus 1 related unreported bug fixed alongside #5 (4 dark-mode `<input>`s in
`optimizations-detail.tsx`).

---

## 1 & 2 — "Columns" button hides the title but not the column

**Symptom (reported on Skills, Eval, Agents, Scenarios):** toggling a column OFF in
the Columns picker removed the column header/title but left the body cells visible.

**Root cause (pre-fix):** the shared `ListCard` computed `visibleColumns` and used it
for the `<thead>`, so the header hid correctly — but `renderRow` was called as
`renderRow(row, index)` and each page's row component rendered every `<td>`
unconditionally. Header hid, body stayed. Exactly the reported symptom.

**Fix — `0bc34fa` "fix(strategies): honor column-visibility toggle in body cells
(xvision-r7wi)" (2026-06-12):**
- `frontend/web/src/components/lists/ListCard.tsx:28` — `renderRow` signature gained a
  third arg: `(row, index, visibleKeys: Set<string>)`.
- `ListCard.tsx:265` — now passes the set: `rows.map((r, i) => renderRow(r, i, visibleKeySet))`.
- Each page's `DesktopRow` now guards every cell with `{visibleKeys.has("key") && <td>…</td>}`.

**Verified fixed (item 1):** `eval-runs.tsx`, `agents.tsx`, `scenarios.tsx` all go
through `ResponsiveListCard → ListCard → renderRow(r, i, visibleKeySet)` and guard
their `<td>`s. ✅

**STILL BROKEN — Skills page (item 2):** `frontend/web/src/routes/settings/skills.tsx`
was **not** updated by `0bc34fa`. It wires the column picker
(`useListColumns("settings-skills", …)` at `skills.tsx:132`, `columnState` passed at
`:181`) but its `renderRow` at **`skills.tsx:197`** ignores the third arg:

```tsx
renderRow={(skill) => (
  <SkillRow key={skill.skill_id} skill={skill} … />
)}
```

…and `SkillRow` (`skills.tsx:~261–294`) renders all four `<td>` unconditionally — no
`visibleKeys.has(…)` guards. So on current main, toggling a Skills column off still
hides only the header and leaves the body cells. **Reproduces on `f1dca11`.**

**Fix:** mirror the `strategies.tsx` change — accept `(skill, _i, visibleKeys)` in
`renderRow`, thread `visibleKeys` into `SkillRow`, and wrap each `<td>` in
`{visibleKeys.has("<colKey>") && …}` using the keys from `DESKTOP_COLUMNS`.

---

## 3 — Decision mode → "mechanistic" returned 404

**Symptom:** saving a strategy's decision mode as mechanistic produced
`not_found: no route for /api/strategy/<id>/mechanistic` → "Something went wrong."

**Verified fixed on main.** Frontend and backend now match exactly:
- Frontend `setMechanisticConfig` (`frontend/web/src/api/strategies.ts:559–569`):
  `apiFetch(\`/api/strategy/${id}/mechanistic\`, { method: "PUT", … })` → **PUT**.
- Backend (`crates/xvision-dashboard/src/server.rs:602`):
  `.route("/api/strategy/:id/mechanistic", put(strategies::put_mechanistic))` → **PUT**.
- Methods + path align; no mismatch.

**Why QA saw a 404:** the route did not exist before commit **`926484c`**
"fix(strategy-routes): wire DELETE /filter and PUT /mechanistic (xvision-tflw,
xvision-5o4r)" (**2026-06-12**). Any image built before that date had no matching
route, so Axum returned the exact `not_found: no route for …` error. The QA build
predated the fix.

**Coverage:** handler `put_mechanistic` at
`crates/xvision-dashboard/src/routes/strategies.rs:~680`; integration tests in
`crates/xvision-dashboard/tests/strategies_filter_mechanistic_routes.rs` (persist
mechanistic, clear on agentic, 404 on unknown id). Save flow triggers from
`frontend/web/src/routes/authoring.tsx:~1188` and `FilterCard.tsx:113`.

---

## 4 — Flywheel "Stage Pattern" not clickable

**Verdict: works as designed.** The button is intentionally disabled until the agent
has ≥ 2 observations.

`frontend/web/src/features/memory/MemorySurface.tsx:643–652`:

```tsx
const MIN_OBSERVATIONS = 2;
const obsCount = status?.observations ?? 0;
const tooFewObs = !statusQuery.isPending && obsCount < MIN_OBSERVATIONS;
const isDisabled = autooptimizerMutation.isPending || tooFewObs;   // disabled={isDisabled}
```

A `title` tooltip explains the gate (`MemorySurface.tsx:653–657`): *"Needs at least 2
observations to stage a pattern (currently N)."* Introduced by **`32fec07`**
"fix(memory): guard Stage Pattern below 2 observations (xvision-5jzr)" (2026-06-12),
with six precondition tests in `MemorySurface.test.tsx`. QA most likely viewed an
agent with 0–1 observations.

**Optional UX polish:** the explanation is a hover-only `title` attribute — invisible
on touch and to keyboard users. Consider a small always-visible helper line under the
button when `tooFewObs` (e.g. "Run at least 2 cycles first").

---

## 5 — Indicator timeframe = blank white rectangle (dark mode)

**STILL BROKEN on `f1dca11`.**

The control (`frontend/web/src/routes/scenarios-detail.tsx:496–507`) is a native
`<select>` with:

```tsx
className="bg-surface border border-border rounded px-2 py-1 text-[12px] text-text"
```

**`bg-surface` is a dead class.** The Tailwind config (`frontend/web/tailwind.config.ts`)
defines only `surface-sidebar/-card/-elev/-panel/-hover` — there is **no** bare
`surface` color and **no** `--surface` variable in `src/styles/tokens.css`. So
`bg-surface` generates no CSS and the element gets no background.

Why it shows as **white specifically here**: a `<div>` with no background just lets the
dark parent show through (invisible). A **native `<select>` does not inherit the
parent background** — with no background set it falls back to the browser's default
white control. Hence the "blank white rectangle in dark mode."

**Correct pattern in the codebase:** sibling selects use `bg-surface-elev`, e.g.
`frontend/web/src/routes/eval-compare.tsx:243` and the `.input` utility in
`src/styles/globals.css:215`. `--surface-elev` = `#171c24` (dark) / `#f2f4f7` (light).

**Fix:** change `scenarios-detail.tsx:500` `bg-surface` → `bg-surface-elev` (optionally
add `focus:outline-none focus:border-gold/40` to match `eval-compare.tsx`).

**Related, unreported (same root cause):** four native `<input>`s in
`frontend/web/src/routes/optimizations-detail.tsx:474, 482, 499, 513` also use the dead
`bg-surface` class and will render white in dark mode. Worth fixing in the same sweep,
plus a grep for any other `bg-surface` (no dash) on form controls.

---

## 6 — "List your strategy" — saving public description failed

**Reported:** `Saving the public description failed — strategy 'local-btc-momentum'`.

**This is an environment/wiring artifact, not a logic bug — and it is NOT fixed on
main (it will recur whenever the marketplace runs in fixture mode).**

What happens, step by step:
1. `local-btc-momentum` is a **fixture/demo** strategy id — defined only in
   `frontend/web/src/features/marketplace/data/fixtures/seller.ts:5`. It has no backing
   strategy file on the server.
2. The marketplace data source is chosen at runtime
   (`features/marketplace/routes/MarketplaceLayout.tsx`): subgraph if
   `VITE_MARKETPLACE_SUBGRAPH_URL` is set, else probe `/api/marketplace/status` and use
   `ApiMarketplaceData` when `active === true`, otherwise fall back to
   `FixtureMarketplaceData`. In **fixture mode the picker lists demo strategies** like
   `local-btc-momentum`.
3. `SellRoute.handleMint` saves the description by calling **`patchStrategyMetadata`
   imported directly from `@/api/strategies`** (`SellRoute.tsx:5` and `:54`) — i.e.
   straight to the **live** backend `PATCH /api/strategy/<id>`, **bypassing the `mp`
   data provider**. So even in fixture mode the save hits the real API.
4. Backend `patch_metadata` (`crates/xvision-dashboard/src/routes/strategies.rs:~523`)
   can't find a file for `local-btc-momentum`, so `classify_metadata_patch_error`
   returns `NotFound("strategy 'local-btc-momentum'")`. The client surfaces
   `err.message`, producing the exact reported string.

**Implication:** on a node where the marketplace indexer is active and real strategies
are listed, picking a real strategy and saving its description **succeeds** — this
won't reproduce. It reproduces specifically when the marketplace is in fixture mode
(indexer/`/api/marketplace/status` not active on the QA node) and a demo strategy is
selected.

**Actions:**
1. Confirm `/api/marketplace/status` returns `active: true` on the EJ node (and that
   `VITE_MARKETPLACE_SUBGRAPH_URL`/indexer is configured for that deployment). If it's
   inactive, that's the real defect to fix for QA.
2. Harden the funnel so fixture mode never calls the live API: route the
   description-save through the `mp` client (so `FixtureMarketplaceData` no-ops it)
   instead of importing `patchStrategyMetadata` directly, and/or hide/disable the Sell
   funnel when the active client is the fixture fallback.

---

## 7 — "Share this acquisition" copy-link copies the whole blurb

**Symptom:** "copy link" copied the full marketing caption + URL, not just the link.

**Verified fixed on main** by **`07d64d4`** "fix(marketplace): Copy link copies URL
only, not the share caption (xvision-jxrc)" (2026-06-12). On current main the copy-link
buttons write the URL only:
- `features/marketplace/routes/ShareComposer.tsx:59–66` →
  `navigator.clipboard?.writeText(shareUrl)` where `shareUrl = \`https://${ogCard.url}\``.
- `features/marketplace/routes/ReceiptRoute.tsx:241–244` →
  `writeText(\`https://${receipt.share.ogCard.url}\`)`.

The caption is still passed to the social deep-links ("Post to X", "Farcaster") only.
Tests assert copy-link writes the URL and **not** the caption:
`ShareComposer.test.tsx:123–140`, `ReceiptRoute.test.tsx:142–159`.

---

## Fix commits referenced (all 2026-06-12, post-QA-build)

| Commit | Subject |
|--------|---------|
| `0bc34fa` | fix(strategies): honor column-visibility toggle in body cells (xvision-r7wi) — **did not cover Skills page** |
| `926484c` | fix(strategy-routes): wire DELETE /filter and PUT /mechanistic (xvision-tflw, xvision-5o4r) |
| `32fec07` | fix(memory): guard Stage Pattern below 2 observations (xvision-5jzr) |
| `07d64d4` | fix(marketplace): Copy link copies URL only, not the share caption (xvision-jxrc) |

## Fixes applied in this branch (`qa/2026-06-14-ej-polish-fixes`)

- **#2 Skills columns** — `settings/skills.tsx`: `renderRow` now receives
  `visibleKeys` and `SkillRow` guards every `<td>` with `visibleKeys.has(...)`
  (mirrors `0bc34fa`); edit-row `colSpan` follows the visible count. Test added.
- **#4 Stage Pattern** — `MemorySurface.tsx`: kept the disabled-precondition guard
  and tooltip, added an **always-visible** hint under the button
  ("Needs ≥ 2 observations (N so far)") so it no longer reads as inertly broken on
  touch/keyboard. Test added.
- **#5 Dark-mode form controls** — `scenarios-detail.tsx:500` select and the four
  `optimizations-detail.tsx` inputs: dead `bg-surface` → `bg-surface-elev`
  (+`text-text`/focus ring to match the canonical pattern). The one remaining
  `bg-surface` at `scenarios-detail.tsx:955` is a `<div>` (renders transparent over
  its dark parent, not a white control) — left as-is to avoid an unintended
  visual change.
- **#6 Marketplace sell** — `MarketplaceData.ts`: `listListableStrategies` /
  `createPublishDraft` now re-throw real `ApiError`s and only fall back to demo
  fixtures on a genuine no-backend error, so a strategies-API hiccup surfaces an
  honest error instead of unlistable demo strategies (the `local-btc-momentum`
  dead-end). `Step1PickStrategy.tsx` gained an error state with retry;
  `SellRoute.tsx` surfaces draft-load errors and maps not-found to human copy.
  Tests added.

## Remaining work (ops, not code)

- **#6** The EJ node currently reports `marketplace.status.active=false` and
  `/api/strategies` returns `{items:[],total:0}` — i.e. no strategies exist there,
  so the sell picker is (correctly) empty. To actually demo listing on EJ, seed a
  strategy and/or bring the marketplace indexer up. The code no longer dead-ends if
  the strategies API errors, but it can't list strategies that don't exist.

## Already fixed on main (no action)

- **#1** Columns (Eval/Agents/Scenarios) — `0bc34fa`
- **#3** Decision mode → mechanistic 404 — `926484c`
- **#7** Share copy-link copies URL only — `07d64d4`
