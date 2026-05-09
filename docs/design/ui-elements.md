# XVN Dashboard — UI Elements Inventory

> **Purpose:** wireframe-handoff inventory of every screen, region, control,
> label, and state in the v1 dashboard. One section per route; controls listed
> with their exact display strings (or label patterns) and the data they bind to.
>
> **Source specs and plans:**
> - `docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md` (wizard, authoring, templates, tiers)
> - `docs/superpowers/specs/2026-05-08-eval-engine-design.md` (runs, scenarios, compare, findings, SSE schema)
> - `docs/superpowers/plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md` (route map, archetypes, axum + SPA, no-build SPA constraint)
> - `docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md` (live cockpit events)
> - `docs/superpowers/plans/2026-05-08-eval-engine-plan.md` (current eval surface)
> - `docs/design/ux-field.md` (eight UX archetypes)
> - `docs/design/gptprompts.md` (visual design system tokens)
> - `docs/dashboard.md` (v0 panel inventory — most CV/vector panels obsolete post-pivot; surviving leaderboard / per-trade ledger structure carry forward)
>
> **Out of scope for v1 wireframes:** marketplace + listings grid (deferred to
> Plan 5), Spreadsheet / Notebook / Canvas / Control Tower / Slot Machine
> archetypes that aren't picked up below (post-hackathon), mobile breakpoints
> (desktop-only), light theme.
>
> **In scope for v1 wireframes:** five archetypes — **Control Tower** (`/`,
> default landing for second-and-subsequent visits), **Wizard** (`/setup`,
> first-run landing + on-demand chat rail), **Inspector** (`/authoring/<id>`),
> **Lab Notebook** (`/journal`), **Flight Deck** (`/live/<id>`); plus the
> **Eval engine** surface (run list, run detail, scenario manager, compare);
> plus **Settings**.
>
> ---
>
> ## Changelog — v0.2 (2026-05-09)
>
> Restructured after an ideonomy-rich evaluation surfaced that the v0.1 nav
> encoded a linear pipeline (Build → Strategies → Eval → Live) while the actual
> user loop is a tight cycle (Notice → Hypothesize → Draft → Test → Diagnose →
> Deploy → Observe → Reflect) with constant back-edges. The five recommended
> moves are now the structural backbone of this doc:
>
> | Move | Change | Sections affected |
> |---|---|---|
> | **A — Control-Tower home** | Replaces Wizard as the default landing on second-and-subsequent visits. Wizard remains as `/setup` for first-run + on-demand. | §2 (new), §3 (was Wizard), §14 (sitemap) |
> | **B — Chat-permanent rail** | Wizard agent becomes a collapsible right-rail on every authenticated route, pre-loaded with current route context. | §1 (global chrome), every route section |
> | **I — Phase-4→1 Bridge** | "Draft variant from this finding" button on every finding row + every veto reason — closes the most-traveled cycle edge. | §7 (Run detail), §11 (Live cockpit) |
> | **F — Lab-Notebook route** | New `/journal` route — append-only chronological surface for findings + free-text notes across all runs. | §9 (new), §14 (sitemap) |
> | **E — Live-preview Inspector** | LLM-slot sections of the Inspector get a split editor: field on left, simulated decision firing against a fixture bar on right. | §4 (Inspector) |
>
> Three moves were considered and deferred (see the ideonomy evaluation in
> session notes for full rationale): phase-typed top nav (too aggressive for
> v1), strategy-centric nav (overlaps with Move A), lineage tree on `/strategies`
> (defer until slot-machine variant pool ships). One move was deferred but
> retained in §15: ambient pass-ribbon (nice-to-have).

---

## 0. Design system reference (from `gptprompts.md`)

These are constants the wireframer should annotate but not redesign.

| Token | Value | Use |
|---|---|---|
| Surface — primary | `#0B0F14` | page background |
| Surface — elevated | `#11161D` | cards |
| Surface — panel | `#1A2029` | nested panels, inspector right rail |
| Border | `#1F2630` (1px slate, never white) | card edges, table dividers |
| Text — primary | `#E8ECF1` | headings, body |
| Text — secondary | `#8B95A4` | labels, helper copy |
| Text — tertiary | `#5A6573` | metadata, timestamps |
| Accent — mint | `#5BE0A2` | primary buttons, equity-up, pass states |
| Status — warn | `#F4B23A` | warnings, "Reportable" badges |
| Status — danger | `#F26A6A` | errors, equity-down, vetoes |
| Status — info | `#6EB4F2` | info, links |
| Type — UI | Inter (or sim.) | labels, body, headings |
| Type — numeric | JetBrains Mono (or sim.), tabular figures | tickers, prices, hashes, IDs |
| Card radius | 10px | all cards |
| Density | moderate-to-high | builder's tool, not consumer |

**Voice:** plain English on all primary surfaces. "Buys dips" not "RSI mean
reversion." Technical labels behind an **Advanced** toggle.

**Status pattern:** filled dot (●) + one-word label, never standalone color.
E.g. `● Running`, `● Passed`, `● Failed`, `● Queued`.

---

## 1. Global chrome (every authenticated route)

### 1.1 Page frame

```
┌─────────────────────────────────────────────────────────────────────────┬─────────────┐
│  TOP NAV (1.2)                                                          │             │
├─────────────────────────────────────────────────────────────────────────┤  CHAT RAIL  │
│                                                                         │   (1.4)     │
│                                                                         │             │
│                       MAIN CONTENT (route-specific)                     │  collapsible│
│                                                                         │  default    │
│                                                                         │  open on    │
│                                                                         │  /setup,    │
│                                                                         │  collapsed  │
│                                                                         │  elsewhere  │
│                                                                         │             │
└─────────────────────────────────────────────────────────────────────────┴─────────────┘
                                              TOAST STACK (bottom-right, over content)
```

### 1.2 Top nav bar — fixed, full-width

| Element | Type | Label | Behavior |
|---|---|---|---|
| Brand wordmark | text logo | `xvn` (lowercase, monospace-adjacent) | links → `/` (Control Tower) |
| Primary nav — Home | nav link | `Home` | → `/` (Control Tower) |
| Primary nav — Strategies | nav link | `Strategies` | → `/strategies` (drafts list) |
| Primary nav — Eval | nav link | `Eval` | → `/eval/runs` |
| Primary nav — Live | nav link | `Live` | → `/live` (deployment list) |
| Primary nav — Journal | nav link | `Journal` | → `/journal` (Lab Notebook) |
| Spacer | flex | — | — |
| Chat rail toggle | icon button | `💬` (or matching icon) + unread dot | opens / collapses §1.4 |
| Status pill — daemon | inline pill | `● Connected` / `● Offline` | binary heartbeat |
| Status pill — LLM | inline pill | `● Anthropic` / `● OpenAI` / `● OpenRouter` / `● No key` | from `/settings` |
| User menu | avatar dropdown | `(initials)` | → menu: `Settings`, `Provenance log`, `About`, `Quit daemon` |

Active nav link gets a 2px mint underline. Inactive links are `text-secondary`.
The `Build` link from v0.1 is gone; "build" is now the action of opening the
chat rail (or visiting `/setup` from Control Tower). The new `Home` and
`Journal` items take its slot.

### 1.3 Toast / notification region

Bottom-right stack, max 3 visible. Auto-dismiss 6s except errors. Variants:
`Info`, `Success`, `Warning`, `Error`. Each toast: status dot + title + 1 line
body + dismiss `×`.

### 1.4 Chat rail (Move B — chat-permanent)

The wizard agent, surfaced as a right-side rail on every authenticated route.
Same agent loop as the standalone Wizard (§3). Width: 360px when expanded,
40px (icon-strip) when collapsed.

| Region | Label / behavior |
|---|---|
| Header strip | `xvn agent` · `● Online` · context chip showing what the agent currently sees, e.g. `Context: this run` / `Context: Inspector draft` / `Context: Home` |
| Context controls | `Change context ▾` dropdown (lets the user explicitly scope the agent to: `Whole workspace`, `This page`, `Selected items`) |
| Chat scroll region | message bubbles (agent left mint border, user right slate fill); inline tool-call cards collapsible |
| Composer | textarea with placeholder dynamic per route (`Ask about this run…`, `Edit this slot…`, `What should I look at first?`); `Send` (`⌘↵`); `Stop` (during streaming) |
| Quick replies row | dynamic per route — see per-route sections for the chip set |
| Footer | `Tokens this session: 12,400` · `Open in /setup ↗` (jumps to full-page Wizard if the user wants more room) |

**Collapse rules:** the rail is open by default on `/setup` only. Elsewhere it
starts collapsed. Per-route open/closed state is remembered (localStorage). An
unread-message dot on the toggle indicates a wizard-initiated nudge (e.g. "I
noticed your paper deploy dropped 3% — want to look?"). Token cost continues
when the rail is collapsed only if a streaming response is in flight; idle
collapsed rail = no LLM calls.

**Empty state when no LLM key:** the rail shows the same `Add an LLM key to
begin` card as §3.3 with a `Set up keys →` link to `/settings`.

### 1.5 Command palette (⌘K)

Modal overlay, centered, 640px wide. Search input at top, placeholder
`Jump to a strategy, run, finding, or scenario…`. Results grouped:
`Strategies`, `Runs`, `Findings`, `Scenarios`, `Deployments`, `Actions`. Action
row example: `New strategy from template…` `↵`. New in v0.2: `Findings` group
is searchable from the palette (so a journal entry from three weeks ago is one
keystroke away from anywhere).

---

## 2. `/` — Control Tower (default landing for second-and-subsequent visits)

**Archetype:** Flight-deck-adjacent (UX field §"FLIGHT DECK gauges"), used as a
home page rather than a per-deployment view. "What needs your attention,
right now."
**First-run handling:** the very first time the daemon boots (no LLM key, no
strategies, no runs) `/` redirects to `/setup`. After the user has any
artifact, `/` becomes the Control Tower below.
**Layout:** two-column main content (8/12 + 4/12 grid). Cards stack vertically
within each column.

### 2.1 Header strip

| Element | Label |
|---|---|
| Page title | `Home` |
| Subtitle | `<n> deployments · <n> drafts · <n> runs since <last visit ts>` |
| Header actions | primary `Open setup agent` (→ `/setup` or expand chat rail) · ghost `New strategy` |
| Time-window selector | pill row: `Today`, `7d`, `30d`, `All` (controls the rollup metrics on this page only) |

### 2.2 Left column — operational

#### 2.2.1 "Needs attention" panel (Phase 0 surface)

| Element | Label |
|---|---|
| Card title | `Needs attention` |
| Empty state | `Nothing flagged. Your bots are running clean.` |
| Item types | each row: severity dot + title + one-line context + `Open ↗` |
| Item — deployment alert | e.g. `paper-eth-mr-v3 dropped −4.1% yesterday` |
| Item — finding | e.g. `Critical finding: regime fit mismatch (eth-mr-v3, scenario crypto-bear-q3-2024)` |
| Item — draft drift | e.g. `Draft btc-momentum-v2 has unsaved validation errors` |
| Item — eval queue | e.g. `2 evals queued, 1 running (~3 min remaining)` |
| Item — wizard nudge | e.g. `I noticed 3 of your last 5 evals failed in chop. Want me to draft a chop-aware variant?` (links into chat rail with that prompt pre-filled) |

#### 2.2.2 Live deployments rollup

| Element | Label |
|---|---|
| Card title | `Live now` + count chip |
| Empty state | `No live runs.` + `Deploy a strategy →` button |
| Per-deployment row | strategy name · mode pill (`Paper` / `Live`) · `● Running` status · P&L today (color by sign) · sparkline · `Open cockpit ↗` |
| Footer link | `View all deployments →` |

#### 2.2.3 Eval queue

| Element | Label |
|---|---|
| Card title | `Eval queue` |
| Empty state | `No queued evals.` |
| Per-row | run name · scenario · status (`● Queued` / `● Running 42%`) · ETA · ghost `Cancel` |
| Footer link | `View all runs →` |

### 2.3 Right column — knowledge & cycle re-entry

#### 2.3.1 Recent findings (Lab-Notebook preview)

| Element | Label |
|---|---|
| Card title | `Recent findings` + `Open journal →` link |
| Per-row | severity dot · finding `kind` · summary (truncated 1 line) · run / strategy chip · `Draft variant from this →` ghost button (Move I) |
| Empty state | `No findings yet — they'll appear after your first eval.` |

#### 2.3.2 Drafts in progress

| Element | Label |
|---|---|
| Card title | `In your workshop` |
| Per-row | draft name · template chip · status (`Draft` / `Validating` / `Eval-ready`) · `Open in Inspector →` |
| Footer link | `View all strategies →` |

#### 2.3.3 Lineage cue (forward-compat)

A single line, optional: `You've forked btc-momentum 4 times this week — see
lineage →`. Only renders when ≥3 sibling drafts exist from one root. Links to
the future lineage tree view (Move G, deferred). v1 wireframes can show this
as a stub link with `Coming soon` chip.

### 2.4 Empty / cold-start states

If the daemon has artifacts but the user hasn't visited in >7 days, the
"Needs attention" panel sorts a single `Welcome back. Here's what changed.`
synthesis card to the top. Wizard-generated, optional, dismissable.

---

## 3. `/setup` — Wizard (first-run landing + on-demand)

**Archetype:** Wizard (UX field §"WIZARD ★ default L1 chat-led").
**When you land here:**
- Automatically on first run (no LLM key, no artifacts).
- By clicking `Open setup agent` from Control Tower.
- By clicking `Open in /setup ↗` in the chat rail when the user wants more room.

**Layout:** two-column split, 50/50 on ≥1440px, 60/40 on smaller. The chat rail
(§1.4) is hidden on this route — the chat is the page.

### 3.1 Left column — Chat

(Same as v0.1 §2.1 — verbatim labels preserved.)

| Element | Label / pattern |
|---|---|
| Header strip | `xvn setup agent` · `● Online` (the wizard's connection state) |
| Chat scroll region | message bubbles, agent on left (mint border), user on right (slate fill) |
| Inline tool-call card | `Tool: list_templates` → collapsible JSON result |
| Inline citation card | `Found template: trend_follower — "Catches uptrends"` |
| Composer textarea | placeholder `Tell me what you want to build…` |
| Composer — Send | primary button `Send` (`⌘↵`) |
| Composer — Attach context | ghost button `+` → menu: `Attach a chart`, `Paste a thesis`, `Reference a strategy`, `Reference a finding` |
| Composer — Stop | only visible while streaming; red ghost `Stop` |
| Quick replies row | dynamic chips: `Try a free strategy`, `Build from a template`, `Describe what I want`, `Diagnose a recent run` |

### 3.2 Right column — Live strategy progress

(Same as v0.1 §2.2.)

| Region | Label | Contents |
|---|---|---|
| Header | `Strategy in progress` | strategy name (placeholder `Untitled`) + status pill `● Drafting` / `● Validating` / `● Eval-ready` / `● Saved` |
| Section — Template | `Template` | selected template chip + `Change` link |
| Section — Slots | `Agents` | three rows: `Regime`, `Intern`, `Trader`. Each: status dot, slot name, model name (mono), `Edit prompt` ghost |
| Section — Mechanics | `Mechanics` | param chips: `Cadence: 15m`, `Asset: BTC/USD`, `Stop: ATR×2`. Editable on click. |
| Section — Risk | `Risk` | preset radio: `Conservative` / `Balanced` / `Aggressive`. `Show details` toggle. |
| Section — Eval preview | `Last eval` | empty: `No eval yet.` Filled: scenario · `● Passed` / `● Reportable` / `● Failed` · Sharpe · total return. Click → `/eval/runs/<id>`. |
| Footer actions | sticky | primary `Save draft` · ghost `Open in Inspector` · ghost `Run paper trade` (disabled until validated) |

### 3.3 First-run / no-LLM-key state

(Same as v0.1 §2.3.)

| Element | Label |
|---|---|
| Card title | `Add an LLM key to begin` |
| Body | `xvn uses your key for both the setup agent and the strategies it builds. We never store your key on a server.` |
| Buttons (3, side by side) | `Get an Anthropic key →` · `Get an OpenAI key →` · `Get an OpenRouter key →` |
| Paste field | label `Already have one?` placeholder `sk-…` |
| Provider select | dropdown next to paste: `Anthropic` (default) / `OpenAI` / `OpenRouter` |
| Submit | primary `Save and continue` |
| Footer link | `Why we don't issue keys →` |

### 3.4 Wizard error / blocked states

(Same as v0.1 §2.4.)

- **MCP tool failed:** inline red card `Tool failed: <name>` + collapsible
  error + `Retry` ghost.
- **Token budget exceeded:** banner above composer: `This run will use ~53,500
  tokens. Continue?` `Cancel` · `Continue`.
- **Daemon disconnected:** chat input disabled with banner `xvn daemon stopped.
  Restart it from the menu.`

### 3.5 Cross-cycle entry points

The wizard accepts pre-loaded contexts from elsewhere in the app. Each entry
mode pre-seeds the first user-side message and pins a context chip in the
header strip:

| Entry source | Context chip | Pre-seeded prompt template |
|---|---|---|
| Control Tower wizard nudge | `Context: 3 chop-fail evals` | `I'd like to draft a variant that handles chop better.` |
| Run detail "Draft variant from this finding" | `Context: finding <id> on run <ulid>` | `The previous version did <X> badly because <Y>. Let's fix it.` (Move I) |
| Live cockpit "Draft variant from this veto reason" | `Context: <N> vetoes for rule <name>` | `My deployment is getting vetoed for <rule>. Let's revise.` (Move I) |
| Inspector overflow | `Context: editing <slot> in draft <name>` | `Help me write a better prompt for <slot>.` |

---

## 4. `/authoring/<draft_id>` — Inspector (depth editing)

**Archetype:** Inspector (UX field §"INSPECTOR form / L2"), upgraded with a
**live-preview split** for LLM slot sections (Move E) so the wizard's "AI
built this" posture survives into depth work.
**Layout:** three-column. Left = bundle outline (sticky tree). Center =
**adaptive section editor** — section-by-section form for mechanical layers,
**split editor (form left / live preview right)** for LLM slot sections.
Right = inspector rail (validation + token estimate + bundle preview). The
chat rail (§1.4) docks outside the right inspector rail when expanded.

### 4.1 Left column — Bundle outline (sticky tree)

(Unchanged from v0.1 §3.1.)

```
Manifest
  ▸ Identity
  ▸ Eval attestations
Layers
  ① Data
  ② Regime classifier        [LLM]
  ③ Intern                   [LLM]
  ④ Trader                   [LLM]
  ⑤ Entry / Exit rules
  ⑥ Risk
  ⑦ Execution
Skills
  ▸ Attached skills
Validation
  ● 2 warnings, 0 errors
```

Tree footer: `Bundle hash: 0xa83…f12` (mono, click to copy).

### 4.2 Center column — Adaptive section editor

The center column shape changes by section type.

#### 4.2.1 Mechanical sections (Manifest, Data, Entry/Exit, Risk, Execution)

Single-column form. Field tables identical to v0.1 §3.2.1 / §3.2.3. No live
preview pane (mechanical params don't have a per-keystroke "what does this
do?" answer that's worth a fixture run).

#### 4.2.2 LLM slot sections (Regime / Intern / Trader) — Move E split editor

Two-pane split (50/50 by default, draggable divider).

**Left pane — Slot form** (same fields as v0.1 §3.2.2):

| Field | Label | Control |
|---|---|---|
| Enabled | `Use this agent` | toggle (Trader required, can't disable) |
| Model class | `Model` | select |
| Prompt | `System prompt` | code editor (monospace, 12pt), `Format` button, `Diff vs template` toggle |
| Tools allowed | `Tools the agent can call` | multi-select chips |
| Output schema | `Expected output` | read-only JSON, `Edit schema` ghost (Advanced) |
| Compose skill | `Add a skill` | button → skill picker modal |
| Token budget | `Max tokens / call` | numeric |

**Right pane — Live preview**:

| Element | Label |
|---|---|
| Pane header | `Preview decision` + fixture chip (e.g. `Fixture: BTC/USD 2025-01-15 08:00`) + `Change fixture ▾` |
| Sub-header status | `● Auto-rerun on (2s debounce)` toggle / `Manual` mode shows a `Run preview` button |
| Input panel (collapsible) | `Inputs the agent sees` — JSON of `ohlcv_history`, `indicator_panel`, etc. as the slot would be invoked |
| Output panel | live LLM output rendered against `output_schema`. While streaming: `● Streaming…` + token count. Final: parsed JSON + validation status. |
| Diff strip (only when prompt edited) | `Δ vs last preview:` shows whether the parsed action / regime / etc. changed compared to the previous preview run |
| Cost row | `Last preview: 1,420 tokens · ~$0.012 at your current model` |
| Footer | `Save these inputs as a custom fixture` ghost button (saves to `~/.xvn/fixtures/<name>.json` — referenceable from any slot in the workspace) |

**Fixture picker** (modal, opens on `Change fixture ▾`):

| Section | Contents |
|---|---|
| `Built-in` | `BTC bull 2025-01-15 08:00`, `BTC chop 2024-09-10 14:00`, `ETH bear 2024-08-05 10:00`, `Flash crash 2024-08-05 12:00` |
| `From your runs` | last 20 setups across user's runs, filterable by asset / regime |
| `Custom` | user-saved fixtures from this slot or others |

**Mechanical fields inside an LLM slot** (e.g. token budget) re-flow to the
left pane only — they don't trigger a preview re-run.

#### 4.2.3 Per-slot footer (every slot section)

`Test slot with sample input` ghost button — opens slot tester drawer (same
behavior as v0.1, used for A/B-testing two prompt revisions side by side
without leaving the page). Drawer is distinct from the always-on preview pane:
the preview shows current state continuously, the drawer is a one-shot 2-up
comparison.

### 4.3 Right column — Inspector rail

(Unchanged from v0.1 §3.3.)

| Region | Label | Contents |
|---|---|---|
| Validation | `Validation` | issue list. Empty: `● Ready to publish or eval.` |
| Token estimate | `Estimated tokens / run` | `input 45,000` · `output 8,500` · `total 53,500`. Source: eval engine §8. |
| Bundle preview | `Bundle JSON` | `Copy hash`, `Download bundle`, collapsible JSON viewer |
| Quick actions | (sticky bottom) | primary `Run eval` · ghost `Save draft` · ghost `Publish…` (disabled in v1, deferred to Plan 5) |

### 4.4 Skill picker modal

(Unchanged from v0.1 §3.4.)

| Element | Label |
|---|---|
| Title | `Attach a skill to <slot name>` |
| Search field | placeholder `Search skills…` |
| Tabs | `My skills` / `Installed` / `Marketplace` (Marketplace tab disabled with `Coming soon` chip in v1) |
| List row | skill name (mono), version, tags, `Attach` ghost |
| Footer | secondary `Cancel`, primary `Attach selected (n)` |

### 4.5 Chat rail context on this route

When opened on the Inspector, the rail's context chip reads `Context: editing
<slot> in <draft name>`. Quick replies: `Improve this prompt`, `Why is this
slot expensive?`, `Suggest a tool to add`, `Diff vs template`.

---

## 5. `/strategies` — Strategy drafts list

(Largely unchanged from v0.1. v0.2 adjustments noted inline.)

| Element | Label |
|---|---|
| Page header | `Strategies` |
| Header actions | primary `New strategy` (→ `/setup`) · ghost `New from template…` |
| Filters bar | search input `Filter by name…` · status select `All / Draft / Validated / Published` · template select |
| Table columns | `Name`, `Template`, `Forked from` *(new in v0.2 — shows parent draft when set)*, `Status`, `Last eval`, `Tokens / run`, `Updated`, `Actions` |
| Row action menu | `⋯` → `Open in Inspector`, `Duplicate`, `Fork` *(new — preserves parentage)*, `Run eval`, `Deploy paper`, `Delete` |
| Empty state | `No drafts yet. Start with the setup agent or pick a template.` + buttons `Open setup agent` · `Browse templates` |

The `Forked from` column is the v1 stub for the deferred lineage tree view
(Move G). When the slot-machine variant pool ships, this column links into the
tree; until then it's a flat parent-name link.

### 5.1 Chat rail context on this route

`Context: Strategies list (filter: <current filter>)`. Quick replies: `Help me
pick which to work on`, `Which has the worst recent eval?`, `Suggest a fork from
<top-of-list>`.

---

## 6. `/eval/runs` — Run list / leaderboard

(Unchanged from v0.1 §5.)

### 6.1 Header strip
| Element | Label |
|---|---|
| Page title | `Eval runs` |
| Header actions | primary `New run` · ghost `Compare selected (n)` (enabled when ≥2 rows checked) |
| Tabs | `All`, `Mine`, `Published evals` |

### 6.2 Filters bar
- Strategy multi-select chips
- Scenario multi-select chips
- Mode select: `All / Backtest / Paper`
- Status select: `All / Queued / Running / Completed / Failed / Cancelled`
- Date range picker: `Started in…`
- Sort by select: `Most recent`, `Best Sharpe`, `Best return`, `Lowest drawdown`

### 6.3 Run table columns
| Column | Notes |
|---|---|
| `☐` | row select (for compare) |
| `Run ID` | ULID, monospace, click → run detail |
| `Strategy` | link to draft |
| `Scenario` | scenario `display_name` |
| `Mode` | pill: `Backtest` / `Paper` |
| `Status` | status dot + label |
| `Sharpe` | numeric |
| `Total return` | %, color by sign |
| `Max drawdown` | %, coral |
| `Win rate` | % |
| `n trades` | count |
| `Tokens used` | mono |
| `Started` | relative ts |
| `⋯` | row menu: `Open`, `Compare with…`, `Re-run`, `Extract findings`, `Download tape`, `Delete` |

Empty state: `No runs yet. Pick a strategy and a scenario to start.`

### 6.4 New-run modal
| Field | Label | Control |
|---|---|---|
| Strategy | `Strategy` | searchable select |
| Scenario | `Scenario` | searchable select with previews (regime tags, time window) |
| Mode | `Mode` | radio: `Backtest` / `Paper` |
| Params override | `Override params (advanced)` | collapsible JSON editor |
| Token estimate panel | (auto-renders on selection) | `~53,500 tokens · ~120s estimated runtime` + warning chip if budget threshold passed |
| Footer | secondary `Cancel`, ghost `Estimate only`, primary `Start run` |

### 6.5 Chat rail context on this route

`Context: Run list (<n> runs in current filter)`. Quick replies: `Pick the most
suspicious run`, `Find runs that disagree on the same scenario`, `Suggest a new
scenario to test`.

---

## 7. `/eval/runs/<run_id>` — Run detail (live + completed)

Same template; live mode adds streaming SSE updates. v0.2 changes are confined
to §7.6 (Findings panel).

### 7.1 Header strip
| Element | Label |
|---|---|
| Title | run display name (or `Run <ULID short>`) |
| Subtitle | strategy name · scenario name · mode pill |
| Status badge | `● Running 42%` (with progress bar inline) / `● Completed` / `● Failed` |
| Header actions | `Stop` (live only, danger ghost) · `Re-run` · `Extract findings` · `Download tape` · `Compare with…` · **`Draft variant from this run →`** *(new in v0.2 — Move I, opens `/setup` with this run as context)* |

### 7.2 KPI tile row (4 across)
| Tile | Label | Value source |
|---|---|---|
| Total return | `Total return` | `metrics.total_return_pct` |
| Sharpe | `Sharpe` | `metrics.sharpe` |
| Max drawdown | `Max drawdown` | `metrics.max_drawdown_pct` |
| Win rate | `Win rate` | `metrics.win_rate` |

### 7.3 Equity curve panel
- Title: `Equity curve`
- Legend: this run (mint) + optional baseline overlay (`Buy & hold` slate)
- Toggle: `Show drawdown shading`
- Crosshair tooltip: timestamp · equity · pct change

### 7.4 Trade markers panel
- Title: `Trades on chart`
- Asset selector if multi-asset universe
- Marker glyphs: `▲ buy` (mint), `▼ sell` (coral), `○ flat` (slate), `✕ close` (white)
- Tooltip on marker: timestamp · action · size · fill price · realized PnL · (collapsible) decision JSON

### 7.5 Trade ledger table
Sortable. Columns: `Time`, `Asset`, `Side`, `Qty`, `Entry`, `Exit`, `PnL`,
`Holding period`, `Fees`, `Decision`. `Decision` opens drawer with prompt + LLM
output for that trade.

### 7.6 Findings panel — Move I integration

| Element | Label |
|---|---|
| Header | `Findings` + count chip + `Re-extract` ghost |
| Empty state | `No findings extracted yet.` `Extract now` button |
| Row | severity dot · finding `kind` · summary · `Evidence ↗` link · **`Draft variant from this →`** *(new — Move I)* · `Add to journal` ghost *(new — Move F)* |
| Severity legend | bottom-right: `● info` `● warning` `● critical` |

The **`Draft variant from this →`** button opens `/setup` with the wizard
agent pre-seeded:
- Context chip: `Context: finding <id> on run <ulid>`
- First user-side message: drafted from finding template (see §3.5)
- Right-pane "Strategy in progress" pre-loaded with the parent strategy as the
  fork basis. The user can confirm or redirect before any tool calls fire.

The **`Add to journal`** ghost button pins the finding to `/journal` with a
free-text note prompt — see §9 for journal mechanics.

### 7.7 Decisions tab (secondary)
Per-decision log: timestamp, slot name, model, tokens in/out, latency, raw
prompt + raw response (collapsible).

### 7.8 Live mode — extra row at top
- SSE event ticker: scrolling line `[10:42:17] decision.emitted long_open BTC/USD 0.05 conv=0.7`
- Pause / resume button
- Token usage running total

### 7.9 Chat rail context on this route

`Context: Run <ulid short> (<strategy>, <scenario>)`. Quick replies: `Why did
it underperform?`, `Compare to its baseline`, `Suggest a variant to draft`.

---

## 8. `/eval/compare?ids=<a>,<b>,…` — Comparison view

(Unchanged from v0.1 §7. Adding Move I trigger noted inline.)

### 8.1 Header strip
| Element | Label |
|---|---|
| Title | `Compare runs` |
| Subtitle | comma-separated run IDs (mono, each clickable) |
| Header actions | `Add another run…` · `Save comparison` · `Export PNG` · **`Draft variant from this comparison →`** *(new — Move I, seeds wizard with the diff between best and worst run)* |

### 8.2 Equity overlay panel
- Multi-line chart, one line per run, distinct colors from accent palette
- Shared time axis · synced crosshair
- Per-line legend with toggle visibility + per-line color swatch
- Mode toggle: `Equity` / `Cumulative return` / `Drawdown`

### 8.3 Trade markers panel
- Strategy toggle row (radio): show markers for one selected strategy at a time
- Same marker semantics as run detail §7.4

### 8.4 Metrics comparison table
Columns mirror run-detail KPIs. Best value per column gets a mint cell border.
Sortable.

### 8.5 Findings panel
Flat list across all runs in the set, grouped by `kind`. Each row has the same
`Draft variant from this →` button as §7.6.

### 8.6 Empty / partial states
- One run only: prompt `Pick at least one more run to compare.` with `Add run` button.
- One run still loading: that run's column shows `● Running 60%` placeholder bars.

### 8.7 Chat rail context

`Context: Compare <n> runs`. Quick replies: `What do the winners share?`,
`Why did <id_b> underperform <id_a>?`, `Suggest a synthesis variant`.

---

## 9. `/journal` — Lab Notebook (Move F — new in v0.2)

**Archetype:** chronological journal. The side-effect substrate of doing
strategy research finally has a home: findings + free-text notes + decisions
about what to address next, all in one append-only timeline that outlasts any
individual strategy.

**Layout:** main content is a single chronological column (max 880px wide for
readability). Right inspector rail surfaces filters + summary stats. Chat rail
docks outside the inspector rail.

### 9.1 Header strip

| Element | Label |
|---|---|
| Page title | `Journal` |
| Subtitle | `<n> entries · <n> findings · <n> notes` |
| Header actions | primary `New note` · ghost `Filter…` · ghost `Export` |

### 9.2 Entry types

The journal is a stream of typed entries. Each entry shares a header
(`<entry kind chip> · <ts relative> · <author chip — "you" or "system">`) and a
body that varies by kind.

| Kind | Body shape | Source |
|---|---|---|
| `finding` | severity dot · finding `kind` · summary · evidence link · source run + strategy chips · `Draft variant from this →` (Move I) | auto-pinned via `Add to journal` from §7.6 OR auto-promoted when severity = critical |
| `note` | free-text markdown · attached refs (run / strategy / finding chips) · edit / delete actions | user-authored |
| `postmortem` | structured fields: `What I tried`, `What worked`, `What didn't`, `Next` | user-authored via `New note → Postmortem template` |
| `wizard_recap` | summarized agent action: e.g. `Drafted eth-mr-v3 from finding <id>` · links to draft · `Continue conversation ↗` | auto-emitted whenever wizard creates / forks something |
| `deployment_event` | `paper-eth-mr started` / `paper-eth-mr stopped (manual)` / `paper-eth-mr errored` | auto from live cockpit events |
| `eval_milestone` | `eth-mr-v3 hit 5 evals · best Sharpe 1.42 vs baseline 0.81` | auto when a strategy crosses a meaningful threshold |

**Auto-pinning rules:**
- All `critical` findings auto-pin to journal.
- `warning` findings pin only when the user clicks `Add to journal`.
- `info` findings never auto-pin.
- All `deployment_event` entries auto-pin (low frequency, high signal).
- `eval_milestone` thresholds: 5/10/25/50 evals on one strategy; first
  passing eval on a new scenario; new best Sharpe.

### 9.3 Composer (sticky top below header)

| Element | Label |
|---|---|
| Composer textarea | placeholder `What did you learn?` · markdown supported |
| Attachment buttons | `+ Run`, `+ Strategy`, `+ Finding`, `+ Scenario` (each opens a search picker that inserts a chip ref) |
| Template select | `Note ▾`: `Note` / `Postmortem` / `Hypothesis` / `Decision log` |
| Submit | primary `Add to journal` (`⌘↵`) |

### 9.4 Right inspector rail

| Region | Label | Contents |
|---|---|---|
| Filters | `Filter` | toggles per entry kind · severity multi-select · strategy multi-select · scenario multi-select · date range |
| Pinned | `Pinned` | starred entries (manual `★` on any entry) |
| Stats | `Last 30 days` | findings count by severity (sparkline) · notes added · drafts forked from journal entries |
| Tags | `Tags` | user-added tags (`#chop`, `#funding`, `#regime-shift`) — clickable to filter |

### 9.5 Entry actions (per row)

`★ Pin` · `🔗 Copy link` · `📝 Add note to this entry` · `Draft variant from
this →` (where applicable) · `⋯ More` (`Edit`, `Delete`, `Tag…`).

### 9.6 Empty state

`Your journal is empty. The first finding from your first eval will land here.
You can also start with a hypothesis: New note → Hypothesis.`

### 9.7 Chat rail context

`Context: Journal (<filter summary>)`. Quick replies: `Summarize what I've
learned this week`, `What's my most repeated mistake?`, `Suggest a variant
based on recent findings`.

---

## 10. `/live` — Deployments list

(Unchanged from v0.1 §9.)

| Element | Label |
|---|---|
| Page title | `Live deployments` |
| Header actions | primary `New deployment` |
| Filter | mode select: `All / Paper / Live` |
| Table columns | `Strategy`, `Mode`, `Broker`, `Status`, `P&L today`, `Open positions`, `Last decision`, `Started`, `⋯` |
| Row action menu | `Open cockpit`, `Pause`, `Resume`, `Stop`, `View logs`, `Re-deploy` |
| Empty state | `No live runs yet.` + `Pick a strategy to deploy` button |

### 10.1 New deployment modal
| Field | Label | Control |
|---|---|---|
| Strategy | `Strategy` | searchable select |
| Mode | `Mode` | radio: `Paper` (default) / `Live (real money)` |
| Broker | `Broker` | select; populated by mode |
| Capital cap | `Capital cap (USD)` | numeric, required for Live |
| Daily loss kill | `Daily loss kill switch (%)` | numeric |
| Confirm checkbox (Live only) | `I understand this will trade real money on my broker account.` | required |
| Footer | secondary `Cancel`, primary `Deploy` |

---

## 11. `/live/<deployment_id>` — Live cockpit (Flight Deck)

**Archetype:** Flight Deck. v0.2 changes confined to §11.6 (risk panel —
Move I integration).

### 11.1 Header strip
| Element | Label |
|---|---|
| Title | strategy display name |
| Subtitle | mode · broker · started ts · uptime |
| Status pill | `● Running` / `● Paused` / `● Stopped` / `● Errored` |
| Header actions | `Pause` ghost · `Resume` ghost · `Stop` danger ghost · `Open in Inspector` · **`Draft variant from this deployment →`** *(new — Move I)* |

### 11.2 KPI gauge row (5 across)
| Tile | Label |
|---|---|
| `P&L today` | numeric, color by sign |
| `Equity` | running balance |
| `Open positions` | count |
| `Decisions / hr` | rate |
| `Tokens / hr` | rate |

### 11.3 Equity ticker chart
Wide chart, last 24h default. Time-range pills: `1h`, `4h`, `24h`, `7d`,
`30d`, `All`. Crosshair tooltip with action markers.

### 11.4 Open positions panel
Table: `Asset`, `Side`, `Qty`, `Entry`, `Mark`, `Unrealized PnL`, `Stop`, `TP`,
`Holding`. Row action: `Close` (danger ghost).

### 11.5 Live decision feed
Reverse-chronological list. Each row: timestamp, slot, action, size, conviction,
risk verdict pill (`passed` / `modified` / `vetoed`), `View prompt ↗`. Filter
chips: `All`, `Trades only`, `Vetoes only`.

### 11.6 Risk panel — Move I integration

- Donut: `passed` / `modified` / `vetoed` % across the run
- Below: `Veto reasons` mini-table — rule name + count + **`Draft variant from
  this →`** ghost button per high-frequency veto rule (≥3 occurrences)

The Move I button on a veto reason opens `/setup` seeded with `Context: <N>
vetoes for rule <name>` and a first message like `My deployment is getting
vetoed for <rule>. Let's revise.` (See §3.5.)

### 11.7 Provenance expander

| Row | Label | Contents |
|---|---|---|
| 1 | `Strategy bundle hash` | mono hash + `Copy` |
| 2 | `Code commit` | git rev → link |
| 3 | `8004 identity` | `tokenId` + Mantle explorer link (if on-chain) |
| 4 | `Reputation receipts` | count + `View feed →` |
| 5 | `Validation receipts` | count + `View feed →` |

In v1 (no marketplace, see Plan 5) the on-chain rows show `Off-chain` chip.

### 11.8 Errored state
Red card replacing the chart: `Deployment errored at <ts>` + last error
message + `Retry` / `Stop` / `Draft a fix in setup agent →` (Move I) buttons.

### 11.9 Chat rail context

`Context: Deployment <strategy> (<mode>)`. Quick replies: `Is this drift
real?`, `Should I pause it?`, `Draft a variant from yesterday's vetoes`.

---

## 12. `/eval/scenarios` — Scenario manager

(Unchanged from v0.1 §8.)

| Element | Label |
|---|---|
| Page title | `Scenarios` |
| Header actions | primary `New scenario` · ghost `Import JSON` |
| Tabs | `Canonical` (xvn-shipped) · `Custom` (user-created) |
| Card grid | one card per scenario |

### 12.1 Scenario card (grid item)
| Region | Contents |
|---|---|
| Title | scenario `display_name` |
| Subtitle | time window · asset count · regime tags as chips |
| Body | description (truncated 2 lines) |
| Footer | `Used by N runs` · `Best Sharpe across runs: X` · row actions: `Run with…`, `Edit`, `Duplicate`, `Delete` (custom only) |

### 12.2 New / Edit scenario form
(Same fields as v0.1 §8.2.)

---

## 13. `/settings` — Settings

(Unchanged from v0.1 §11.)

### 13.1 LLM keys
### 13.2 Brokers
### 13.3 Daemon & runtime
### 13.4 Identity (ERC-8004)
### 13.5 Danger zone

(See v0.1 doc for field-level detail; no changes in v0.2.)

---

## 14. Shared modals / overlays

| Modal | Trigger | Primary fields |
|---|---|---|
| Confirm destructive action | delete / wipe | typed-name input + `Cancel` / `Delete` |
| Token-budget warning | run start when est > threshold | estimate + `Cancel` / `Continue` |
| Slot tester drawer | per-slot `Test slot…` | sample input panel + `Run` + response panel + token count |
| Skill picker | `Add a skill` in Inspector | (see §4.4) |
| Fixture picker | `Change fixture ▾` in Inspector live preview | (see §4.2.2) |
| Command palette | `⌘K` | (see §1.5) |
| Toast | system events | (see §1.3) |

---

## 15. Status / state taxonomy (consistent visuals)

| Domain | States | Dot color |
|---|---|---|
| Run | `Queued`, `Running`, `Completed`, `Failed`, `Cancelled` | slate / info / mint / danger / tertiary |
| Deployment | `Running`, `Paused`, `Stopped`, `Errored` | mint / warn / tertiary / danger |
| Validation | `Ready`, `Warnings`, `Errors` | mint / warn / danger |
| Eval verdict | `Passed`, `Reportable`, `Failed` | mint / warn / danger |
| Risk verdict | `Passed`, `Modified`, `Vetoed` | mint / warn / danger |
| Finding severity | `info`, `warning`, `critical` | info / warn / danger |
| Daemon | `Connected`, `Offline` | mint / tertiary |
| LLM key | `Anthropic`, `OpenAI`, `OpenRouter`, `No key` | mint / mint / mint / warn |
| Journal entry kind | `finding`, `note`, `postmortem`, `wizard_recap`, `deployment_event`, `eval_milestone` | (use kind chips, not dots) |

---

## 16. Routes — at a glance

```
/                                 Control Tower (default landing, 2nd+ visit)
                                  → first-run only: redirects to /setup
/setup                            Wizard (first-run + on-demand full-page)
/strategies                       Strategy drafts list
/authoring/<draft_id>             Inspector (mechanical = form, LLM slots = split editor)
/eval/runs                        Run list / leaderboard
/eval/runs/<run_id>               Run detail (live + completed)
/eval/compare?ids=<a>,<b>,…       Comparison view
/eval/scenarios                   Scenario manager
/journal                          Lab Notebook (cross-run findings + notes)
/live                             Deployments list
/live/<deployment_id>             Live cockpit (Flight Deck)
/settings                         Settings
/settings/identity                Settings → identity sub-page
─────────────────────────────────
GLOBAL OVERLAYS (no route):
Chat rail                         Right side, every authenticated route
Command palette (⌘K)              Modal overlay, every authenticated route
─────────────────────────────────
DEFERRED (Plan 5 / post-hackathon):
/marketplace                      Listings grid (Spreadsheet archetype)
/marketplace/<listing_id>         Listing detail + buy
/strategies (lineage tree view)   Move G — defer until slot-machine variant pool ships
Pass-ribbon footer                Move H — ambient deployment ticker, defer
/lab                              Notebook / Lab Bench (deeper than /journal)
/canvas                           Node-graph composer
```

---

## 17. Cycle-to-route map (for the wireframer's mental model)

The five-move bundle was chosen because it gives every phase of the user's
loop a UI surface. This table is the one-page proof:

| Cycle phase | Route(s) | New surface? |
|---|---|---|
| 0. Notice | `/` Control Tower §2.2.1 "Needs attention" panel | ✓ NEW (Move A) |
| 1. Hypothesize | `/setup` Wizard · Chat rail anywhere · `Draft variant from this →` buttons | ✓ NEW back-edges (Moves B + I) |
| 2. Draft | `/authoring/<id>` Inspector · LLM slots get live preview | ✓ NEW preview pane (Move E) |
| 3. Test | `/eval/runs` · `/eval/runs/<id>` · `/eval/compare` | unchanged |
| 4. Diagnose | `/eval/runs/<id>` Findings · `/eval/compare` Findings | enhanced with Move I + Move F buttons |
| 5. Deploy | `/live` · new deployment modal | unchanged |
| 6. Observe | `/live/<id>` Flight Deck | enhanced with Move I on veto rules |
| 7. Reflect | `/journal` Lab Notebook | ✓ NEW route (Move F) |

Every phase that had no surface in v0.1 (0 and 7) now has one. Every back-edge
from Diagnose / Observe to Hypothesize (the most-traveled edges) has a one-
click affordance via `Draft variant from this →`.

---

## 18. Wireframer notes (open questions to resolve in design)

1. **First-run detection.** What makes `/` redirect to `/setup`? Probably "no
   LLM key OR no strategies AND no runs." Spec the exact predicate so the
   redirect is testable.
2. **Chat rail context-handoff continuity.** When the user clicks a `Draft
   variant from this →` button, the rail collapses (because they're now on
   `/setup` where the chat is the page). When they finish drafting and
   navigate away, the rail re-expands with the same conversation visible? Or
   does the conversation reset? Lean: persists for the session, with a `Start
   fresh` button at the top of the rail.
3. **Live preview cost.** The Inspector live preview re-fires on every prompt
   keystroke (debounced 2s). At an avg 1,400 tokens per fixture run that's
   real money on the user's account. Default the auto-rerun toggle to OFF for
   the first edit session of a slot, then surface a "you can turn on
   auto-rerun if you want continuous feedback" hint after the user manually
   runs it twice.
4. **Journal as inbox.** Should `/journal` show unread/read state? A
   never-visited critical finding pinned a week ago should still feel "fresh."
   Lean: `Unread` chip on entries the user hasn't expanded since they were
   pinned.
5. **Wizard nudges.** The Control Tower wizard nudge (e.g. "I noticed 3 chop
   failures…") is generated by the wizard agent in the background. What
   triggers it? Lean: a daily summarization pass when the user opens `/`,
   capped at 1 nudge per session.
6. **Multi-asset trade markers.** Run detail and live cockpit both show
   markers on a price chart. With a multi-asset strategy, do we show a chart
   per asset (tabs) or one chart with a top selector? Lean: top selector.
7. **Findings density.** Some runs will produce 20+ findings. Should the
   findings panel collapse low-severity ones by default, or always render flat?
8. **Live cockpit refresh cadence.** SSE events arrive at sub-second; KPI
   gauges need throttle/debounce or they'll thrash. Pick a target update floor.
9. **Comparison view at N=10+ runs.** Color palette runs out around 8 runs.
   Decide strategy: pagination, "highlight 1 vs all faded," or palette change.
10. **Off-chain badge in provenance.** When marketplace lands (Plan 5), the
    `Off-chain` chip becomes a real on-chain link. Wireframes should show both
    states so the design contracts are visible now.
11. **Empty palette: 0 strategies, 0 runs, 0 deployments.** Each list page
    needs a designed empty state — not generic "no data" — that points to the
    setup agent or wizard rail as the canonical first action.

---

*Document version: 0.2 · 2026-05-09. Lives at
`/Users/edkennedy/Code/xianvec/docs/design/ui-elements.md`. Cross-references
listed in the header. v0.1 → v0.2 changelog at top.*
