# xvision v1 frontend design

End-to-end design for the v1 web dashboard. The prototype at `frontend/prototype/` defines the visual language; this doc bridges it to the actual `xvision-engine` API surface and surfaces every gap I uncovered between prototype copy and code reality.

**Author**: synthesis pass on 2026-05-10 against the v1 plans, the runtime crates, and the design handoff.

> **Scope note from the user**: marketplace, autooptimizer, and journal are deferred. Findings have no proper structure behind them yet — that and other backend gaps are flagged inline (`⚠ BACKEND GAP`) and consolidated in [§9](#9-backend-gaps-that-block-v1).

---

## 1. What v1 is (and isn't)

**V1 ships a vertical slice**: author a strategy → backtest it (Alpaca paper supported as a "live but isolated" mode) → compare runs → manage providers/brokers. That's the whole loop.

| In v1 | Out of v1 |
|---|---|
| `/` Home (Control Tower) | `/live/:id` Flight Deck (live deployment cockpit) — needs Plan 2c scheduler |
| `/setup` Wizard (first-run + on-demand) | `/marketplace` — Plan 5 (NFTs, blockchain) |
| `/strategies` list | `/journal` Lab Notebook — own deferred plan |
| `/authoring/:id` Inspector | AutoOptimizer (AR-1/AR-2/AR-3 — separate program) |
| `/eval/runs` list | Lineage tree visualization, Slot Machine, Spreadsheet sweep, Canvas node graph |
| `/eval/runs/:id` detail | Mobile responsive, multi-workspace, theme switcher |
| `/eval/compare?ids=…` | Voice/transcription chat input |
| `/settings/{providers,brokers,daemon,identity,danger}` | Wizard proactive nudges (UI exists, no producer in v1) |
| Persistent right-side chat rail | |
| ⌘K command palette (FTS5-backed) | |

**Two architecture facts that shape everything below:**

1. **The UI never imports crate logic directly.** All actions go through `xvision-engine::api::<domain>::<fn>(ctx, req)`. The dashboard is one of three callers (CLI and MCP are the others) — same surface, three transports.
2. **Backtest only.** No live daemon in v1. Alpaca paper is exercised through the same eval pipeline; "Live deployments: 3" tile in the prototype is misleading and should read "Paper deployments" (see [§9](#9-backend-gaps-that-block-v1)).

---

## 2. Stack recommendation

**Recommended: Vite + React 18 + TypeScript + Tailwind, served as static SPA by `xvision-dashboard` (axum).**

| Layer | Choice | Why |
|---|---|---|
| Build tool | Vite | Zero-config TS+JSX, dev server with HMR, builds to static files axum can serve from `xvision-dashboard/static/` |
| UI framework | React 18 | Prototype is already React; minimizes translation cost |
| Language | TypeScript | The engine API is typed Rust; codegen TS types from `engine::api::*` request/response structs (see [§5](#5-api-contract--codegen)) |
| Styling | Tailwind + CSS variables | Prototype's `--gold`, `--surface-*`, etc. become Tailwind theme tokens; `styles.css` becomes the source of truth, ported to `tailwind.config.ts` |
| Server state | TanStack Query | Cache, invalidation, polling, optimistic updates; pairs with the REST surface |
| Realtime | Native `EventSource` (no library) | SSE channels for wizard chat, run progress, chat rail — keep it minimal |
| Routing | React Router v6 (data router) | File-based routing not worth it for ~12 routes; data router gives loaders/actions that mesh with TanStack Query |
| Component primitives | Radix UI (unstyled) | Accessible primitives for dialog, popover, dropdown, tabs; Tailwind for the visuals |
| UI state | Zustand (one tiny store) | Chat-rail open state, command palette state, breadcrumb context — no Redux needed |
| Icons | Inline SVG via `shared.jsx`-style icon set | Already authored in the prototype; port verbatim to `<Icon name="…" />` |
| Charts | Hand-rolled SVG (initial) → Recharts later | Equity curves and sparklines in the prototype are hand-rolled; keep them. Adopt Recharts only if interactivity demands grow (multi-axis brushing, etc.) |

**Alternative considered**: HTMX + Alpine + axum templating. **Rejected** because (a) the Inspector's split editor with live preview and the wizard's streaming chat both want a real client-side state model, and (b) the prototype is React — keeping the same paradigm avoids a translation step.

**Co-location**: The `xvision-dashboard` crate (planned in Plan 2d, not yet built) hosts the axum server. The frontend lives at `frontend/web/` and builds to `crates/xvision-dashboard/static/` via a `cargo xtask build-web` or `package.json` script that the dashboard's `build.rs` triggers.

---

## 3. Project layout

```
frontend/
├── README.md             # high-level orientation
├── DESIGN.md             # this doc
├── prototype/            # design handoff bundle (visual source of truth)
└── web/                  # production app — to be created
    ├── package.json
    ├── vite.config.ts
    ├── tailwind.config.ts        # ports prototype tokens
    ├── tsconfig.json
    ├── index.html
    ├── public/
    └── src/
        ├── main.tsx
        ├── App.tsx
        ├── api/                  # generated TS types + thin fetch wrappers
        │   ├── client.ts         # base fetch w/ error mapping
        │   ├── strategies.ts
        │   ├── eval.ts
        │   ├── settings.ts
        │   ├── chat-rail.ts
        │   ├── search.ts
        │   └── types.gen.ts      # codegen from Rust (see §5)
        ├── routes/                # one file per route
        │   ├── home.tsx
        │   ├── setup.tsx
        │   ├── strategies.tsx
        │   ├── authoring.$id.tsx
        │   ├── eval.runs.tsx
        │   ├── eval.runs.$id.tsx
        │   ├── eval.compare.tsx
        │   └── settings/
        │       ├── providers.tsx
        │       ├── brokers.tsx
        │       ├── daemon.tsx
        │       ├── identity.tsx
        │       └── danger.tsx
        ├── components/
        │   ├── shell/            # Sidebar, Topbar, ChatRail (port from shared.jsx)
        │   ├── chrome/           # CmdK palette, Toast region, Breadcrumb
        │   ├── kpi/              # KpiTile, EquityChart, Sparkline
        │   ├── tables/           # RunsTable, StrategiesTable, TradeLedger
        │   ├── editors/          # SlotEditor, BundleOutline, ValidationRail
        │   └── primitives/       # Button, Pill, Dot, Card, Input, Toggle
        ├── hooks/
        │   ├── useSSE.ts
        │   ├── useChatRail.ts
        │   └── useCmdK.ts
        ├── stores/
        │   └── ui.ts             # Zustand: chat rail open, palette open, etc.
        ├── styles/
        │   └── tokens.css        # CSS variables (ported from prototype/styles.css)
        └── lib/
            ├── format.ts         # number/currency/duration formatters
            └── ulid.ts
```

The `prototype/` directory stays intact as the visual reference — components in `web/src/components/` cite their source artboard in a top-of-file comment.

---

## 4. Route map and navigation

```
/                              Home — Control Tower
/setup                         Wizard (first-run if no provider; on-demand otherwise)
/strategies                    Strategies list
/authoring/:bundleId           Inspector
/eval/runs                     Runs leaderboard
/eval/runs/:runId              Run detail
/eval/compare?ids=a,b[,c…]     Compare runs
/settings/providers            LLM providers
/settings/brokers              Alpaca + Orderly (stub)
/settings/daemon               Daemon heartbeat (read-only)
/settings/identity             ERC-8004 identity (read-only stub)
/settings/danger               Wipe / reset
```

**Navigation primitives** (from `prototype/shared.jsx`):

- **Sidebar** (200px): Home, Strategies, Live (disabled in v1, with tooltip "shipping in v1.1"), Eval, Journal (disabled, "deferred"), Data (settings shortcut for now), Settings. Brand `xvn` in Cormorant italic at the top, "Setup agent" callout card, user row at the bottom.
- **Topbar**: route title (Cormorant 38px), subtitle, ⌘K box (right-aligned, 380px).
- **Right-side chat rail** (320px, collapsible to 40px): persistent across all routes; per-route open state. See [§7](#7-chat-rail).

**Route-level loading & errors**: React Router data router loader functions return TanStack Query prefetches. Errors fall through to a route-level `<ErrorBoundary>` that renders a card with the `ApiError` mapped to a friendly message.

**Default landing**: `/` if a default LLM provider exists, else `/setup`. The dashboard root handler reads `engine::api::settings::get_default_provider()` and 302s.

---

## 5. API contract & codegen

The engine API foundation plan defines the shape:

```
POST/GET/PUT/DELETE /api/strategies            list, get, create, update, delete
POST/GET           /api/eval/runs              run, list_runs, get_run
GET                /api/eval/scenarios         list_scenarios
GET                /api/eval/compare?ids=…     side-by-side
GET/POST/PUT/DELETE /api/settings/providers    CRUD
GET/POST           /api/settings/brokers       CRUD (Orderly stub disabled)
GET                /api/settings/daemon        heartbeat
GET                /api/settings/identity      ERC-8004 read-only
POST               /api/settings/danger        typed-confirm wipe ops
GET                /api/search?q=…&kinds=…     FTS5 across strategies/runs/findings
GET                /api/skills                 list available skills
POST               /api/skills/attach          attach to slot
POST               /api/chat-rail/sessions     create
GET                /api/chat-rail/sessions/:id history
POST               /api/chat-rail/chat         SSE stream
PUT                /api/chat-rail/sessions/:id/scope  context update
POST               /api/wizard/chat            SSE stream (drafting loop)
GET                /api/health                 ⚠ BACKEND GAP — see §9
```

**Codegen**: Use [`ts-rs`](https://github.com/Aleph-Alpha/ts-rs) on the request/response structs in `xvision-engine/src/api/`. A `cargo xtask gen-types` step writes `frontend/web/src/api/types.gen.ts`. Run it in CI; commit the generated file. This eliminates the JSON-schema-drift class of bugs.

**Error mapping**:

| `ApiError` variant | HTTP | UI behavior |
|---|---|---|
| `NotFound` | 404 | Toast + redirect to list |
| `Validation { field, msg }` | 400 | Inline form error |
| `Conflict { msg }` | 409 | Inline + retry option |
| `Internal { id }` | 500 | Toast with `id` for grep into `api_audit` |

All errors include an `audit_id` so a user-reported issue traces back to the audit row.

---

## 6. Per-screen design

For each screen: **data needs**, **components**, and **gaps**.

### 6.1 Home — Control Tower (`/`)

**Source**: `prototype/screen-home.jsx`.

**Data**:
- KPI row: live deployments count, today's P&L (paper, since no live in v1), open positions count, eval runs (30d).
- Equity chart: combined paper P&L curve over selectable horizon.
- Top strategies by P&L today.
- Recent runs table (last 5–10).
- Open positions table (paper-only in v1).
- Quick-start cards (4 actions: New strategy, Run backtest, Go to paper, Add to journal).
- Right rail: agent CTA, recent activity feed, system status.

**Components**: `KpiTile`, `EquityChart`, `RecentRunsTable`, `OpenPositionsTable`, `QuickActionCard`, `ActivityFeed`, `SystemStatusList`.

**Endpoints**:
- `GET /api/dashboard/home` ← **a new aggregator endpoint** that returns all KPIs in one round-trip (avoids 6× fetches on the highest-traffic page).
- `GET /api/eval/runs?limit=10&sort=-started_at` for recent runs.
- `GET /api/health` for system status (⚠ gap).

**Gaps** (mark in code with `// TODO(home):`):

- ⚠ **"Live deployments" tile** is misleading in v1 — no live daemon. **Decision**: rename to "Paper deployments" and show whatever the eval engine considers "active paper sessions". If even that's empty in v1, replace the tile with "Strategies validated (this week)" or hide it.
- ⚠ **Open positions** are ephemeral in the executor (see §9). For v1 either (a) snapshot them to a `paper_positions` table after each cycle, or (b) drop the tile and link to the active run's detail.
- ⚠ **System status** has no health endpoint. Build `/api/health` that probes Alpaca, the LLM providers, and the SQLite/data dir; return `{ alpaca: "ok"|"degraded"|"down", … }`.
- ⚠ **Activity feed** items (e.g., "Run 01H8N7Z completed", "New finding extracted") need an event source. Reuse the `api_audit` table (already planned) — filter to a curated set of `kind` values.
- "Add to journal" Quick Action — journal is deferred; **disable the card** with a "Coming soon" pill.

### 6.2 Setup wizard (`/setup`)

**Source**: `prototype/screen-setup.jsx`.

**Data flow**: chat thread on the left (user ↔ "Setup agent"), live "Strategy in progress" panel on the right that updates as the agent fills slots.

**Components**: `WizardChat` (with `MessageList`, `Composer`, `QuickReplies`), `StrategyProgressPanel` (cards: Template, Agents, Mechanics, Risk, Last eval), `WizardActions` (Open in Inspector, Run paper trade, Save draft).

**Streaming**: `POST /api/wizard/chat` returns SSE. Event envelope:

```ts
type WizardEvent =
  | { type: "tool_call"; name: string; args: unknown }
  | { type: "tool_result"; name: string; result: unknown }
  | { type: "agent_message"; content: string; delta: boolean }
  | { type: "bundle_patch"; patch: BundlePatch }   // updates progress panel
  | { type: "done"; bundle_id: string };
```

The progress panel re-renders on `bundle_patch`. The chat thread renders all events for transparency.

**Endpoints**:
- `POST /api/wizard/chat` (SSE) — main loop.
- `GET /api/templates` — list of strategy templates.
- `POST /api/strategies` (called by agent via tool, not by UI directly) — finalizes the draft.

**Current status / gaps**:
- ✅ **WizardLoop and SSE route ship.** `crates/xvision-dashboard/src/wizard_loop.rs` backs `POST /api/wizard/chat`.
- ⚠ **`?seed=` context handler** (Plan 2d Task 7a) for "Draft variant from this finding" → not built. Frontend should handle the URL param (`/setup?seed=finding:<run_id>:<finding_id>`) and POST it as part of the initial chat request; backend resolves and injects.
- ⚠ **First-run vs on-demand**: Plan says first-run goes to `/setup` if no provider exists. In on-demand mode the wizard needs an "exit to drafts" action. The prototype shows only one mode; **add a "Cancel draft" button** that returns to `/strategies`.

### 6.3 Strategies list (`/strategies`)

**Source**: `prototype/screen-strategies.jsx`.

**Data**: current API returns a compact list of strategy summaries (`agent_id`, `template`, primary `model`). The richer prototype columns (fork lineage, status, last eval, token budget) require additional engine aggregation.

**Components**: `StrategiesTable`, `StatusFilter`, `TemplateFilter`, `SearchBox`, `BulkActions` (currently only "New from template" and "New strategy").

**Endpoints**: `GET /api/strategies`.

**Current status / gaps**:
- ⚠ **Status is not stored on `Strategy`.** Compute UI status from validation output, archive state, and eval attestations rather than adding a second source of truth.
- ⚠ **`forked_from` not tracked**. Add `parent_bundle_id: Option<String>` to `PublicManifest`. Migration: `ALTER TABLE bundles ADD COLUMN parent_bundle_id TEXT REFERENCES bundles(bundle_id)`. Set on agent-driven "Draft variant from this".
- ⚠ **Last eval column** ("1.62 · bull-q1-25") needs a denormalized cache or a JOIN to `eval_runs`. Pick one in the engine API; the UI doesn't care.
- "New from template" button → opens a small modal listing templates with a one-line description of each. Selection navigates to `/setup?template=mean_reversion` to trigger a templated wizard. Template catalog is available at `GET /api/templates`.

### 6.4 Inspector / Authoring (`/authoring/:bundleId`)

**Source**: `prototype/screen-inspector.jsx`. Most complex screen.

**Layout** (4-column): sidebar (200) · bundle outline (220) · split editor (flex) · validation rail (280).

**Bundle outline** (left, second column): tree of Manifest sections (Identity, Eval attestations) and pipeline Layers (Data, Regime, Intern, Trader, Entry/Exit, Risk, Execution). Active layer is highlighted with a gold left border.

**Split editor** (center):
- **Left pane** — slot configuration form: model dropdown, system prompt code editor, tools allowed (chips), max tokens.
- **Right pane** — live preview: fixture selector, debounced auto-rerun, JSON output of the slot's decision.

**Validation rail** (right): list of warnings/errors, token estimate breakdown, bundle JSON preview.

**Components**: `BundleOutline`, `SlotEditor`, `LivePreview`, `ValidationRail`, `TokenEstimate`, `BundleJsonView`.

**Endpoints**:
- `GET /api/strategy/:id` — full bundle.
- `PUT /api/strategy/:id/slot/:role` — legacy slot mutation.
- `POST /api/strategy/:id/agents`, `DELETE /api/strategy/:id/agents/:role`, `PATCH /api/strategy/:id/agents/:role` — AgentRef composition.
- `PUT /api/strategy/:id/pipeline` — pipeline kind / graph edge update.
- `PUT /api/strategy/:id/risk` — risk config update.
- `POST /api/strategy/:id/validate` — validation.
- `POST /api/eval/runs` — kick off "Run eval" button.

**Current status / gaps**:
- ✅ **Inspector edits real AgentRefs/PipelineDef.** Fixed legacy slots still parse, but the current authoring surface is strategy composition over workspace agents.
- ⚠ **Validation warnings** are still narrower than the prototype. Real validators exist for bundle shape and token budget; richer diagnostics should be returned by `engine::api::strategy::validate(id)`.
- ⚠ **Live preview** depends on running a single slot in isolation. The agent crate (`xvision-engine/src/agent/execute.rs`) can do this; needs an API wrapper that takes `(slot_name, fixture_id)` and returns the decision + token usage.
- ⚠ **"Eval attestations"** node in outline tree — `eval_attestations` exists in storage/API, but UI affordance remains thin. Show a simple list in v1 or punt richer publishing controls to v1.1.
- "Test slot" button (header) ≈ "Live preview" right-pane action — collapse one of them into the other.
- Bundle JSON preview is truncated (`...`) in the prototype — make it an expandable `<details>` with the full JSON.
- ⚠ **"Use this agent" toggle** (top of slot editor) — semantics unclear. Best guess: lets the user disable a slot for the current run (e.g., bypass the Intern). Confirm with engine team before wiring.

### 6.5 Eval runs list (`/eval/runs`)

**Source**: `prototype/screen-eval-runs.jsx`.

**Data**: paginated list of runs with all metrics (Sharpe, Return, MaxDD, WinRate, Trades, Tokens, started_at). Three top-level tabs (All / Mine / Published evals). Filter pills (Strategy, Scenario, Mode, Status, Started).

**Components**: `RunsTable` (sortable), `RunFilters`, `CompareBar` (sticky bottom when ≥2 selected), `NewRunButton`.

**Endpoints**: `GET /api/eval/runs?strategy=…&scenario=…&status=…`.

**Current status / gaps**:
- ✅ **Persistent `eval_runs` ships.** `RunStore` persists run metadata, decisions, equity samples, findings, and attestations in SQLite.
- ✅ **Live chart streaming ships for backtests.** `GET /api/eval/runs/:id/stream` streams `RunChartEvent`s for live chart updates; the REST snapshot lives at `GET /api/eval/runs/:id/chart`.
- ⚠ **"Mine" vs "All" vs "Published"** — implies a `created_by` and a `published_at` on runs. Single-user v1 → "Mine" == "All". Hide the tabs in v1, or render but no-op the filter.
- "Compare selected (0)" → enables when ≥2 rows checked, navigates to `/eval/compare?ids=…`.

### 6.6 Run detail (`/eval/runs/:runId`)

**Source**: `prototype/screen-run-detail.jsx`.

**Sections**: breadcrumb + header, KPI tiles (4), equity curve with B&H baseline + trade markers, Findings list, Trade ledger (paginated).

**Components**: `RunHeader`, `KpiTile` (reused), `EquityChartWithMarkers`, `FindingsList`, `TradeLedger`.

**Endpoints**:
- `GET /api/eval/runs/:id` — header + KPIs + equity series.
- `GET /api/eval/runs/:id/chart` — chart-ready bars, indicators, equity, drawdown, position, and markers.
- `GET /api/eval/runs/:id/stream` — live chart SSE while a run is active.

**Gaps** (the user explicitly flagged this area):

- ⚠ **Findings have no schema.** Proposed minimal schema:

  ```rust
  pub struct Finding {
      pub finding_id: Ulid,
      pub run_id: Ulid,
      pub kind: String,            // free-form snake_case label, no enum yet
      pub severity: Severity,      // Critical | Warning | Info
      pub title: String,
      pub summary_md: String,      // markdown
      pub evidence: Vec<EvidenceRef>,
      pub created_at: DateTime<Utc>,
  }
  pub enum EvidenceRef {
      TradeRange { from: Ulid, to: Ulid },
      RegimeWindow { from: DateTime<Utc>, to: DateTime<Utc> },
      MetricThreshold { metric: String, threshold: f64, observed: f64 },
  }
  ```

  Persisted in a `findings` table FK'd to `eval_runs.run_id`. Extraction is a post-run pass (initially rule-based: drawdown > 20% → `large_drawdown`; chop-regime trades < 0.4 Sharpe → `regime_fit_mismatch`; etc.). LLM-based extraction can come later.

- ⚠ **"Draft variant from this →"** action: navigates to `/setup?seed=finding:<run_id>:<finding_id>`. Backend `?seed=` handler is unbuilt (Plan 2d Task 7a).

- ⚠ **Trade ledger** is in-memory in the executor; needs persistence to a `trades` table per run. Schema: `(trade_id, run_id, opened_at, closed_at, side, qty, entry_price, exit_price, realized_pnl_usd)`.

- "Re-extract" button on Findings — POST `/api/eval/runs/:id/findings/extract` — kicks the rule-based extractor (and later, the LLM extractor).

- "Download tape" button — exports the full event log of the run (briefings, decisions, fills, trades) as JSONL. New endpoint `GET /api/eval/runs/:id/tape`.

### 6.7 Compare runs (`/eval/compare?ids=a,b,c`)

**Not in prototype** but explicitly v1-scoped per the engine API plan.

**Layout**: two or three columns, each showing a run's header KPIs + equity curve + findings, side-by-side. A sticky top bar shows ID and lets the user swap a run via popover.

**Components**: `CompareColumn` (reuses `RunHeader`, `KpiTile`, `EquityChart`, `FindingsList`).

**Endpoint**: `GET /api/eval/compare?ids=…` returns an array of compact run summaries (no trade ledgers — only on-demand).

**Decisions**:
- Cap at 3 runs visually. Beyond 3 → render warning, only render first 3.
- Equity curves overlaid on a *single* chart at the top, in addition to per-run columns below — easier to spot divergences.
- ⚠ **Out of scope for v1**: paramater-sweep / spreadsheet view. Compare is point-comparison only.

### 6.8 Settings — Providers (`/settings/providers`)

Radio-style list of LLM providers with default-flag, plus add/edit forms. Each provider: `{ name, kind: anthropic|openai|local, api_key_ref, model_default }`.

**Endpoints**: `GET/POST/PUT/DELETE /api/settings/providers`.

**UI specifics**:
- "Test connection" button per provider — `POST /api/settings/providers/:name/test` → returns latency + model list.
- API key field reads/writes the secrets file via the engine API (key is *referenced* by name in `default.toml`; values live in `~/.xvn/secrets.env`).
- Default-provider radio.

### 6.9 Settings — Brokers (`/settings/brokers`)

Two cards: Alpaca paper (configurable), Orderly (disabled stub).

**Endpoints**: `GET/POST /api/settings/brokers`.

**Gaps**:
- ⚠ **Orderly card** has a wallet-connect button in the v1.5 plan but the wallet plan is not in v1. Render a disabled card with a one-line note linking to the deferred plan.

### 6.10 Settings — Daemon, Identity, Danger

- **Daemon**: read-only heartbeat + last-seen timestamps from `/api/settings/daemon`. No control actions in v1.
- **Identity**: read-only ERC-8004 NFT info (likely empty in v1 — show "Not minted yet" with link to deferred plan).
- **Danger zone**: typed-confirmation buttons — wipe drafts, wipe runs, reset all. Each requires typing the literal ID/name to enable submit. Endpoint: `POST /api/settings/danger { op, confirm }`.

---

## 7. Chat rail

Persistent right-side panel on every route except `/setup` (where the wizard *is* the chat). Width 320px open / 40px collapsed. State persists per-route in a `ui` Zustand store + localStorage.

**Architecture** (per `docs/superpowers/plans/2026-05-10-chat-rail-persistence-plan.md`):

- One session per route + scope. `ContextScope` enum: `{ kind: "global" | "strategy" | "run" | "finding"; ref: string }`.
- Frontend opens an SSE on `POST /api/chat-rail/chat` with the current scope; backend's WizardLoop variant injects scope-aware system prompt.
- History persists in `chat_messages` keyed by `session_id`; loaded on rail open.
- Closing the rail keeps the session alive; reopening reattaches.

**UX**:
- Header shows current scope ("This page: Run 01H8N7Z") with a swap-to-global option.
- Composer at the bottom, Cmd+↵ to send.
- Streaming agent messages render with the same gold left-border treatment as the wizard.
- Tool calls render as collapsible `<details>` blocks (don't drown the user in `list_templates` results).

**Gaps**:
- ⚠ **Entire chat-rail backend is unbuilt** (per the audit). Stub the frontend against a fake SSE endpoint that echoes user input. Cut over when the rail backend ships.

---

## 8. Cross-cutting

### 8.1 Command palette ⌘K

Per `docs/superpowers/plans/2026-05-10-command-palette-plan.md`:

- Modal (Radix Dialog) opened with ⌘K / Ctrl+K from anywhere.
- Two sections: **Search** (FTS5 across strategies, runs, findings via `GET /api/search?q=…&kinds=…`) and **Actions** (verbs like "Create strategy", "Run last config", "Open settings").
- Keyboard nav: ↑↓ to move, ↵ to commit, Esc to close.
- Recent items at top when query is empty.

### 8.2 Toasts and notifications

Bottom-right toast region. Triggered by:
- API errors (mapped from `ApiError`).
- Long-running ops completing (eval run finished, draft saved).
- Audit-flagged events (a destructive op succeeds).

### 8.3 Theming

Folio dark only in v1. The `tailwind.config.ts` reads from `src/styles/tokens.css` so a future light treatment is one CSS file away. **Do not** build a theme switcher UI in v1 (out of scope).

### 8.4 Auth

Localhost-only daemon in v1, no auth on the API surface. **Frontend assumption**: `same-origin` fetches, no tokens. If the user runs the daemon behind a remote tunnel they accept the trust model.

### 8.5 Realtime channels (SSE)

| Channel | Endpoint | Events |
|---|---|---|
| Wizard chat | `POST /api/wizard/chat` | `tool_call`, `tool_result`, `agent_message`, `bundle_patch`, `done` |
| Chat rail | `POST /api/chat-rail/chat` | same as wizard, no `bundle_patch` |
| Live run chart | `GET /api/eval/runs/:id/stream` | `equity`, `marker`, `status` |

One `useSSE(endpoint, body)` hook handles all three. JSON-encoded events, one per `data:` line.

### 8.6 Empty states

Every list has a designed empty state. Authoring "no slots yet" → CTA to add the first slot. Eval runs "no runs yet" → CTA to run the first backtest. Settings providers "no providers" → CTA to `/setup`. Don't ship a blank table.

---

## 9. Backend follow-ups after the v1 cut

Consolidated list of gaps surfaced above, updated after the dashboard/eval implementation pass. These no longer describe blockers for rendering the current app; they describe the remaining deltas from the fuller prototype.

| # | Gap | Severity | Resolution | Owner |
|---|---|---|---|---|
| 1 | Findings extraction/storage is implemented but still shallow | **High** | Continue rule-based extractor first, LLM extractor second | `xvision-engine` |
| 2 | Trade ledger is folded into decision rows, not a dedicated paginated table | **High** | Add a dedicated trade table only if the UI needs independent pagination/export | `xvision-engine` |
| 3 | Strategy status remains computed, not stored | **Medium** | Compute from validation, archive state, and eval attestations | `xvision-engine` |
| 4 | Strategy parent lineage missing | **Medium** | Add `parent_bundle_id` / fork metadata and set on "Draft variant from this" | `xvision-engine` |
| 5 | Validation diagnostics are narrower than the prototype | **Medium** | Return richer `ValidationDiagnostic { code, severity, msg, hint }` values | `xvision-engine` |
| 6 | Live-preview slot endpoint missing | **Medium** | Wrap `agent::execute` in a route owned by strategy/agent preview | `xvision-engine` + `xvision-dashboard` |
| 7 | Open positions remain broker-derived / executor-local | **Medium** | Snapshot paper positions if the UI needs durable position history | `xvision-execution` |
| 8 | `?seed=` context handler for findings → wizard | **Medium** | Resolve finding seeds into wizard context | `xvision-dashboard` |
| 9 | `eval_attestations` UI slot in Inspector is thin | **Low** | Show run-linked attestations in v1; richer publish controls later | `xvision-dashboard` |
| 10 | Activity feed on Home has no curated event source | **Low** | Filter `api_audit` to a curated `kind` list | `xvision-engine` |
| 11 | Inspector "Use this agent" toggle semantics unclear | **Low** | Decide whether this means per-run role disable, archive, or no-op | docs |

None of these block rendering the current dashboard. Treat them as product-depth follow-ups: build the screen against the current API, then light up richer columns/actions as the backend aggregation lands.

---

## 10. Phased delivery

Five phases. Each is independently shippable to a `v1-frontend-preview` build that staff can use.

**Phase 0 — scaffolding (1–2 days)**
- `frontend/web/` Vite app, Tailwind config ported from prototype tokens.
- Build pipeline: `pnpm build` → `crates/xvision-dashboard/static/`.
- Empty axum server in `xvision-dashboard` crate, serves the SPA shell.
- Sidebar + Topbar + ChatRail (collapsed-only) chrome.

**Phase 1 — read-only screens (1 week)**
- Home (with mocked data adapters where backends missing).
- Strategies list (real `GET /api/strategies`).
- Eval runs list (mocked until `eval_runs` table lands; cuts over without UI changes).
- Run detail (KPI tiles + equity curve from existing computation; Findings stubbed).
- Settings: providers, brokers (read-only), daemon, identity.

**Phase 2 — authoring (1 week)**
- Inspector with form-only slot editor (no live preview yet).
- Save draft, validation rail (token estimate live; other warnings stubbed).
- "New strategy" → blank Inspector.

**Phase 3 — agent surfaces (1–2 weeks)**
- Wizard end-to-end (after Plan 2d ships WizardLoop).
- Inspector live-preview (after `preview-slot` endpoint).
- Chat rail end-to-end (after chat-rail plan ships).

**Phase 4 — polish + missing pieces (ongoing)**
- Findings schema + extractor + UI.
- "Draft variant from this →".
- Compare runs.
- Command palette ⌘K.
- Activity feed wiring.
- Empty states polish, error handling, accessibility audit.

**Cutover criterion**: Phase 1 + 2 are usable to demonstrate the v1 vertical slice (manually authoring a bundle and running an eval through the UI) on top of CLI-driven operations. Phase 3 makes the experience match the prototype.

---

## 11. Open questions for the user

Three things I couldn't resolve from code or plans — flagging for sign-off before implementation, *not* before this design lands:

1. **"Live deployments" KPI on Home** — given there's no live daemon in v1, do we (a) rename to "Paper deployments" and surface paper-mode active sessions, or (b) replace with a "Strategies validated this week" metric, or (c) hide the tile?
2. **Strategy status semantics** — confirm the rule for "Validated": one successful eval attestation is enough, or must the bundle pass a quality gate (Sharpe > 0, no warnings, etc.)?
3. **Findings extraction trigger** — should findings extract automatically post-run, or only when the user clicks "Re-extract" in v1? Auto-extract is nicer but adds latency to every run completion.

---

## Appendix A — file-by-file mapping

```
prototype/shared.jsx        → web/src/components/shell/{Sidebar,Topbar}.tsx
                              web/src/components/primitives/{Icon,Sparkline}.tsx
prototype/styles.css        → web/src/styles/tokens.css + tailwind.config.ts
prototype/screen-home.jsx   → web/src/routes/home.tsx (composes KpiTile, EquityChart, ...)
prototype/screen-setup.jsx  → web/src/routes/setup.tsx
prototype/screen-strategies.jsx     → web/src/routes/strategies.tsx
prototype/screen-inspector.jsx      → web/src/routes/authoring.$id.tsx
prototype/screen-eval-runs.jsx      → web/src/routes/eval.runs.tsx
prototype/screen-run-detail.jsx     → web/src/routes/eval.runs.$id.tsx
(no prototype)              → web/src/routes/eval.compare.tsx
(no prototype)              → web/src/routes/settings/*.tsx
prototype/design-canvas.jsx → not ported (dev-only canvas wrapper)
```

## Appendix B — token-level visual port checklist

When porting `styles.css` to `tailwind.config.ts`, preserve these without renaming:

- `--bg`, `--surface-sidebar`, `--surface-card`, `--surface-elev`, `--surface-panel`, `--surface-hover`
- `--border`, `--border-strong`, `--border-soft`
- `--text`, `--text-2`, `--text-3`, `--text-4`
- `--gold`, `--gold-soft`, `--gold-bg`, `--gold-bg-strong`
- `--warn`, `--danger`, `--info`
- `--radius-card` (6px), `--radius-sm` (4px)

Fonts: Cormorant Garamond (display), Inter (UI), JetBrains Mono (numerics). Load via `@fontsource/*` packages, not Google Fonts CDN, so the daemon ships self-contained.

## Appendix C — what's been ruled out

For posterity, surfaces actively NOT in the v1 design:

- Mobile / responsive breakpoints below 1280px.
- Multi-user, multi-workspace, RBAC.
- Theme switcher (light mode), accent customization.
- Embedded code editor (Monaco/CodeMirror) — the system prompt field uses a styled `<textarea>` in v1; rich editor in v1.5.
- Real-time multi-user presence on Inspector.
- Drag-and-drop layer reordering in Bundle outline (the order is canonical per the architecture).
- Voice composer.
- In-app onboarding tour beyond the first-run wizard.
