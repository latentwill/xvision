# GPT Image 2 — XVN Dashboard Prompt Library

> Companion to `ux-field.md` (the eight UX archetypes × two engines).
> Generated via `ideonomy-plain` — abstraction-lift surfaced a shared design system every
> prompt parameterizes; combination produced the 10 × 2 = 20 prompt cells; the spectrum
> organon governs iteration depth (rough draft → refined → production hand-off).
> Picked dimensions: age (kept "contemporary 2026 fintech" as a guardrail), visibility (each
> prompt names what is emphasized vs collapsed), autonomy (each prompt signals AI-led vs
> user-led posture).

These are **draft prompts** — per the research, GPT Image 2 is best as a fast design-draft
generator. Use these to surface hierarchy and feel, then iterate before hand-off to
implementation.

---

## How to use

1. Open ChatGPT with GPT Image 2 selected.
2. Paste the **Shared Design System** block once at the top of the session — every prompt
   below assumes it is already in context.
3. Paste any prompt below. Generate. Iterate using the ladder at the bottom.
4. For best layout fidelity, request a **16:9** canvas (desktop dashboard) unless the prompt
   specifies otherwise.

---

## Shared Design System  (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: contemporary 2026 fintech tooling. Restrained, technical, confident.
Inspirations: Linear, Vercel, TradingView, Stripe Atlas. Avoid skeuomorphism, avoid
corporate stock illustration, avoid cartoonish wizard hats, avoid sparkles, avoid emoji
icons, avoid "AI generated" gloss.

Palette (dark mode default):
- Background: deep desaturated navy-charcoal (#0B0F14 primary surface, #11161D elevated
  surface, #1A2029 panel surface).
- Borders: low-contrast slate (#1F2630), never pure white, never #FFF — use a 1px
  semi-transparent slate border or a subtle inner glow.
- Text: high-contrast off-white #E8ECF1 for primary, muted #8B95A4 for secondary, dim
  #5A6573 for tertiary metadata.
- Accent (single): electric mint #5BE0A2 for primary actions, status-good, equity-up.
- Status: amber #F4B23A for warning, coral #F26A6A for danger / equity-down, sky #6EB4F2
  for info.
- Brand wordmark "xvn" rendered in a clean grotesk, lowercase, monospace-adjacent.

Typography:
- Sans-serif (Inter or similar) for UI labels, body, headings.
- Monospace (JetBrains Mono or similar) for numerics, tickers, IDs, code, hashes,
  percentages, tokens, prices.
- Tight tracking on numerics; tabular figures so columns align.

Components:
- Cards have 10px rounded corners and a 1px slate border (never white).
- Pill chips for tags / status (e.g., "Tier A", "paper", "conservative").
- Status dots: filled circle (●) plus a one-word label.
- Buttons: primary is mint fill on dark, secondary is ghost with slate border.
- Tables: thin row dividers, monospace numerics, sticky header.
- Sparklines and equity charts: thin 1.5px line, mint when up, coral when down, soft
  gradient fill, no axis chrome unless specified.
- Icons: thin-line 1.5px stroke (Lucide style); never colorful glyphs.

Density: moderate-to-high. This is a builder's tool, not a consumer app. Prefer information
density to whitespace splash; but breathe between primary regions.

Voice in copy: plain English. Templates are labeled "Buys dips", "Catches uptrends", not
"RSI mean reversion". Technical labels live behind an "Advanced" toggle.

Render quality: sharp 1x desktop screen, 16:9 by default, exactly as if it were a real app
screenshot — pixel grid clean, no painterly textures, no bokeh, no faux-3D.
```

---

## Prompt Library  (20 prompts, 10 archetypes × 2 engines)

Each prompt is a single block. Paste verbatim after the design system block above.

---

### 1. Wizard  —  chat-led, AI-types, default at `/`

**1a — Strategy Creation Engine**

```
App screen of the xvn AI Agent Wizard for building a crypto trading strategy. Dark mode,
desktop 16:9. Two-column layout: left ~58% is the chat conversation; right ~42% is a live
"visual progress" sidecar.

Top app bar: lowercase "xvn" wordmark on the left, a small "Wizard" pill in mint, breadcrumb
"build a strategy", and a settings cog on the right.

Left column (chat): scrollable thread with assistant messages on the left in a soft slate
card with a small mint avatar dot, user messages on the right in a tinted slate card. The
last assistant message reads: "Got it — using the 'Buys dips' template. Which asset should
it watch?" Below that message, three quick-reply chips in a row: "ETH", "BTC", "SOL".
Beneath the thread, a single-line composer with placeholder "type your reply…" and a
mint arrow-send button.

Right column (visual progress): card titled "Building eth-mr · v0 draft". Below the title,
a vertical list of seven tiny layer rows, each a slim card: ① Data ✓, ② Regime classifier
"buys dips" ✓, ③ Signal interpreter ✓, ④ Decision arbiter — in progress with a small mint
spinner, ⑤ Entry/Exit — pending, ⑥ Risk preset chip "Conservative" ✓, ⑦ Execution chip
"Alpaca paper" ✓. Bottom of right column: a thin progress bar labeled "Ready 60%" with
"Run preview eval" as a primary mint button.

Mood: calm, confident, contemporary builder tool. The user feels guided, not patronized.
```

**1b — Eval Engine**

```
App screen of the xvn AI Agent Wizard running an evaluation against a market scenario.
Dark mode, desktop 16:9. Two-column layout: left ~55% is chat; right ~45% is a live
scoreboard plus a streaming equity preview.

Top app bar: lowercase "xvn" wordmark, "Wizard" pill in mint, breadcrumb "evaluate
eth-mr".

Left column (chat): the assistant's last message reads: "Estimated 53,500 tokens for this
run, about 2 minutes. Ready?" Below, two primary action chips side by side: "Run" in mint
and "Estimate only" as ghost. A user message above reads: "use the bull q1-25 scenario".
Below, four small scenario cards in a 2×2 grid — "Crypto bull Q1-25" (selected with a mint
ring), "Crypto bear Q3-24", "Crypto chop Q2-25", "Flash crash 2024-08".

Right column (scoreboard): card titled "Run 01H8N7Z · backtest". A grid of four KPI tiles
with monospace values: Tokens 53,500, Runtime ~2m, Decisions 1,080, Status "awaiting
confirm". Below the KPIs, a thin equity line chart placeholder with a faint dashed line
and the label "streaming will appear here". Below that, a "Findings" section header with
"0 so far" in dim text.

Mood: pre-flight, settled, the moment before the run starts.
```

---

### 2. Inspector  —  form-led, structured fields per layer

**2a — Strategy Creation Engine**

```
App screen of an xvn strategy inspector — a structured form for editing every layer of a
trading strategy bundle. Dark mode, desktop 16:9. Single dense central panel with a left
nav rail and a right "validation summary" rail.

Left nav rail (220px): app wordmark, then a list of drafts with one item highlighted —
"eth-mr v1.2 (draft)", "btc-tf (shipped)", "cln-pp (draft)", a "+ new draft" affordance at
the bottom.

Center panel: title "Strategy Draft · eth-mr · v1.2-draft" with a hash chip "9f2c…". Below,
seven collapsible layer rows, three open and four collapsed. Open rows: ① Data layer
showing "OHLCV alpaca" and indicator chips "rsi", "bb", "atr"; ④ Decision arbiter showing
a multi-line prompt textarea with the visible string "use rsi<30 + bb_lower touch → long;
exit on rsi>70 or 2:1 RR" and a model dropdown set to "claude-sonnet-4.6"; ⑥ Risk showing
a segmented control with three options where "Conservative" is selected, plus three small
metric tiles "max 1 pos", "1.5% / trade", "daily kill 5%". Collapsed rows show only their
title and a one-line summary.

Right rail (280px): a "Validation" card listing checks with mint check marks ("3 LLM slots
filled", "Risk valid", "Tools authorized") and an estimate block "tokens / run: 53.5k".
Below, a stacked button group: ghost "Validate", primary mint "Run eval", ghost with caret
"Publish".

Mood: power-user, structured, every field accountable.
```

**2b — Eval Engine**

```
App screen of an xvn evaluation configurator — a single dense form for kicking off one
backtest or paper run. Dark mode, desktop 16:9. Centered card on a soft panel surface,
~720px wide.

Card title: "New evaluation run". Subtitle below: "01H8N7Z · backtest mode". Card body is
a vertical form with labeled rows:
- "Strategy" — readonly chip "eth-mr@v1.2-draft" with hash "9f2c…"
- "Scenario" — dropdown set to "Crypto bull Q1-25" with a small calendar icon
- "Mode" — segmented control with two options, "Backtest" selected (mint), "Paper" inactive
- "Params" — three labeled number inputs in a row: rsi_oversold = 25, bb_period = 20,
  stop_atr = 2.0
- "Seed" — single number input "12345" with a refresh icon

Below the form, a divider then an "Estimate" panel: three pill metrics in a row — "53,500
tokens" (with sub-row "input 45k · output 8.5k"), "~120s runtime", "1,080 decision points".

Footer of the card: a left-aligned ghost "Estimate only" button, a right-aligned primary
mint "Run" button, and a faint "cancel" text link far right.

Mood: precise, technical, ready-to-fire.
```

---

### 3. Canvas  —  spatial node graph, drag-drop wire

**3a — Strategy Creation Engine**

```
App screen of a node-graph canvas for composing an xvn trading strategy visually. Dark mode,
desktop 16:9. Full-bleed canvas with a left tool drawer and a right inspector panel.

Canvas content: five rounded rectangle nodes arranged left-to-right with connecting wires:
"Data · ohlcv" → "② Regime classifier" → "③ Signal interpreter" → "④ Decision arbiter" →
"Broker · alpaca-paper". Each node has a colored top-stripe (mint for LLM slots, slate for
mechanical), an icon, a one-line label, and small input/output pin dots. A secondary wire
loops below labeled "indicator panel" and "risk veto" branching into the trader node.
Background canvas has a subtle dotted grid (#1A2029 dots on #0B0F14).

Left tool drawer (220px): a "Skill drawer" with a search input "search skills…" and a
scrollable list of draggable skill chips: "regime-classifier-base", "news-aware-decision",
"risk-conservative", "crypto-trader-base", each with a small grip handle.

Right inspector panel (320px): "④ Decision arbiter" header, then editable fields — Prompt
(multi-line textarea with visible text "use rsi<30 + bb_lower → long…"), Model dropdown
("claude-sonnet-4.6"), Tools checkboxes (ohlcv ✓, indicator_panel ✓, position ✓), Output
schema with a small "JSON schema" link.

Floating top-right: zoom controls (− 100% +) and a "Run preview eval" mint button.

Mood: spatial, expressive, designer-oriented power tool.
```

**3b — Eval Engine**

```
App screen of an xvn eval canvas — drag a strategy and a scenario together to fire a run,
or drop two completed runs onto a comparison node. Dark mode, desktop 16:9. Full-bleed
canvas, dotted-grid background.

Canvas content: top-left a card "Strategy · eth-mr@v1.2" with a hash and a small drag
handle; top-right a card "Scenario · Crypto bull Q1-25" with a calendar icon. Both nodes
wire down into a central diamond-shaped action node "Run ▶" rendered in mint. From the
Run node, three output wires fan downward into three result nodes: "Metrics" (with KPIs
sharpe 1.62, dd −7.1%, trades 47), "Findings" (with three small severity chips "info",
"warn", "info"), and "Attestation" (with a signature glyph and "signed by @author").

Below the run, a separate ghosted "Compare" node with two drop-zone slots labeled "drop
run A" and "drop run B".

Left rail (200px): library tabs "Strategies", "Scenarios", "Runs". Currently "Runs" is
active, listing four small run cards with status dots — "01H8N ✓", "01J2P ✓", "01K9R
running 41%", "01M4T queued".

Top-right floating: zoom controls and a "Save canvas" ghost button.

Mood: composable, exploratory, the run as a wirable artifact.
```

---

### 4. Notebook  —  cell-based REPL, max reversibility

**4a — Strategy Creation Engine**

```
App screen of an xvn notebook for programmatically building a trading strategy via the MCP
API. Dark mode, desktop 16:9. Single centered column ~860px wide on a darker page surface,
mimicking a Jupyter / Observable notebook.

Top bar: file name "building_eth_mr.xvnnb", a kernel status pill "xvn-engine · ready" in
mint, and "Run all" / "Restart" ghost buttons.

Notebook body: five stacked cells, each with a left gutter "[1]" through "[5]" in dim mono.
Each cell has a code area on top (monospace, syntax-highlighted) and an output area below
(slightly indented, with a "╰▶" gutter glyph).

- Cell 1: code "draft = template('mean_reversion')"; output "draft_id = drft_01H8N…"
- Cell 2: code "set_prompt(draft, 'trader', '''use rsi<30 + bb_lower → long…''')"; output
  "slot ④ updated · 124 tokens"
- Cell 3: code "attach_skill(draft, 'trader', 'news-aware-decision@1.0')"; output
  "✓ skill composed · agent prompt now 312 tokens"
- Cell 4: code "validate(draft)"; output "✓ 3 LLM slots filled · ✓ risk valid · ✓ ready"
- Cell 5: code "run_eval(draft, 'bull-q1-25')"; output is a thin live progress bar 41%
  with "01H8N…" run id below

Active cell has a mint left border. Below the last cell: an empty "+ new cell" affordance.

Mood: code-first, programmable, L4 researcher tool.
```

**4b — Eval Engine**

```
App screen of an xvn evaluation notebook — sequential cells for estimating, running,
metricking, comparing. Dark mode, desktop 16:9. Single centered column ~860px wide.

Top bar: file name "eval_eth_mr.xvnnb", kernel pill "xvn-engine · ready", "Run all" button.

Notebook body: six cells:
- Cell 1: "est = estimate(eth-mr, 'bull-q1-25')" → output JSON "{ tokens: 53500, runtime_s:
  120, decision_points: 1080 }"
- Cell 2: "run = run_eval(eth-mr, 'bull-q1-25', mode='backtest', seed=12345)" → output
  "run_id = 01H8N…" then a streaming progress bar at 41% with "ETA 50s" beside it
- Cell 3: "metrics(run)" → output a small table of monospace KPIs: sharpe 1.62, max_dd
  −7.1%, n_trades 47, win_rate 0.58
- Cell 4: "findings(run)" → output a JSON array of two finding objects, syntax-colored,
  with kind / severity / evidence keys visible
- Cell 5: "compare([run, prior_run_id])" → inline equity chart placeholder with two
  overlaid lines, mint and sky
- Cell 6: empty cell with cursor caret

Mood: data-science feel; researcher iterating; outputs are live.
```

---

### 5. Control Tower  —  multi-pane mission control

**5a — Strategy Creation Engine**

```
App screen of an xvn strategy control tower — multi-pane workspace for power users
managing several drafts at once. Dark mode, desktop 16:9. Three columns and a top header.

Top header: "xvn" wordmark, breadcrumb "Drafts", a search input, a notifications bell with
a "2" badge.

Left column (260px) — Drafts list: header "Drafts", then six draft items, the active one
"eth-mr" highlighted with a mint bar; below the list a "Templates" subsection with eight
items "Trend", "Breakout", "Mean reversion (selected)", "Momentum", "Range", "Scalping",
"News", "Custom". A "+ new" affordance pins to the bottom.

Center column (flex) — Active draft "eth-mr": four stacked cards in a 2×2 grid:
- "Slots" card showing ② regime ✓, ③ intern ✓, ④ trader (in progress, spinner)
- "Eval preview" card showing sharpe 1.62 ▲, drawdown −7.1%, trades 47 in monospace
- "Skills attached" card listing two pills "news-aware-decision", "risk-conservative"
- "Tier" card with two radio rows "Tier A · open" (selected, mint), "Tier B · sealed"

Right column (320px): top half is a "Wizard chat" panel showing "idle — type to engage"
with a composer below; bottom half is "Activity log" with three rows: "13:04 slot ④ prompt
updated", "13:05 risk preset → Conservative", "13:07 attached skill news-aware-decision".

Mood: dense ops console, multiple objects in flight, calm coordination.
```

**5b — Eval Engine**

```
App screen of an xvn eval control tower — live mission control while one run is streaming
and a queue waits behind. Dark mode, desktop 16:9. Three columns and a top header.

Top header: wordmark, breadcrumb "Runs", a "Queue: 4" pill, a notifications bell.

Left column (260px) — Run queue: subsections "Active" with "01H8N · eth-mr" selected (mint
bar, spinner glyph), "Queued" with three items "btc-tf", "cln-pp", "mom-1", and "Completed"
with three items "cmp-1 ✓", "bb-2 ✓", "dchn ✓".

Center column (flex) — Active run "01H8N · bull-q1-25": large equity line chart at the
top, ~280px tall, mint line ascending with one mid-chart drawdown notch and a small ▲
peak marker; below the chart, a thin progress bar at 56% labeled "tokens 29k / ~53k · ETA
~50s"; below progress, a metrics strip with five tiles "sharpe 1.62 ▲", "dd −7.1%",
"trades 47", "tokens 29k", "findings ◐ 3".

Right column (320px): top is a "Ticker" panel showing five SSE-feed rows with timestamps
and event chips — "13:04 +long BTC 0.05", "13:08 fill 95400.0", "13:14 −short BTC 0.02",
"13:18 finding: regime drift", "13:22 metrics updated"; bottom is "Findings extracted" —
two warning rows "regime_drift · info", "overtrading · warn" with severity dots.

Mood: live operations, streaming, every gauge lit.
```

---

### 6. Flight Deck  —  dense gauge cluster, few big buttons

**6a — Strategy Creation Engine**

```
App screen of an xvn pre-flight checklist for a strategy about to be deployed to paper
trading. Dark mode, desktop 16:9. Single full-bleed page; cockpit-feel.

Top bar: wordmark, breadcrumb "eth-mr · pre-flight".

Page body: a row of five large status tiles spanning the full width, each ~220px wide,
each rendered as a card with a thin slate border and a soft inner-glow border:
- "Slots" tile: shows "②③ ✓" big and "④ ⏳" smaller below
- "Risk" tile: shows a mint chip "Conservative" and "kill ✓" below
- "Tokens / week" tile: monospace "~12k" big with subtitle "estimated cost"
- "Tier" tile: shows "A" big with "open" subtitle
- "Ready" tile: a circular progress ring at 80% in mint with "80%" centered

Below the tile row, ~80px of breathing space, then three large primary action buttons
across the page bottom, each ~280px wide and 64px tall: ghost "Validate" on the left,
mint primary "Paper deploy ▶" in the center (the only mint button), ghost with caret
"Publish" on the right.

Mood: cockpit, the moment before takeoff, decisive.
```

**6b — Eval Engine**

```
App screen of an xvn run cockpit — live in-flight gauges while a backtest streams. Dark
mode, desktop 16:9. Single full-bleed page; cockpit feel.

Top bar: wordmark, breadcrumb "run 01H8N7Z · in flight", a small "Backtest · bull-q1-25"
pill.

Page body: a row of five gauge tiles spanning full width:
- "Sharpe" tile: monospace "1.62" with a small mint up-triangle
- "Drawdown" tile: monospace "−7.1%" in coral
- "Trades" tile: "47" with a tiny sparkline beneath
- "Tokens" tile: "29k" with a thin progress arc beneath ("of ~53k")
- "Findings" tile: a small severity dot ◐ and "3 new" beneath

Below the gauges, a large progress strip ~70px tall: a thick bar at 41% in mint with a
soft gradient fill, labeled "ETA 50s · 442 / 1,080 decisions", with a small embedded
sparkline showing tokens-burn rate.

Below the progress, four large action buttons: ghost "Pause", danger ghost "Abort" (in
coral text), ghost "Compare …", ghost with caret "Publish ↗".

Mood: cockpit, in-flight, gauges alive, controls ready.
```

---

### 7. Spreadsheet  —  tabular sweep, whole catalog at once

**7a — Strategy Creation Engine**

```
App screen of an xvn strategy template gallery rendered as a sortable spreadsheet. Dark
mode, desktop 16:9. Top header, then a wide table that takes the rest of the page.

Top header: wordmark, breadcrumb "Templates", a search input "filter templates…", a
"Create from scratch" ghost button.

Table: eight rows, seven columns, sticky header, monospace numerics. Columns: "Template",
"Regime", "Assets", "Price", "Tier", "Sharpe ▼" (active sort, mint caret), "Status".

Rows (visible content):
- "Trend" · "↑ trend" · "BTC, ETH" · "Free" · "A" · "1.91" · star pinned
- "Breakout" · "↑ vol" · "BTC" · "Free" · "A" · "1.78" · empty
- "Mean reversion" · "↔ range" · "ETH" · "Free" · "A" · "1.62" · "Editing" pill in mint
- "Momentum" · "↑ trend" · "BTC, ETH" · "Free" · "A" · "1.55" · empty
- "Scalping" · "μ-struct" · "BTC" · "Free" · "A" · "0.94" · empty
- "News trader" · "event" · "ETH" · "$5/mo" · "B" · "1.40" · empty
- "Custom" · "any" · "any" · "Free" · "A" · "—" · empty
- "On-chain" · "flows" · "BTC, ETH" · "Free" · "A" · "1.71" · empty

Footer strip: small dim text "click row → inspector · shift-click → batch eval · ⌘-click
→ fork as new draft".

Mood: bloomberg-lite, scannable, rank-sortable.
```

**7b — Eval Engine**

```
App screen of an xvn parameter sweep — paid-tier batch eval rendered as a heatmap-style
matrix. Dark mode, desktop 16:9.

Top bar: wordmark, breadcrumb "Sweep · eth-mr · bull-q1-25", a chip "16 runs · 4 running
· 12 done", a primary mint "Run sweep" button (disabled, sweep already running).

Page body: a large rectangular grid taking ~70% of the page. Rows are bb_period values
(14, 20, 30, 50). Columns are rsi_oversold values (20, 25, 30, 35, 40). Each cell shows a
sharpe number in monospace; cell background color encodes value via a green-to-coral
gradient (high values mint-tinted, low values coral-tinted, ~50% saturation, never pure).
The cell at bb_period=20, rsi_oversold=20 reads "1.62" with a small ▲ glyph and a subtle
mint ring indicating it is selected.

Right rail (300px): "Drilldown · run 01H8N · bb20 / rsi20" header; below it a small equity
line chart, then a metrics list (sharpe 1.62, dd −7.1%, trades 47, tokens 29k), then three
finding rows with severity dots.

Below the heatmap, two side-by-side smaller heatmaps labeled "seed #1234" and "seed
#5678", each 4×5, intensity-shaded, showing seed-stability.

Mood: research-lab spreadsheet, data-dense, inviting drilldown.
```

---

### 8. Lab Bench  —  journal + tray, everything versioned

**8a — Strategy Creation Engine**

```
App screen of an xvn strategy lab bench — a researcher's persistent workspace where every
edit is a commit and every draft is a branchable tree. Dark mode, desktop 16:9. Two
columns plus a top header.

Top header: wordmark, breadcrumb "Lab · eth-mr line", a small git-style branch indicator
"v1.2 · 7 commits".

Left column (flex) — Journal: a vertical timeline with seven entries. Each entry has a
timestamp on the left in dim mono ("13:14", "13:22", etc.), a small commit-dot in mint, and
a content card with a one-line summary plus a tiny diff preview. Sample entries:
- "13:14 forked btc-tf → eth-mr; switched ETH; rsi 30→25"
- "13:22 ran bull-q1: sharpe 1.62 · finding regime_drift"
- "13:30 branched eth-mr → eth-mr-v2 ; tightened stop"
- "13:42 attached skill news-aware-decision@1.0"
- "13:50 ran bull-q1 again: sharpe 1.71 ↑"
- "13:55 cherry-picked stop change back to eth-mr@v1.3"

The latest entry has a blinking caret indicating an active session.

Right column (300px) — Tray: header "Drafts in tray". A small tree-list of three drafts:
- "btc-tf" with a "shipped" pill
- "eth-mr" expanded showing nested versions v1.0, v1.1, v1.2 (with v1.2 marked active)
- "eth-mr-v2" with "draft" pill
At the bottom of the tray, "+ new draft" ghost button and a "diff any two →" link.

Mood: researcher's lab notebook, every action recoverable, history-as-first-class.
```

**8b — Eval Engine**

```
App screen of an xvn eval lab bench — every run is permanent, diff-able, attestable. Dark
mode, desktop 16:9. Two columns plus a top header.

Top header: wordmark, breadcrumb "Lab · runs of eth-mr", a chip "12 runs · 4 attested ·
1 running".

Left column (flex) — Journal: vertical timeline of eight entries with timestamps and run
dots:
- "12:00 run 01H8N · eth-mr@v1.2 · bull-q1 — sharpe 1.62 ✓" (mint dot)
- "12:14 run 01J2P · eth-mr@v1.2 · chop-q2 — sharpe 0.41 ✗" (coral dot)
- "12:22 attestation signed for bull-q1 → marketplace" (with a small signature glyph)
- "12:30 run 01K9R · eth-mr@v1.3 · bear-q3 — running 41%" (animated mint dot)
- "12:40 finding extracted: tail-risk concentration" (amber severity chip)
- "12:55 diff(01H8N, 01J2P) → regime_fit_mismatch" (with a small split-arrow glyph)

The active run entry has a thin progress bar inline.

Right column (300px) — Tray: header "Runs". List of four runs as cards: 01H8N (✓ done),
01J2P (✓ done), 01K9R (live progress bar 41%), 01M4T (queued, dim). Below the list, two
ghost buttons stacked: "+ new run" and "↻ re-run all".

Mood: researcher's run journal, immutable, signable, comparable.
```

---

### 9. Ticker  ◇  coinage — ambient persistent strip

> Surfaced via negation of "page = run". The run lives off-page in a thin always-present
> strip; the user navigates freely while runs stream. Render this as a *fragment* (top or
> bottom strip) rather than a full screen.

**9a — Strategy Creation Engine**

```
UI fragment, not a full screen — render only a horizontal strip ~64px tall spanning a
desktop window width 1440px, dark mode. Place this strip at the bottom of the viewport,
docked, with a thin slate top-border and a soft drop-shadow on the upper edge to indicate
it floats above page content (which can be a faded blurred surface behind it).

Strip content, left to right:
- A small mint pulsing dot, then "drafts" label.
- Three pill cards in a row, each ~280px wide, separated by 12px gaps: "eth-mr · slot ④
  ⏳ filling" with a small spinner; "btc-tf · ✓ shipped" with a mint check; "cln-pp ·
  draft · 3 days idle" in dim text with an amber dot.
- A vertical divider, then "+ new draft" as a ghost mini-button.
- Far right: a small expand-up chevron icon to pop the strip into full Drafts view.

Aspect: very wide and short. No page chrome, no header, no other panels — only the strip.
```

**9b — Eval Engine**

```
UI fragment — render only a horizontal strip ~64px tall at the bottom of a 1440px desktop
viewport, dark mode, docked, thin slate top-border, soft upper drop-shadow.

Strip content, left to right:
- A mint pulsing dot, then "runs" label.
- Three live run pill cards, each ~320px wide:
  - "01H8N · eth-mr · bull-q1 ▓▓▓▓▓░░░ 41%" with a thin inline mint progress bar and an
    "ETA 50s" mono label.
  - "01J2P · ✓ done · sharpe 0.41" with a small "view ↗" link in mint.
  - "01K9R · queued" in dim text.
- A vertical divider.
- A small "Compare 2 selected" ghost button (currently disabled / dim).
- Far right: a small expand-up chevron for the full Runs view.

Aspect: very wide and short. The strip persists across page navigation — the design must
read as "ambient", not as a primary surface.
```

---

### 10. Slot Machine  ◇  coinage — three-reel configurator for L1 onboarding

> Surfaced via negation of "linear flow". Wizard's lite cousin for users who can't yet
> articulate intent. Render as a single landing card.

**10a — Strategy Creation Engine**

```
App screen of an xvn "starter spinner" — a single hero card that suggests a complete
strategy by spinning three reels. Dark mode, desktop 16:9, but the active surface is one
centered card ~960px wide on a dimmed page background.

Card title: "Not sure where to start? Spin a starter." Subtitle: "We'll suggest a complete
strategy you can run in paper mode in under a minute."

Card body: three large reel-style picker columns side by side, each ~260px wide and
~340px tall, each with a beveled inner edge giving a slot-machine feel but in a flat,
modern style (no skeuomorphic bevels — use mint accents). Each reel shows three values
stacked vertically with the middle value highlighted by a mint bar:
- Reel 1 "Template": "Trend", "Breakout", "Mean reversion" (highlighted), "Momentum",
  "Range".
- Reel 2 "Asset": "ETH", "BTC" (highlighted), "SOL", "WBTC", "AVAX".
- Reel 3 "Risk": "Aggressive", "Balanced", "Conservative" (highlighted).

Below the reels, a thin status row: "Suggested: Mean reversion · BTC · Conservative · est.
12k tokens / week · paper mode default".

Card footer: three buttons in a row — ghost circular "↻ Pull again", primary mint "Keep
this →", and ghost "Tweak in inspector".

Mood: playful but restrained, low-stakes onboarding for L1 users.
```

**10b — Eval Engine**

```
App screen of an xvn "scenario spinner" — three reels that pick a random eval scenario
combo for users who want to stress-test their strategy without thinking about which
scenario to pick. Dark mode, desktop 16:9, single centered card ~960px wide.

Card title: "Random shake-down eval". Subtitle: "Pick three scenarios at random and run
your strategy against each. Tokens estimated before any run starts."

Card body: three reel pickers like a slot machine, side by side, each ~260px wide:
- Reel 1 "Regime": "Bull Q1-25", "Bear Q3-24" (highlighted), "Chop Q2-25", "Flash crash
  2024-08", "Sideways Q4-23".
- Reel 2 "Asset universe": "BTC only", "ETH only", "BTC + ETH" (highlighted), "Top 5",
  "Long tail".
- Reel 3 "Capital": "$1k", "$10k" (highlighted), "$100k", "$1M".

Below the reels, an estimate strip in monospace: "3 runs · ~160k tokens · ~6 minutes
total · backtest mode". A faint "buyer pays own LLM tokens" footnote in dim text.

Card footer: three buttons — ghost "↻ Spin again", primary mint "Run all 3 →", ghost
"Configure manually".

Mood: playful exploration, but the numbers are honest — token-cost is always shown.
```

---

## Iteration ladder  (the spectrum from rough draft → handoff)

GPT Image 2 is best at the rough-draft end. Walk down the spectrum after the first
generation:

1. **Draft pass.** Use the prompt verbatim. Generate 2–3 variations. Pick the one whose
   *hierarchy* is strongest — which screen makes you instantly understand where to look
   first. Don't worry about copy fidelity yet.

2. **Hierarchy tightening.** Re-prompt with: "Same screen, but make the [primary action /
   primary metric / live region] visually dominant. Reduce competing emphasis on [X]."
   Generate 1–2 variants.

3. **Copy fidelity.** Re-prompt with: "Same screen, but the exact copy on [region X] must
   read '[verbatim string from the spec]'. Keep all other layout identical." GPT Image 2 is
   strong at text — use it. Repeat for any region whose copy drifted.

4. **Density calibration.** If the screen is too sparse, re-prompt: "Increase information
   density — show 8 rows in the table instead of 4; add a secondary metrics strip below
   the chart." If too dense, the inverse.

5. **Component handoff.** Once you have a picked-direction screen, stop iterating in image
   space and go to code. Use the screen as a reference image alongside shadcn/ui or
   Tailwind. The image is now an *intent doc*, not a deliverable.

---

## Style nudges  (from the picked dimensions)

- **Visibility (what is emphasized vs collapsed):** every screen makes a visibility
  decision. Wizard hides complexity (visual progress is a *summary*, not a panel of every
  field); Inspector surfaces every field (collapsed by default, expandable); Lab Bench
  surfaces history. When iterating, ask: "what is this screen choosing to hide, and is the
  choice load-bearing?"

- **Autonomy (AI-led vs user-led):** signal posture in the chrome. AI-led screens (Wizard,
  Slot Machine) show typing indicators, suggested-reply chips, AI avatar dots. User-led
  screens (Inspector, Notebook, Lab Bench) show cursors, command palettes, code gutters.
  Don't mix the two postures within one screen — pick one and lean.

- **Age (era of the design):** explicitly target "contemporary 2026 fintech tooling".
  Counter-prompt against: skeuomorphic glass, drop-shadow-heavy 2010s flat, oversaturated
  consumer-app gradients, AI-generated gloss. The reference family is Linear / Vercel /
  Stripe / TradingView, not Robinhood and not Bloomberg.

- **No 100% white borders in dark mode** (project rule): if a generated screen has
  pure-white borders or `#fff` lines, regenerate with the explicit instruction "borders
  are 1px slate (#1F2630), never white, never #FFF".
