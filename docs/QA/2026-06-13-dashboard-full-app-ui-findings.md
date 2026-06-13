# xvn Dashboard — Full-App Test Findings (for a coding agent)

**Date:** 2026-06-13
**Target:** `https://xvn.tail2bb69.ts.net/` (xvn personal node on `extndly-dev`), xvn 0.21.0
**Method:** Playwright (headless Chromium) driving the live SPA. (`webwright` is not installed on this
box; Playwright is the working equivalent.) A recon crawl of 21 routes (`app_crawl.mjs` →
`app-crawl.json` + `app-shots/`), then **6 parallel sub-agents** deep-testing route groups by
clicking every control and verifying dead ones (click → confirm no URL/DOM/network/toast change),
plus self-verification of the top cross-cutting claims. Screenshots in
`/root/xvn-work/chatrail-test/app-shots/` (recon) and `agent*`/per-page names (deep tests).

**Safety:** strictly read-only — no evals started, optimizer untouched (one was in flight), no live
trades, no settings/provider/broker/wallet mutations. **Cleanup:** the "New Strategy" test created 2
`Untitled strategy` records (see CC-5); both deleted. Workspace verified back to baseline
(12 strategies, 25 scenarios). No other artifacts created.

> ⚠️ **Fix within the app's own architecture rules** (from the xvision repo `CLAUDE.md`) — several
> sub-agent "use a modal/dialog" suggestions would VIOLATE these:
> - **No popups/modals/sheets/popovers.** Everything routes, docks, rails, accordions, tabs, or
>   inline-expands. Allowed exceptions: **toasts** and the **Cmd-K command palette**. So the
>   scenario "Clone to edit" inline form and the inline "wallet not detected" message are *correct
>   patterns* — improve them inline, do **not** convert to dialogs.
> - **No right-side boxes when the chat rail is visible** (the rail owns the right column). Fix
>   mobile overlap with padding, not new side panels.
> - **Agent-native parity** — every user action should have an agent/MCP equivalent (cross-ref the
>   companion `docs/QA/2026-06-13-chat-rail-parity-findings.md`).

---

## TL;DR — highest-value fixes

| # | Sev | Theme | One-liner |
|---|-----|-------|-----------|
| CC-1 | **High** | Charts | **Time-range toggles don't re-render charts** — TODAY/7D/30D/ALL (dashboard), 1h…1w (/live), and the strategy-detail chart all leave the canvas unchanged on click (self-verified: canvas hash identical across all 4 dashboard ranges). |
| CC-2 | **High** | Navigation | **Detail cross-links go to list pages, not the record** — eval-run AGENT pill → `/agents`, SCENARIO pill → `/scenarios` (payload lacks the target UUID). |
| CC-3 | **High** | Data binding | **Literal `undefined` & placeholder/seed data shown to users** — `/live` rows: "Decisions undefined / Trades undefined", named "eval run"; marketplace "Strategy #1/#2"; dashboard optimizer card "Best holdout Δ → $?". |
| CC-4 | **High** | Filters | **Tabs/filters that don't filter** — `/live` status tabs, marketplace Trending/New/Mine + facet chips, scenarios "Source: Optimizer" (0 shown despite 9 existing), eval "Mode" filter count. |
| CC-5 | **High** | Safety | **"New Strategy" instantly persists an `Untitled strategy`** — no wizard/confirm; pollutes the list (created 2 during this test). |
| G-1 | **High** | API | `GET /api/eval/runs?status=queued,running` → **400 on every page load** (backend takes one status; comma-join & repeated params both fail). |
| MKT-1 | **High** | Marketplace | Listing bundles **503/404** (`/api/marketplace/listings/1/bundle`→503, `/2/bundle`→404); all perf metrics `—`; seed names. |
| DOC-1 | **High** | Docs | Nav items are `<button>` (no `href`) → not bookmarkable/right-clickable; **"Agent daemon" & "MCP surface" silently render Quickstart** (broken/stub); unknown slugs fall back to Quickstart instead of 404. |
| STR-1 | **High** | Strategy detail | Chart Y-axis renders literal **`00.0%`** on every tick; the **4h** range shows a single diagonal line with 8× repeated `4/1` date labels. |
| G-2 | **Med** | Routing | **No 404 page** — unknown routes silently redirect to `/`. |
| OPT-1 | **High** | Optimizer | `/optimizer/run/:id` is **not a real detail page** — renders the global "hasn't run yet" empty state with only a session chip; no candidates/timeline/cross-links. |
| LIVE-1 | **High** | Live | Default ALL view hides the strategy list (only chart + "deploy one"); list only appears under STOPPED, and tabs don't filter. |
| RWD-1 | **Med** | Responsive | Mobile (390px): chat rail fills the viewport and **hides the dashboard** on first load; chat-rail bottom bar overlaps content on `/eval-runs` & `/docs`; strategies toolbar overflows 88px. |

(Full per-area detail below. ~70 distinct findings.)

---

## Cross-cutting themes (fix once, fixes many pages)

### CC-1 [High] Time-range/chart toggles don't re-render the chart
- **Dashboard `/`**: clicking TODAY / 7D / 30D / ALL leaves the equity chart visually unchanged.
  **Self-verified**: `canvas.toDataURL()` identical before/after all four toggles; only 0–2
  `/api/eval/runs?...&since=` requests fire (so the *fetch* updates but the chart consumer ignores
  the filtered result). Strategy legend chips on `/` likewise don't toggle series.
- **`/live`**: 1h/4h/6h/12h/1d/1w/All update the active-button styling but the chart label stays
  "Live · BTC · 4Hour" and the candles don't change.
- **`/strategies/:id`**: the "4h" range renders a single diagonal line with X-axis `4/1 4/1 … 4/2`
  (8 repeated days) — see STR-2. "All" renders correctly.
- **Likely root cause:** the range/mode state updates but isn't in the chart component's render
  inputs (or the slice/`since` param is computed but the result prop isn't re-applied). Fix in the
  shared chart wrapper.
- *Note:* PRICE+TRADES and VS BUY&HOLD *mode* toggles DO change the canvas; DRAWDOWN looked identical
  to RETURN% (unconfirmed — verify the drawdown series is computed).
- *Unconfirmed:* one agent observed a ~75-request burst on a single 7D click; **I could not
  reproduce** (1–2 requests/toggle). Treat as transient unless it recurs.

### CC-2 [High] Detail cross-links navigate to list pages, not the specific record
- `/eval-runs/:id`: the **AGENT** pill (`aria-label "Open Agent …"`) → `/agents` (list), the
  **SCENARIO** pill → `/scenarios` (list). The STRATEGY pill correctly goes to `/strategies/:id`.
- Root: the eval-run payload exposes display names but not the agent/scenario **UUIDs**, so the link
  can't be built. Add the ids to the run summary and link to `/agents/:id` / `/scenarios/:id`.
- Related dead links: scenario **Runs** tab rows aren't clickable (run id is plain text, should link
  to `/eval-runs/:id`); agent-detail "prompt optimization runs" looks like a link but is plain text.

### CC-3 [High] Literal `undefined` and placeholder/seed data reach the UI
- `/live` STOPPED rows: **"Decisions undefined"**, **"Trades undefined"** (JS `undefined` coerced to
  string — needs `?? '—'`); rows are titled **"eval run"** instead of the strategy name.
- Dashboard `/` optimizer card: **"Best holdout Δ → $? / —"** (literal `$?` template).
- `/marketplace`: listings are **"Strategy #1 v1" / "Strategy #2 v1"** (seed/NFT-default names), all
  perf metrics `—`.
- `/charts/annotated`: Insight Log shows 5 hardcoded demo events (Bull Flag/Volume Divergence/…) with
  static timestamps — looks like mock content with no run selector.
- `/charts/hero`: "The donchian-breakout-4h-ollama-dr1 is **up-1.02%**" (missing space + should be
  "down" for a negative return).

### CC-4 [High] Tabs/filters that don't filter
- `/live`: ALL/LIVE/PAUSED/STOPPED tabs render identical list content (DOM text identical ALL vs
  STOPPED).
- `/marketplace`: Trending/New/Mine tabs identical; the facet chips ("Most agent-bought 2", "Most
  cloned 2", "Free-tier breakouts 2") are no-ops; Sort label doesn't update after selection.
- `/scenarios`: **"Source: Optimizer" returns 0** even though **9 `ec-day-*` optimizer scenarios
  exist** in the backend (CLI confirms 25 total = 16 non-ec-day + 9 ec-day; UI shows 14). The
  optimizer-generated scenarios are invisible in the UI.
- `/eval-runs`: the **Mode** filter doesn't update the "N of 26 runs" count (Status filter does —
  inconsistent). The **Columns** picker (Max DD/Mode/Tokens/Duration/Started) shows "5" checked but
  none of those columns appear as `<th>` in the table.
- `/scenarios` & `/agents`: "Hide archived" toggle doesn't change counts (may be 0 archived — but the
  count badge gives no "N of M" feedback to tell).

### CC-5 [High] "New Strategy" persists immediately with no confirm
- Clicking **New Strategy** on `/strategies` navigates straight to a new persisted record
  (`/strategies/<new-ulid>`) titled "Untitled strategy / missing agent" — no wizard, no name step,
  no confirm, no undo. Two such records were created during this test and had to be deleted.
- Fix: open a creation step (name/template) before persisting, or create a transient draft not
  written until "Save manifest".

---

## Global / app-wide

- **G-1 [High]** `GET /api/eval/runs?status=queued,running` → **400** (`unknown run status
  'queued,running'`) on every page load (console `[xvn:query] query.error`). Backend accepts only a
  single status; comma-join AND repeated `status=` both 400. Fix backend to accept multi-status, or
  split the client into two requests. *(Also in the chat-rail report.)*
- **G-2 [Med]** **No 404 page** — unknown routes (e.g. `/this-route-does-not-exist`) silently
  redirect to `/` with Dashboard content and no message. Add a catch-all 404 route (or at least a
  toast "page not found").
- **G-3 [Med] (a11y)** Cmd-K command palette isn't wrapped in `role="dialog"`/`role="listbox"`; the
  ~53 results are bare `<button>`s, not `role="option"` / `cmdk-item` — screen-reader navigation
  broken. (Functionally the palette works well.)
- **Works app-wide:** all 11 left-nav links resolve with correct `aria-current` active state; theme
  toggle switches and **persists** across navigation; Cmd-K opens, filters, and navigates; no JS
  errors on most routes.

---

## Dashboard (`/`)
- **[High]** Range toggles don't re-render chart (CC-1, self-verified).
- **[Med]** DRAWDOWN mode looks identical to RETURN% — verify the drawdown series is computed/passed.
- **[Low]** Header "0 runs / 0 findings since you were last here · **0h ago**" — show "just now" under 1h.
- **[Low]** "Evaluate 5 user strategies awaiting first eval →" is styled as plain small text, not an
  obvious CTA.
- **[Low]** Optimizer card shows literal **"Best holdout Δ → $?"** (CC-3).
- **[Low]** "Live trading **8 stale**" badge has no tooltip defining "stale".
- **Works:** PRICE+TRADES / VS BUY&HOLD modes render distinct charts; all 7 "View run →" links and
  "View all →" resolve; leaderboard preview shows real metrics; CRITICAL FINDINGS shows real failures.

## Strategies (`/strategies`, `/strategies?sort=leaderboard`, `/strategies/:id`, `/authoring/:id`)
- **STR-1 [High]** Detail chart Y-axis renders literal **`00.0%`** on every tick (static format
  string, not a value formatter). `agent screenshots S10/S-chart-yaxis-check`.
- **STR-2 [High]** Detail "4h" range → single diagonal line, X-axis `4/1 4/1 … 4/2` (bad
  window/slice). "All" is correct. (CC-1.)
- **[High]** "New Strategy" instant-persist (CC-5).
- **[High]** Detail **"Layers ▾"** dropdown opens (▾→▴) but renders **no options** (empty overlays
  list) — banner "Completed eval return % curves." appears but nothing to toggle.
- **[Med]** "Check eval readiness" gives no in-session feedback; the "Recheck/No blocking issues"
  result only appears after a fresh reload (state not re-rendered in place).
- **[Med]** Leaderboard sort reorders rows but adds **no rank/metric column** — can't see *why* the
  order is what it is. Add a "Best PnL/Sharpe" column + rank badge.
- **[Med]** **"Folder"** view toggle (next to "List") opens a RAG document library
  (`?view=folder`), not a folder-grouped strategy view — misleading label.
- **[Med]** Filter editor shows internal `"status":"draft"` field in the saved-filter JSON; "Back to
  strategies" link is duplicated (header + body).
- **[Low]** "Save filter"/"Save mode" enabled with no dirty-state tracking; "agent-direct" shape tag
  uses the warn/amber color (reads as an error); empty Description on a shipped strategy; "DSL
  reference: see docs/operator/filters.md" is unlinked text.
- **[Improvement]** Add "Name Z→A" / "Best PnL" sorts; allow PnL/Sharpe/Runs columns in Columns
  picker; replace the agent `<select>` (long ULIDs) with a searchable combobox; confirm before
  "Insert JSON example" overwrites filter content.
- **Works:** List/Folder/Sort/Columns/Shape filter, per-row Actions menu (Open/Duplicate/Compare/View
  raw JSON), row→detail nav, Agentic/Mechanistic toggle, filter editor insert/clear, Quick
  Performance card (+$10,992.85 / 3.56 Sharpe), zoom +/-, `/authoring/:id` alias renders identically.

## Scenarios (`/scenarios`, `/scenarios/:id`)
- **[High]** Optimizer scenarios invisible in UI (CC-4).
- **[High]** Runs-tab rows not clickable (CC-2).
- **[Med]** Bar-cache tab shows raw `2025-01-01T00:00:00+00:00` and nanosecond `Fetched at:
  2026-06-01T16:28:30.930407474+00:00` — format to friendly dates.
- **[Med]** "Long / Short / No markers" below the chart look like toggles but are static labels.
- **[Low]** Clone form offers relative ranges (YTD/Last 90d) for historical fixed-window scenarios →
  can point past the bar cache; "Search assets…" preview label has no actual input element in the DOM.
- **[Improvement]** Show scenario search on desktop (currently mobile-only); count badge "N of M".
- **Works (well!):** Definition chart renders real BTC OHLCV (2,161 bars), all range toggles + zoom +
  Layers (SMA/EMA/Bollinger/Donchian/markers/panes) + Data table work; Source filter
  (Canonical/User/Clone/…) and Sort/Columns work; Clone form pre-fills from source and cancels
  cleanly; Runs-tab empty state is clean. *(Inline clone form is correct per no-popups rule.)*

## Agents (`/agents`, `/agents/:id`, `/agents/memory`, `/agents/skills`, `/agents/new`)
- **[Med]** TOOLS column renders contradictory stacked text ("No tools / 2 tools / indicator_panel /
  ohlcv / …") — show a clean "2 tools" count + tooltip.
- **[Med]** Agent-detail "RECENT RUNS" always "No runs yet" even for agents in strategies with
  completed backtests — attribution query should join via strategy→agent.
- **[Low]** Memory "EMBEDDING JSON" default `[1,0]` looks like a test value; Skills shows only a count
  + link (no inline skill names); "prompt optimization runs" is dead text (CC-2).
- **Works:** list (12 agents, real names/slots/skills), sort, row→detail (ULID route), Configuration
  tab (provider/model/prompt/tools/memory), Memory & Skills pages with empty states, `/agents/new`
  template picker.

## Eval Runs & Charts (`/eval-runs`, `/eval-runs/:id`, `/eval-runs/compare`, `/charts/*`)
- **[High]** AGENT/SCENARIO cross-links go to list pages (CC-2).
- **[High]** Columns toggle has no visible effect on the table (CC-4).
- **[Med]** `/charts/compare` subtitle says "Roster changes update the URL" but removing a roster pill
  (×) does **not** update `?ids=` — only internal state.
- **[Med]** Failed-run detail empty state says "likely still queued or running" (run is **failed**);
  the real error (`stop_loss_pct must be between 0.1 and 20.0 (got 0.03)`) is buried below empty
  chart panels. Hoist the error to the top for failed runs and fix the status-aware copy.
- **[Med]** Completed run shows TOTAL PNL +$10,992.85 but **Closed trade PnL `—`** / "Realized
  unavailable"; Findings flags a `win_rate_anomaly` (100% win vs 0W/1L) — data inconsistency.
- **[Med]** Mode filter doesn't update run count (CC-4); pagination ←Prev/Next→ permanently disabled
  when all fit on one page (hide it instead).
- **[Low]** "Start eval" form renders the full 26-run checkbox table below the form with no label
  (confusing); `/charts/compare` overlays two runs from non-overlapping periods (Jan–Mar vs Oct–Nov)
  on one axis → meaningless overlay (offer relative-time mode); `/charts/hero` "up-1.02%" malformed
  (CC-3); `/charts/overview` has no strategy selector to focus the drawdown panel; charts/annotated
  appears to be mock/demo.
- **Works:** Status & Strategy filters (with "N of 26" + active chips), Status sort, STRATEGY
  cross-link, row→detail, the chart **Layers** menu (rich), Start-eval form open/cancel, **Download
  JSON** (real file), failed-run error is machine-complete, compare metrics+Findings tables.

## Optimizer (`/optimizer`, `/optimizer/run/:id`)
- **OPT-1 [High]** `/optimizer/run/:id` is not a real detail page — it renders the global "hasn't run
  yet / Launch run" empty state with only a `Session <id> ×` chip; no candidate table, timeline,
  events, or cross-links to the session's eval runs.
- **[High]** `/optimizer` shows "nothing kept yet" / empty lineage even though the lineage API returns
  an **active node with `gate_verdict: Pass`** — the lineage viz reads only the (empty) `cycles`
  array, not `lineage`.
- **[Med]** The page's SSE feed (`/api/autooptimizer/events`) never closes → recon `networkidle`
  timed out (callers must use `domcontentloaded`); consider a resolved loading indicator.
- **[Med/Low]** "hover a branch…" hint shows with no branches; empty sections lack CTAs/doc links.
- **Works:** all optimizer APIs 200; live SSE works; renders real in-flight progress (with the
  domcontentloaded caveat).

## Live Trading (`/live`)
- **LIVE-1 [High]** Default ALL view hides the 20-strategy list (shows only chart + "deploy one");
  list appears only under STOPPED. Tabs (ALL/LIVE/PAUSED/STOPPED) don't filter the list (CC-4).
- **[High]** Rows show "Decisions undefined / Trades undefined" and are named "eval run" (CC-3).
- **[High]** Range toggles don't change the chart (CC-1).
- **[Med]** "Expand trace dock" / "open dedicated trace view" buttons sit at y≈1053 (off-screen on a
  900px laptop); the dedicated-view button disappears after expanding.
- **[Low]** DAILY PNL shows two stacked `—`; venue banner shows raw env names (`ORDERLY_KEY,
  ORDERLY_SECRET, …`); "Deploy strategy →" goes to `/strategies` generally.
- **Works:** real BTC chart; trace dock (when reached) shows TRACE/SIMPLE/ADVANCED + span filters +
  decision navigator; testnet banner persistent.

## Marketplace (`/marketplace`, `/marketplace/lineage/:n`)
- **MKT-1 [High]** Bundle endpoints **503** (`/listings/1/bundle`) and **404** (`/2/bundle`)
  (self-confirmed via curl); all perf metrics `—`; listings named "Strategy #1/#2" (seed) (CC-3).
- **[Med]** Trending/New/Mine tabs + facet chips don't filter; Sort label doesn't update (CC-4).
  "Mine" should gate on wallet + show an empty state.
- **[Med]** Detail route is `/marketplace/lineage/<int>` (sequential int id, not ULID) — non-portable
  links; bundle 404 suggests the int→bundle mapping is broken.
- **[Low]** "Version history" and "More from 0x…" sections render empty headers (hide if empty);
  "List your strategy" full-page wizard has no Cancel/close (Esc doesn't work).
- **Works:** detail pages show real on-chain data (buyer addresses, fees, "Run free"/"Clone to edit");
  Filters panel (asset checklist) works; the listing form reads real workspace strategies; testnet
  banner; mobile collapses cleanly.

## Settings (`/settings/{general,providers,brokers,wallet,marketplace,danger}`)
- **[Med]** `/settings/brokers` logs `settings.broker.load.error` on every load (silent; content
  still renders) — investigate / narrow the catch.
- **[Med]** Providers: `openai` shows "Pick models" while others show "Models · N" (no explanation);
  General "Memory" section has no Save button / no "restart required" banner after a change; the gold
  "Custom" badge has no tooltip.
- **[Med]** `/settings/wallet` and `/settings/marketplace` "Profile" are sparse stubs (one button /
  static descriptor cells) — hide or explain.
- **[Low]** Mobile: settings tab bar truncates → "Marketplace"/"Danger zone" unreachable (no scroll
  affordance); Danger-zone active tab uses gold (brand/positive) not red; "Add provider" Cancel is
  bare text; broker rows show raw env var names as primary labels.
- **Works (well!):** providers show real config with **masked keys** (`● set`, password inputs,
  "never round-trip through this UI again" — no secrets leaked); Alpaca key masked `••••JMSH`;
  **Danger-zone actions require verbatim confirmation** before enabling — good safety.

## Docs (`/docs?slug=…`)
- **DOC-1 [High]** Nav items are `<button>` with no `href` → not bookmarkable, not
  right-click-open-in-tab, not shareable (slug only set via JS pushState).
- **[High]** "**Agent daemon**" (`slug=agent-daemon`) and "**MCP surface**" (`slug=mcp-surface`)
  silently render the **Quickstart** fallback (broken/missing content, no "coming soon").
- **[Med]** Unknown/guessed slugs silently render Quickstart instead of 404; several real docs use
  non-obvious slugs (Operator Runbook=`runbook`, Optimizer Config=`autooptimizer-config`, XVision=
  `xvision`) so guessed/shared URLs fail.
- **[Low]** Docs sidebar search shows `⌘K` on this Linux app (top bar correctly shows `Ctrl K`);
  DISPLAY OPTIONS collapse state not persisted; mobile chat rail overlaps last nav items; several
  RESEARCH docs are thin stubs (Research Index 1.5k, AI Courses 1.4k chars).
- **Works:** 21/23 docs render rich real content (CLI Reference 30k chars/406 code blocks, Filter DSL
  Catalog 12k, Driving-xvn 15k); "Copy as Markdown" present; active-state highlight; full doc-link
  status table in the appendix.

## Responsive / mobile (cross-cutting, RWD-1 [Med])
- Mobile 390px `/`: the **chat rail fills the whole viewport**, hiding the dashboard until you tap
  the hamburger — on small screens default to main content with the rail behind a toggle.
- Chat-rail bottom bar overlaps the last card/nav item on `/eval-runs` and `/docs` — add
  `padding-bottom` ≈ rail height (do **not** add a side panel — see chat-rail column rule).
- `/strategies` toolbar overflows ~88px at 390px (New Strategy/Sort/Columns clipped) — collapse into
  a "Filters" button or flex-wrap.
- Tablet 768px: sidebar stays 360px (~47% of width), cramping content — collapse to an icon rail.
- Settings tab bar truncates on mobile (see Settings).

---

## Appendix
- **Docs nav link status** — 23 items, 21 OK / 2 broken (agent-daemon, mcp-surface → Quickstart);
  several OK-but-non-obvious slugs. (Full table in sub-agent 6 output / reproduced below.)
- **Screenshots:** recon set in `app-shots/{dashboard,strategies-list,…}.png`; deep-test set in
  `app-shots/` prefixed `a1-…/S…/s…/a3…/eval-…/charts-…/opt-…/live-…/mkt-…/settings-…/docs-…`.
- **Verification scripts:** `app_crawl.mjs` (recon), `verify_dash.mjs` (range-toggle/request-count),
  `lib.mjs` (shared harness).

### Docs nav link status table
| Section | Label | slug | Status |
|---|---|---|---|
| QUICKSTART | Quickstart | quickstart | OK |
| CONCEPTS | Strategies | strategies | OK |
| CONCEPTS | Agents | agents | OK |
| CONCEPTS | Optimizer | optimizer | OK |
| CONCEPTS | Providers & Brokers | providers | OK |
| CONCEPTS | Scenarios | scenarios | OK |
| CONCEPTS | Eval Runs | eval-runs | OK |
| CONCEPTS | Experiments | experiments | OK |
| CLI | CLI Reference | cli-reference | OK (30k) |
| AGENT | Driving xvn as an agent | driving-xvn-as-an-agent | OK |
| AGENT | **Agent daemon** | agent-daemon | **BROKEN → Quickstart** |
| AGENT | **MCP surface** | mcp-surface | **BROKEN → Quickstart** |
| OPERATOR | Operator Manual | operator-manual | OK |
| OPERATOR | Firing Conditions | firing-conditions | OK |
| OPERATOR | Filter DSL Catalog | filter-dsl-catalog | OK (12k) |
| OPERATOR | Why some commands aren't in xvn | cli-non-surfaced | OK (non-obvious slug) |
| OPERATOR | Operator Runbook | runbook | OK (non-obvious slug) |
| OPERATOR | Optimizer Config | autooptimizer-config | OK (non-obvious slug) |
| RESEARCH | Research Index | research-index | OK (thin) |
| RESEARCH | XVision / Agentic Trading | xvision | OK (thin, non-obvious slug) |
| RESEARCH | Growth / Marketing | growth-marketing | OK (thin) |
| RESEARCH | AI Courses | ai-courses | OK (thin) |
| RESEARCH | Frontier Image Model Training | frontier-image-training | OK (thin, non-obvious slug) |
