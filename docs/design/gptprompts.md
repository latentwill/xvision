# GPT Image 2 — XVN Dashboard Prompt Library

> Companion to `ui-elements.md` v0.2 (canonical surface inventory),
> `themes.md` v0.1 (canonical visual tokens — Folio theme), and `ux-field.md`
> (the eight UX archetypes). The Shared Design System block below encodes
> Folio; if `themes.md` and this doc disagree, `themes.md` wins.
>
> These are **draft prompts** — GPT Image 2 is best as a fast design-draft
> generator. Use these to surface hierarchy and feel, then iterate before
> hand-off to implementation.
>
> ## Changelog — v0.3 (2026-05-09)
>
> Added six prompts for the autoresearch + marketplace surfaces from the
> [autoresearcher spec](../superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md)
> and the [marketplace plugin spec](../superpowers/specs/2026-05-09-marketplace-plugin-design.md):
>
> - §11 Live Evening Cycle Viewer — the autoresearch demo headline
> - §12 Genealogy Tree — every variant ever born, with parents, children, ghosts
> - §13 Mutation Diff Inspector — three orthogonal panes (prose / param / tool) + finding + inversion check
> - §14 Mutator-Skill Ladder — the second ladder, measuring the LLM mutator itself
> - §15 Ladder with Provenance — augments the existing strategy ladder with lineage depth + parent hash
> - §16 Marketplace Tab — Persona B surface, only visible with `--features marketplace`
>
> v1 in-scope grew from 10 → 16 prompts. Deferred archetypes renumbered §11–§15 → §17–§21.
>
> ## Changelog — v0.2 (2026-05-09)
>
> Restructured around the v0.2 route map. Two changes:
>
> **(a) Engines unified.** The original library had `Xa` and `Xb` per
> archetype (Strategy Creation Engine vs Eval Engine). v0.2 ui-elements treats
> these as one product with a unified route tree, not parallel surfaces — so
> each archetype now gets one canonical prompt. Eval surfaces have their own
> archetypes (Run Detail, Compare, Run List) instead of being a side-by-side
> mirror.
>
> **(b) Scope split: in-v1 vs deferred.** The 10 archetypes from
> `ux-field.md` are now grouped into:
> - **v1 in-scope** (10 prompts in v0.2; 16 in v0.3) — Control Tower, Wizard,
>   Inspector (with live-preview split), Lab Notebook, Flight Deck (with Move I),
>   Run Detail (with Move I), Compare, Run List + Scenarios, Chat Rail fragment,
>   Command Palette overlay; v0.3 adds the autoresearch + marketplace prompts.
> - **Deferred** (5 prompts, preserved verbatim from v0.1 for future use) —
>   Canvas, Power Notebook, Spreadsheet, Ticker / Pass-ribbon, Slot Machine.
>
> The five v0.2 ★ moves (Control Tower home, chat-permanent rail, Lab Notebook,
> live-preview Inspector, Phase-4→1 "Draft variant from this →" buttons) are
> baked into the in-scope prompts.

---

## How to use

1. Open ChatGPT with GPT Image 2 selected.
2. Paste the **Shared Design System** block once at the top of the session —
   every prompt below assumes it is already in context.
3. Paste any prompt below. Generate. Iterate using the ladder at the bottom.
4. For best layout fidelity, request a **16:9** canvas (desktop dashboard)
   unless the prompt specifies otherwise (Chat Rail fragment is portrait;
   Command Palette is square-ish).

---

## Shared Design System  (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: contemporary 2026 fintech tooling, with a quietly classical typographic
register. Restrained, technical, confident. Inspirations: NousResearch (the quieter pages),
Linear, Vercel, TradingView. Avoid skeuomorphism, avoid corporate stock illustration, avoid
parchment textures, avoid alchemical sigils, avoid wizard-hat or scroll iconography, avoid
cartoonish ornament, avoid sparkles, avoid emoji icons, avoid "AI generated" gloss.

Theme name: Folio. The visual register hints at considered typography on a warm dark
ground — flirting with classical, never arcane.

Palette (dark mode default — Folio theme):
- Background: warm graphite (#0E1014 primary surface, #15171C elevated surface, #1C1F25
  panel surface). Slightly less blue than typical fintech dark mode.
- Borders: warm slate (#232730), 1px solid, never pure white, never #FFF.
- Text: warm off-white #ECE6D9 for primary (slight cream), muted #9A9387 for secondary,
  dim #5E5A52 for tertiary metadata.
- Accent (single): warm amber-gold #D4A547 for primary actions, status-good, equity-up.
  This is the single biggest visible identity — chart equity-up lines are amber-gold,
  not amber-gold. Equity-down is cinnabar.
- Status: warm amber #DB9230 for warning, cinnabar #C8443A for danger / equity-down,
  muted slate-blue #6E94BA for info.
- Brand wordmark "xvn" rendered in Cormorant Garamond italic, all lowercase. No monogram,
  no ligature, just the wordmark.

Typography:
- Display / heading: Cormorant Garamond (transitional serif). Sizes 24–48px. Italic
  permitted on subtitles. Used for page titles, card titles, brand mark.
- Body / UI: Inter (humanist sans). Sizes 13–16px. Used for labels, body copy, helper
  text. NOT serif — body stays sans for data scan speed.
- Numeric: JetBrains Mono. Sizes 12–14px. Tabular figures ON. Used for prices, hashes,
  IDs, percentages, ULIDs, all numeric data.

Components:
- Cards have 4px rounded corners (not 10px — hint of "printed page" rather than "pill")
  and a 1px warm-slate border.
- Pill chips for tags / status (e.g., "Tier A", "paper", "conservative") — same 4px
  radius family.
- Status dots: filled circle (●) plus a one-word label. The dot color shifts with status:
  amber-gold for good states (running/passed/completed), warm amber for warnings,
  cinnabar for danger/vetoed/failed, slate-blue for info/queued, dim for offline.
- Buttons: primary is amber-gold fill with dark text; secondary is ghost with warm-slate
  border; danger is ghost with cinnabar text. All 4px radius. No gradients, no drop
  shadows, no "illuminated initial" caps. Sentence case.
- Tables: thin row dividers in warm-slate, monospace numerics, sticky header.
- Sparklines and equity charts: thin 1.5px line, amber-gold when up, cinnabar when down,
  soft gradient fill, no axis chrome unless specified.
- Icons: thin-line 1.5px stroke (Lucide style); never colorful glyphs. No sigils.
- Texture: a 1–1.5% grain noise overlay on elevated surfaces (cards, modals); page
  background stays flat; chart data areas stay flat. The grain should be invisible at
  first glance — perceptible only when looked for.

Top nav (every authenticated route): "xvn" wordmark on the left, then nav links
"Home · Strategies · Eval · Live · Journal", then on the right: a chat-rail toggle icon
(speech bubble) with an optional unread amber-gold dot, a daemon status pill ("● Connected"),
an LLM provider pill ("● Anthropic"), and a circular avatar.

Chat rail (every authenticated route except /setup): a collapsible right-side rail, 360px
wide when expanded, 40px when collapsed (icon-strip). When expanded: header reads
"xvn agent · ● Online" with a context chip below ("Context: this run", "Context: editing
slot ④", etc.). Below the header is a chat thread (assistant on left, user on right) and
a composer at the bottom with route-aware placeholder. Use slate panel surface (#1A2029),
1px slate border on the left edge.

Status pattern: filled dot (●) plus a one-word label, never standalone color. Use
"Passed/Modified/Vetoed" for risk, "Queued/Running/Completed/Failed" for runs,
"info/warning/critical" for findings.

Density: moderate-to-high. This is a builder's tool, not a consumer app. Prefer information
density to whitespace splash; but breathe between primary regions.

Voice in copy: plain English. Templates are labeled "Buys dips", "Catches uptrends", not
"RSI mean reversion". Technical labels live behind an "Advanced" toggle.

Render quality: sharp 1x desktop screen, 16:9 by default, exactly as if it were a real app
screenshot — pixel grid clean, no painterly textures, no bokeh, no faux-3D.
```

---

## Prompt Library — v1 in-scope (16 prompts)

Each prompt is a single block. Paste verbatim after the design system block above.

---

### 1. Control Tower  —  default landing (`/`) for second-and-subsequent visits

```
App screen of the xvn home page — a "what needs your attention" dashboard for a power user
who has been away from the app for a day. Dark mode, desktop 16:9. The chat rail is docked
on the right, collapsed (40px icon strip showing the speech-bubble glyph and a small amber-gold
unread dot).

Top nav: lowercase "xvn" wordmark, then "Home" (active, amber-gold underline), "Strategies",
"Eval", "Live", "Journal". Right side: chat rail toggle (with the amber-gold unread dot), a
"● Connected" daemon pill, "● Anthropic" LLM pill, and a circular avatar with initials.

Page header strip: title "Home" in primary off-white, subtitle in dim secondary "2 deploys
· 5 drafts · 12 runs since you were last here · 18 hours ago". Right side of strip: a
primary amber-gold button "Open setup agent" and a ghost button "New strategy". Below the
header, a small pill row of time-window selectors: "Today", "7d" (active, amber-gold outline),
"30d", "All".

Page body: two-column grid (8/12 + 4/12).

Left column ("operational"):
1. "Needs attention" card. Title in primary, count chip "3" in amber-gold. Three rows visible:
   - Coral severity dot · "paper-eth-mr-v3 dropped −4.1% yesterday" · "Open ↗" link in amber-gold.
   - Amber severity dot · "Critical finding: regime fit mismatch (eth-mr-v3, scenario
     crypto-bear-q3-2024)" · "Open ↗".
   - Sky info dot · "I noticed 3 of your last 5 evals failed in chop. Want me to draft a
     chop-aware variant?" with an "Ask the agent →" link in amber-gold.
2. "Live now" card. Title plus a amber-gold count chip "2". Two deployment rows, each with:
   strategy name in primary, a mode pill ("Paper") in slate, a "● Running" status, P&L
   today in monospace ("+0.62%" amber-gold or "−0.41%" cinnabar), a tiny 80px sparkline, and an
   "Open cockpit ↗" link.
3. "Eval queue" card. One row "Queued: btc-momentum-v2 on chop-q2-25 · ETA ~3min" with a
   thin progress bar at 12% and a "Cancel" ghost button.

Right column ("knowledge & cycle re-entry"):
1. "Recent findings" card with "Open journal →" link in amber-gold at top-right. Three rows
   showing severity dot · finding kind ("Regime fit mismatch", "Overtrading", "Underperf
   in chop") · truncated summary · a strategy chip · a "Draft variant from this →" ghost
   mini-button.
2. "In your workshop" card. Three draft rows: name · template chip ("Catches uptrends",
   "Buys dips", "Trades the range") · status pill ("Eval-ready" amber-gold outline / "Draft"
   slate / "Validating" amber) · "Open in Inspector →" link.
3. Below the workshop card, a single thin line: "You've forked btc-momentum 4 times this
   week — see lineage →" in dim text, with a small "Coming soon" chip in slate.

Mood: ops console for a power user returning to in-flight work. Calm, scannable, every
card answers "what should I look at first?" The amber-gold accents draw the eye to the
attention-grabbing items first.
```

---

### 2. Wizard  —  `/setup` first-run landing + on-demand full page

```
App screen of the xvn setup agent — an AI-led conversation that builds a trading strategy
for the user. Dark mode, desktop 16:9. The chat rail is hidden on this route (the chat IS
the page).

Top nav: standard nav as defined in the design system. "Home" inactive, no nav item
active (this route is /setup, not in the primary nav). Top right shows daemon and LLM
pills.

Page body: two-column split, ~58/42.

Left column (chat, ~58%): scrollable thread of messages. Header strip reads "xvn setup
agent · ● Online" in primary. Three visible message exchanges:
- Assistant message in slate card with amber-gold left border, amber-gold avatar dot: "Hi! I'm the
  xvn setup agent. I'll help you build or pick an AI trading bot. What's your goal today?"
- User reply in tinted slate card on the right: "I want a bot that catches uptrends in
  ETH but exits fast when chop starts."
- Assistant follow-up: "Got it — using the 'Catches uptrends' template with a chop-aware
  exit. Let me set up the slots." Below this message, an inline tool-call card (collapsed)
  reads "Tool: create_strategy(template='trend_follower', name='eth-trend-chopaware')" with
  a small "▸ result" toggle in dim text.

Below the thread, a row of three quick-reply chips in amber-gold outline: "Try a free strategy",
"Build from a template", "Diagnose a recent run".

Composer at the bottom: a single-line textarea with placeholder "type your reply…", a
"+" attachment button (ghost), and a amber-gold arrow-send button labeled "Send" with a small
"⌘↵" keyboard hint.

Right column (visual progress, ~42%): card titled "Strategy in progress" with a status
pill "● Drafting" in amber-gold outline. Below the title:
- Section "Template" with selected chip "Catches uptrends" in amber-gold outline + small
  "Change" link in dim text.
- Section "Agents" — three slim rows: "② Regime — claude-sonnet-4.6 · ● Filled" with amber-gold
  check; "③ Intern — claude-sonnet-4.6 · ● Filled"; "④ Trader — claude-sonnet-4.6 · ⏳
  Drafting" with a small amber-gold spinner and an "Edit prompt" ghost link.
- Section "Mechanics" — chip row: "Cadence: 15m", "Asset: ETH/USD", "Stop: ATR×2".
- Section "Risk" — segmented radio with three options where "Conservative" (default,
  selected with amber-gold background) is highlighted. A tiny "Show details" toggle.
- Section "Last eval" — empty state in dim text: "No eval yet — finish drafting and I'll
  run one."

Sticky footer of the right column: primary amber-gold "Save draft" + ghost "Open in Inspector"
+ ghost (disabled) "Run paper trade".

Mood: AI-led, conversational, with a parallel visual of what's being built so the user
can SEE the strategy taking shape. Posture is "AI is doing the work, user is steering."
```

---

### 3. Inspector  —  `/authoring/<id>` with live-preview split for LLM slots (Move E)

```
App screen of the xvn strategy Inspector — a structured editor for one strategy bundle.
The user is currently on an LLM slot section (the "Trader" slot), so the center column
is rendered as a live-preview split. Dark mode, desktop 16:9. The chat rail is docked on
the right, collapsed to 40px icon strip.

Top nav: standard nav, "Strategies" active.

Page body: three columns plus the chat rail. Left rail 220px (bundle outline), center
flex (split editor), right rail 280px (validation + token estimate + bundle preview),
chat rail 40px collapsed.

Left rail — Bundle outline:
- "Manifest" header with collapsible chevron, two sub-items "Identity" and "Eval
  attestations" indented.
- "Layers" header with seven sub-items: "① Data", "② Regime classifier [LLM]" (with amber-gold
  [LLM] chip), "③ Intern [LLM]", "④ Trader [LLM]" (highlighted with amber-gold left bar — this
  is the active section), "⑤ Entry / Exit rules", "⑥ Risk", "⑦ Execution".
- "Skills" header with one sub-item "Attached skills".
- "Validation" footer line with "● 2 warnings, 0 errors" in amber.
- Tree footer in dim mono: "Bundle hash: 0xa83…f12  [copy]".

Center column — split editor for the "Trader" slot. Header strip: "④ Trader — LLM slot"
in primary. Below the header, a 50/50 horizontal split with a draggable divider in the
middle (thin slate handle).

Left half of split (form):
- "Use this agent" toggle in amber-gold (on, with the toggle disabled because Trader is
  required).
- "Model" dropdown set to "claude-sonnet-4.6".
- "System prompt" label, then a code editor textbox (monospace, 12pt, syntax-tinted)
  showing 6 visible lines of prompt text starting with "You are a trading agent that
  decides whether to open, close, or hold a position…". A "Format" ghost button and
  "Diff vs template" toggle below the editor.
- "Tools the agent can call" with 4 chips: "ohlcv", "indicator_panel", "position",
  "funding_rate".
- "Add a skill" ghost button with a "+" icon.
- "Max tokens / call" numeric input "1500".

Right half of split (live preview):
- Pane header: "Preview decision" with a fixture chip in slate "Fixture: BTC/USD
  2025-01-15 08:00" and a "Change fixture ▾" dropdown.
- Sub-header status: a toggle "● Auto-rerun on (2s debounce)" in amber-gold with a small
  cost hint to the right "~$0.012 per re-run".
- Collapsed "Inputs the agent sees" panel showing only the title and a "▸ expand"
  chevron.
- Output panel: a JSON object rendered with syntax highlight reading
  '{ "action": "long_open", "size_bps": 200, "conviction": 0.72, "reasoning": "RSI
  oversold at 28 with bb_lower touch; regime trending; conviction moderate." }'. Above
  the JSON, a small "● Validated against output_schema" amber-gold badge.
- Diff strip: "Δ vs last preview: action unchanged · conviction 0.65 → 0.72 ↑".
- Cost row: "Last preview: 1,420 tokens · ~$0.012 at your current model" in dim mono.
- Footer ghost button: "Save these inputs as a custom fixture".

Right rail — Inspector summary:
- "Validation" card. Two amber warnings listed: "Trader prompt over 800 tokens — consider
  trimming" with "Jump to fix →"; "Stop loss tighter than scenario default" with "Jump
  to fix →".
- "Estimated tokens / run" card. Three monospace numbers: "input 45,000", "output 8,500",
  "total 53,500".
- "Bundle JSON" card with "Copy hash" and "Download bundle" ghost buttons.
- Sticky bottom: primary amber-gold "Run eval" button + ghost "Save draft" + ghost (disabled,
  "Coming soon" chip) "Publish…".

Right edge — Chat rail collapsed (40px). Just the speech-bubble icon at the top.

Mood: power-user depth tool, but the live-preview pane makes it FEEL like the AI is still
in the loop — every prompt edit re-fires against the fixture and the user SEES the
behavior change, not infers it. The form on the left is the dial; the preview on the
right is the engine note.
```

---

### 4. Lab Notebook  —  `/journal` (Move F)

```
App screen of the xvn Journal — an append-only chronological surface for findings, notes,
and decisions about strategy work. Dark mode, desktop 16:9. The chat rail is docked on
the right, collapsed.

Top nav: standard nav, "Journal" active (amber-gold underline).

Page header strip: title "Journal" in primary, subtitle in dim secondary "147 entries ·
38 findings · 109 notes". Right side: primary amber-gold button "New note", ghost "Filter…",
ghost "Export".

Page body: main content (single chronological column, max 880px wide, centered) plus a
right inspector rail (300px) plus the chat rail (40px collapsed).

Composer at the top of the main column (sticky below header):
- A single textarea with placeholder "What did you learn?" with markdown support hint in
  dim mono.
- A row of 4 attachment chips: "+ Run", "+ Strategy", "+ Finding", "+ Scenario".
- A "Note ▾" template select (dropdown showing options "Note" / "Postmortem" /
  "Hypothesis" / "Decision log").
- Right-aligned primary amber-gold "Add to journal" button with "⌘↵" hint.

Main timeline (chronological, newest at top): 6 visible entries, each as a slate-bordered
card with a kind chip on the top-left:

1. Kind chip "finding" (amber background). Header line: "Critical finding · 14 minutes
   ago · system". Body: a cinnabar severity dot, "regime_fit_mismatch", "Strategy
   underperformed in chop sub-regime within bull window. Sharpe in chop = 0.3 vs baseline
   1.1." Reference chips: "eth-mr-v3" + "run 01H8N7Z". Two ghost buttons: "Draft variant
   from this →" (amber-gold) and "★ Pin".

2. Kind chip "wizard_recap" (amber-gold background). Header: "Drafted eth-mr-v3 from finding ·
   28 minutes ago · system". Body: "Forked from eth-mr-v2 with chop-aware exit rules and
   tightened stop. Tokens used: 2,400." A "Continue conversation ↗" link in amber-gold.

3. Kind chip "note" (slate background). Header: "1 hour ago · you". Body: "Funding rate
   on Mantle has been negative for 3 days straight — worth checking if any of my
   funding-skew strategies are getting suppressed entries." A reference chip
   "stablecoin-flow-v1". Two ghost buttons: "★ Pin" and "Edit".

4. Kind chip "deployment_event" (sky background). Header: "2 hours ago · system".
   Body: "paper-btc-momentum stopped (manual)." Reference chip "btc-momentum".

5. Kind chip "eval_milestone" (amber-gold background). Header: "Yesterday · system". Body:
   "eth-mr-v3 hit 5 evals · best Sharpe 1.42 vs baseline 0.81." Reference chip
   "eth-mr-v3".

6. Kind chip "postmortem" (slate background). Header: "2 days ago · you". Body shows a
   structured 4-row layout: "What I tried: [text]", "What worked: [text]", "What didn't:
   [text]", "Next: [text]". Tag chips: "#chop", "#regime-shift". A "★ Pinned" chip in
   amber-gold at the top-right.

Right inspector rail:
- "Filter" card. A vertical list of toggles: "finding" (on, amber-gold), "note" (on, amber-gold),
  "postmortem" (on, amber-gold), "wizard_recap" (off, slate), "deployment_event" (off, slate),
  "eval_milestone" (off, slate). Below the toggles: a "Severity" multi-select with three
  chips, a "Strategy" select, a "Date range" picker.
- "Pinned" card. Three pinned-entry rows with amber-gold star glyphs.
- "Last 30 days" stats card. Three rows: "Findings" with a tiny sparkline, "Notes added
  · 23", "Drafts forked from journal entries · 7".
- "Tags" card. A wrap of clickable chips: "#chop", "#funding", "#regime-shift",
  "#stablecoin-flow", "#orderly-onboarding".

Mood: a researcher's lab notebook. Calm, chronological, the place where lessons
accumulate across strategies. Severity dots and kind chips give the eye an instant scan.
```

---

### 5. Flight Deck  —  `/live/<id>` live cockpit with Move I integration

```
App screen of the xvn live cockpit — a single deployment running paper trades against
Alpaca, mid-session, with the chat rail expanded on the right (the user just asked the
agent something about a recent veto). Dark mode, desktop 16:9.

Top nav: standard nav, "Live" active.

Page header strip: title "eth-mr-v3 (Catches uptrends, chop-aware)" in primary. Subtitle
in dim secondary "Paper · Alpaca paper · started 09:12 · uptime 4h 27m". Right side of
strip: a "● Running" amber-gold status pill, then ghost "Pause", ghost "Resume" (disabled),
danger ghost "Stop" in cinnabar text, ghost "Open in Inspector", and a primary amber-gold "Draft
variant from this deployment →" button (this is the Move I trigger).

Page body: main content (flex) plus the chat rail expanded on the right (360px).

Main content:
1. KPI gauge row — five tiles spanning full width, each ~220px wide:
   - "P&L today" tile: monospace "+1.42%" big in amber-gold.
   - "Equity" tile: "$10,142.30" in primary.
   - "Open positions" tile: "1" big with "ETH long 0.05" in dim mono below.
   - "Decisions / hr" tile: "12" with a tiny sparkline beneath.
   - "Tokens / hr" tile: "1,840" with a thin progress arc.

2. Equity ticker chart — wide chart area ~280px tall. A amber-gold line ascending with one
   small dip mid-chart and a recent ▲ peak marker. Time-range pills above the chart:
   "1h", "4h" (active), "24h", "7d", "30d", "All". Crosshair tooltip showing
   "13:42 · $10,142.30 · +0.18%".

3. Two-column row below the chart:
   - Left card (60%): "Open positions" table. One row: "ETH/USD · long · 0.05 · entry
     2845.20 · mark 2851.50 · +0.22% · stop 2780 · TP 2920 · holding 23m". Row action:
     ghost "Close" (danger).
   - Right card (40%): "Live decision feed" — reverse-chronological. Filter chips at top:
     "All" (active), "Trades only", "Vetoes only". Five rows visible, each with timestamp,
     slot name in dim, action chip ("long_open" amber-gold, "flat" slate, "short_close" cinnabar),
     conviction "0.72", risk verdict pill ("● passed" amber-gold or "● vetoed" cinnabar), and a
     "View prompt ↗" link.

4. Risk panel below: a small donut chart on the left ("84% passed · 12% modified · 4%
   vetoed" with three colored segments), and on the right a "Veto reasons" mini-table:
   - "max_position_size · 3 vetoes" with a primary amber-gold "Draft variant from this →" ghost
     button (Move I, only on rules with ≥3 occurrences).
   - "daily_loss_circuit · 1 veto" (no draft button — under threshold).

Right rail — Chat rail EXPANDED (360px wide, slate panel surface):
- Header strip: "xvn agent · ● Online" with a context chip below in slate
  "Context: Deployment eth-mr-v3 (paper)".
- Three message exchange in the thread:
  - User: "why did the max_position_size veto fire 3 times today?"
  - Assistant in amber-gold-bordered card: "All 3 vetoes hit when the strategy tried to open
    second concurrent positions while the first was still open. Your risk preset is
    Conservative which caps concurrent at 1. Want me to draft a variant with
    Conservative + concurrent=2?"
  - Below the assistant message: two action chips in a row, "Draft variant" (amber-gold) and
    "Show me the trades" (ghost).
- Composer at the bottom with placeholder "Ask about this run…" and a small "Tokens
  this session: 4,820" footer + an "Open in /setup ↗" link.

Mood: cockpit, in-flight, gauges alive — but the chat rail makes the AI feel present,
not absent. The user is steering AND has a co-pilot.
```

---

### 6. Run Detail  —  `/eval/runs/<run_id>` with Move I "Draft variant" buttons

```
App screen of an xvn evaluation run detail page — a completed backtest run showing all
its metrics, trades, and findings. Dark mode, desktop 16:9. The chat rail is docked on
the right, collapsed (40px).

Top nav: standard nav, "Eval" active.

Page header strip: title "Run 01H8N7Z · eth-mr-v3 backtest" in primary. Subtitle in dim
secondary "Strategy: eth-mr-v3 · Scenario: Crypto bull Q1 2025 · Backtest". Right side: a
"● Completed" amber-gold status pill, then ghost "Re-run", ghost "Extract findings", ghost
"Download tape", ghost "Compare with…", and a primary amber-gold "Draft variant from this run
→" button (Move I).

Page body: main content (flex) plus chat rail (40px collapsed).

1. KPI tile row — 4 tiles spanning full width:
   - "Total return" tile: "+18.4%" big in amber-gold with a small "vs +12.1% buy & hold" in dim
     below.
   - "Sharpe" tile: "1.62" with a small ▲ glyph and "+0.41 vs baseline" beneath.
   - "Max drawdown" tile: "−7.1%" in cinnabar with "vs −9.2% baseline" beneath.
   - "Win rate" tile: "58%" with a tiny donut showing 27/47 wins.

2. Equity curve panel — wide chart ~320px tall. Two lines: a thicker amber-gold line for the
   strategy and a thinner slate line labeled "Buy & hold". Toggle above the chart: "Show
   drawdown shading" (off, slate). A faint shaded region beneath the equity line shows
   drawdown periods.

3. Trade markers panel — a horizontal price chart of ETH/USD over the same time range
   with markers at every decision point: ▲ buy in amber-gold, ▼ sell in cinnabar, ○ flat in slate,
   ✕ close in white. Hover state shown on one marker with a tooltip "2025-01-15 08:00 ·
   long_open · 0.05 · fill 2845.20 · realized PnL +1.4% · ▸ decision JSON".

4. Trade ledger table — sortable, 6 visible rows. Columns: "Time" (mono), "Asset",
   "Side" (chip amber-gold/cinnabar), "Qty" (mono), "Entry" (mono), "Exit" (mono), "PnL" (mono,
   colored), "Holding period", "Fees", "Decision" (link "View ↗").

5. Findings panel — header "Findings · 4" with a "Re-extract" ghost button. Four rows:
   - Sky info dot · "regime_drift · Mid-bull regime shifted to chop on 2025-02-18; strategy
     adapted within 6 bars" · "Evidence ↗" link · primary amber-gold "Draft variant from this →"
     ghost button (Move I) · ghost "Add to journal".
   - Amber warning dot · "drawdown_concentration · 60% of total drawdown occurred in 3
     consecutive trades 2025-03-01..03" · "Evidence ↗" · "Draft variant from this →"
     (amber-gold) · "Add to journal".
   - Coral critical dot · "tail_risk · single trade lost 4.2%, 2x next-largest loss" ·
     "Evidence ↗" · "Draft variant from this →" (amber-gold) · "★ Auto-pinned to journal" chip.
   - Sky info dot · "underutilization · agent went flat for 12 consecutive bars 2025-02-
     27..28" · "Evidence ↗" · "Draft variant from this →" · "Add to journal".

   Bottom-right corner of the panel: severity legend "● info  ● warning  ● critical".

Right edge — Chat rail collapsed (40px) with speech-bubble icon and a small amber-gold unread
dot.

Mood: post-flight debrief, dense data with clear hierarchy — KPIs first, equity story
second, trades third, findings fourth. The "Draft variant from this →" buttons on every
finding are the cycle-closure affordance: every diagnostic line is one click away from a
new hypothesis.
```

---

### 7. Compare View  —  `/eval/compare?ids=…`

```
App screen of an xvn comparison view — three evaluation runs of the same strategy across
different scenarios, overlaid for direct comparison. Dark mode, desktop 16:9. Chat rail
collapsed.

Top nav: standard, "Eval" active.

Page header strip: title "Compare runs" in primary. Subtitle in dim mono showing three
ULIDs as clickable tokens "01H8N7Z, 01J2P9R, 01K9R5T". Right side: ghost "Add another
run…", ghost "Save comparison", ghost "Export PNG", primary amber-gold "Draft variant from
this comparison →" (Move I).

Page body:
1. Equity overlay panel — large chart ~360px tall with three lines in distinct accent
   colors: amber-gold (run A, "eth-mr-v3 on bull-q1-25"), sky (run B, "eth-mr-v3 on chop-q2-
   25"), amber (run C, "eth-mr-v3 on bear-q3-24"). Shared time axis (relative time, 0 to
   90 days). Synced crosshair shown on one of the lines. Above the chart, three pill
   toggles per line with color swatches and on/off visibility toggles. Chart-mode
   selector top-right: "Equity" (active, amber-gold outline) / "Cumulative return" / "Drawdown".

2. Trade markers panel — a strategy radio row at the top with three options "eth-mr-v3
   on bull-q1-25" (selected, amber-gold), "...on chop-q2-25", "...on bear-q3-24". Below, a
   single-asset chart with that strategy's markers visible.

3. Metrics comparison table — 3 columns (one per run) plus a metric column. Best value
   per row gets a amber-gold cell border. Rows: "Total return" (+18.4%, +3.1%, −2.4%), "Sharpe"
   (1.62, 0.41, −0.18), "Max drawdown" (−7.1%, −12.4%, −18.9%), "Win rate" (58%, 51%,
   42%), "n trades" (47, 39, 32), "Tokens used" (51,200, 48,900, 45,400). Sortable column
   headers with caret glyphs.

4. Findings panel — flat list across all runs in the set, grouped by kind. Filter chips
   at top: "info", "warning", "critical". Five visible rows, each with: severity dot,
   finding kind, summary (truncated), source-run chip in slate, "Draft variant from this
   →" amber-gold ghost button.

Mood: side-by-side diagnostic. The eye can scan three runs simultaneously and ask "what
do the winners share that the losers don't?" The Move I buttons turn that diagnostic
into a generative hypothesis.
```

---

### 8. Run List + Scenarios  —  `/eval/runs` and `/eval/scenarios` (paired surfaces)

```
TWO screens, render side-by-side as a single 16:9 image with a thin slate divider down
the middle. Both are dark mode. Chat rail not shown (this is a documentation
screenshot of two list pages).

LEFT HALF — Run list:

Top of left half: page title "Eval runs" in primary; small "Mine" / "All" / "Published
evals" tabs below.

Header strip actions: primary amber-gold "New run" + ghost "Compare selected (2)" (enabled,
since 2 rows are checked).

Filter bar: a multi-select chip row "eth-mr-v3 ✕  btc-momentum-v1 ✕" + "Add strategy +",
then "All scenarios ▾", then "All / Backtest / Paper" segmented control with "All"
selected, then a "Date range" picker, then a "Sort: Most recent ▾" dropdown.

Run table: 8 rows, sticky header. Columns: ☐ checkbox · Run ID (mono ULID short, 6
chars) · Strategy · Scenario · Mode pill · Status (with dot) · Sharpe (mono) · Total
return (mono, colored) · Max DD (mono cinnabar) · Win rate · n trades · Tokens (mono) ·
Started (relative) · ⋯. Two rows are checked (enabling the Compare button). Status mix:
"● Completed" (amber-gold, 5 rows), "● Running 42%" with inline progress bar (1 row), "●
Queued" (1 row), "● Failed" cinnabar (1 row).

RIGHT HALF — Scenario manager:

Top of right half: page title "Scenarios" in primary; tabs below "Canonical" (active) /
"Custom".

Header strip actions: primary amber-gold "New scenario" + ghost "Import JSON".

Card grid: 6 scenario cards in a 3×2 grid, each ~300px wide:
- "Crypto bull Q1 2025" — subtitle "2025-01-01 to 2025-04-01 · 1 asset · trending_bull,
  low_vol" with three regime chips. Body: "Strong uptrend, low volatility regime."
  Footer: "Used by 12 runs · Best Sharpe: 1.62" + ghost row actions "Run with…" "Edit"
  "Duplicate".
- "Crypto bear Q3 2024" — similar layout.
- "Crypto chop Q2 2025" — similar.
- "Flash crash 2024-08" — similar with a cinnabar "high_vol" chip.
- "BTC sideways Q4 2023" — similar.
- "Multi-asset bull Q2 2024" — similar with a "BTC, ETH, SOL" asset list.

Mood: two scannable directories side by side. The run list is dense and tabular for
power-user work; the scenario gallery is browsable and visual for set-up.
```

---

### 9. Chat Rail (fragment, expanded)  —  Move B persistent agent presence

```
UI fragment, not a full screen — render only a vertical right-side rail, 360px wide and
~900px tall (the height of a typical desktop window minus top-nav). Dark mode, slate
panel surface (#1A2029) with a 1px slate left border and a subtle drop-shadow on the
left edge to indicate it floats above page content (the page can be a faded blurred
surface to the left).

Header strip at the top of the rail (~100px tall):
- Title "xvn agent · ● Online" in primary with a small amber-gold dot.
- Below the title, a context chip in slate-darker pill: "Context: Run 01H8N7Z (eth-mr-
  v3, bull-q1-25)" with a small dropdown chevron at the right.
- Below the chip, a row of three small ghost icon buttons: "↻ start fresh", "🔍 search
  history", "≡ collapse rail" (which would shrink to 40px icon strip).

Chat scroll region (middle, ~600px tall):
- 5 message exchanges, alternating assistant on left (amber-gold left border, slate card,
  small amber-gold avatar dot) and user on right (tinted slate card aligned right).
- Most recent assistant message is mid-stream — has a small amber-gold pulsing cursor at the
  end of its last word, indicating typing.
- Inside the third assistant message, an inline tool-call card (collapsible, currently
  collapsed) reading "Tool: list_findings(run_id='01H8N7Z') · ▸ result (4 findings)".
- Inside the fourth assistant message, two inline action chips below the text: "Draft
  variant" (amber-gold) and "Show me the worst trade" (ghost).

Quick replies row (above composer, ~50px tall):
- Three chips in a row: "Why did it underperform?", "Compare to its baseline", "Suggest
  a variant to draft" — all in amber-gold outline.

Composer at the bottom (~150px tall):
- A multi-line textarea with placeholder "Ask about this run…", ~80px tall.
- Below the textarea, a left-aligned "+ context" ghost button (with an icon menu hint:
  attach a chart, attach a finding, etc.) and a right-aligned primary amber-gold "Send" button
  with "⌘↵" hint.
- Footer row in dim mono: "Tokens this session: 4,820" on the left and "Open in /setup
  ↗" link on the right.

Mood: ambient AI presence — feels like a co-pilot that's always there but never
demanding. The amber-gold accents (avatar dot, send button, action chips, pulsing cursor) draw
the eye to where the AI is actually doing something, while the slate body recedes.

Aspect: tall and narrow. No top nav, no other panels — only the rail.
```

---

### 10. Command Palette (modal overlay)  —  ⌘K from anywhere

```
UI fragment — render a centered modal overlay on a dimmed (~50% darkened) page
background. The modal is 640px wide and ~520px tall, dark mode, elevated surface
(#11161D) with a 1px slate border and a soft drop-shadow.

Top of the modal: a wide search input (~50px tall) with a left-anchored "⌘K" pill in dim
mono and a placeholder "Jump to a strategy, run, finding, or scenario…" in dim text. The
input has focus (visible cursor at the start, faint amber-gold outline ring).

Below the search input, a thin slate divider, then a scrollable result list grouped by
category. Each group has a small uppercase dim-text header followed by 1-3 result rows.
Each result row is ~40px tall and has: a category-appropriate icon on the left (Lucide
thin-stroke), the result name in primary, a secondary metadata line in dim ("Strategy ·
Catches uptrends · Eval-ready"), and a right-aligned "↵" key hint glyph in dim.

Visible groups (in this order):

STRATEGIES (3 rows):
- "eth-mr-v3" with subtitle "Mean reversion · ETH · Eval-ready"
- "btc-momentum-v1" with subtitle "Momentum · BTC · Live (paper)"
- "stablecoin-flow-v1" with subtitle "On-chain · Multi-asset · Draft"

RUNS (2 rows):
- "01H8N7Z · eth-mr-v3 · bull-q1-25" with subtitle "Backtest · Sharpe 1.62 · Completed"
- "01J2P9R · eth-mr-v3 · chop-q2-25" with subtitle "Backtest · Sharpe 0.41 · Completed"

FINDINGS (2 rows):
- "regime_fit_mismatch in eth-mr-v3" with subtitle "Critical · 14 minutes ago"
- "drawdown_concentration in eth-mr-v3" with subtitle "Warning · 1 hour ago"

ACTIONS (2 rows):
- "New strategy from template…" with a "↵" hint
- "Open setup agent" with a "↵" hint

Footer of the modal (~30px tall): a thin row in dim mono showing keyboard hints "↑↓ to
navigate · ↵ to open · esc to close" on the left, and "0.4ms" search-time chip on the
right (a power-user touch).

Mood: developer-tool power surface — Spotlight, Linear's command bar, GitHub's `?`
shortcut. Fast, dense, every keystroke counts.

Aspect: square-ish (640×520), centered on a dimmed page. Page beneath should be just
faint enough to confirm there's a real surface there, but not legible.
```

---

### 11. Live Evening Cycle Viewer  —  `/autoresearch/live` (the autoresearch demo headline)

```
App screen of the xvn live evening cycle viewer — autoresearch is currently mid-run.
Strategies are mutating, paper-tests running, the gate sorting wheat from chaff in real
time. Dark mode, desktop 16:9. The chat rail is docked on the right, collapsed (40px).

Top nav: standard nav with "Eval" active. Top right shows the daemon status pill, the LLM
provider pill, and the avatar. ALSO a small new pill near the daemon: "● Cycle running ·
Lineage 3/5" in amber-gold outline.

Page header strip: title "Evening cycle · 2026-06-12" in primary, subtitle in dim secondary
"Started 22:00 · 47 minutes elapsed · ε=0.07 (pre-committed)". Right side: a token/cost
meter "Tokens: 84,200 of 150,000 budget" with a thin horizontal progress bar at 56%, then
ghost "Pause cycle", primary amber-gold "Open genealogy →".

Page body: two-column split, ~38/62.

Left column (lineage selector, ~38%): vertical list of 5 lineage rows, each ~70px tall.
Each row is a slate-bordered card with a small lineage color dot, a lineage name in primary
("Lineage A · btc-momentum line"), a mini sparkline of recent Sharpe (amber-gold), and a
status chip ("● Mutating" amber-gold pulsing / "● Paper-testing" sky / "● Idle" dim / "●
Sealed" amber-gold solid). The currently-active lineage (Lineage C) is highlighted with an
amber-gold left border. A small "+ ghost branch" footer line in dim mono showing rejected
mutations count: "4 ghost branches tonight".

Right column (mutation timeline, ~62%): a vertical scrolling stream of cycle events for
the active lineage. Each event is a thin slate-bordered card with a left gutter timestamp
in mono. Five visible events:

1. Top of stream (most recent), pulsing: "22:46:12 · ◆ proposing mutation… " with a thin
   pulsing amber-gold typing indicator. Below: a collapsed inline tool-call card "Mutator:
   claude-haiku · ▸ token estimate: 1,400".

2. "22:45:38 · candidate generated · diff: +RSI(14)→RSI(21), prose tweak (8 lines), +bbands
   tool". A small expand chevron "▸ view diff" in dim text.

3. "22:44:51 · paper-test (day window)" with a horizontal progress bar at 100% in amber-gold,
   result chip "● Δ-Sharpe +0.18 · PASS" in amber-gold solid.

4. "22:44:19 · paper-test (held-out)" with progress bar 100% in slate, result chip
   "● Δ-Sharpe −0.04 · FAIL" in cinnabar.

5. "22:44:12 · ◇ REJECTED · ghost branch logged · reason: gate-rejected (held-out failed)".
   Faded to ~60% opacity. A small "↗ inspect ghost" link in dim text.

Beneath the stream, a single row of fixed-height stat tiles (~80px tall, full width, 4
tiles): "Mutations proposed: 12", "Committed: 3", "Quarantined: 1", "Ghost branches: 8".

Right edge — Chat rail collapsed (40px) with speech-bubble icon and a small amber-gold dot.

Mood: live operations theater. Designed to read on a projector at five meters — large
type for status chips and timestamps, narrow for event detail. The pulsing typing
indicator on the top event is the only motion; everything else is calm. Judges should
glance at this once and instantly understand "the loop is alive and selective".
```

---

### 12. Genealogy Tree  —  `/autoresearch/lineage`

```
App screen of the xvn genealogy tree — every strategy variant ever born, with parents,
children, and ghost branches all visible. Dark mode, desktop 16:9. Chat rail docked on
the right, collapsed.

Top nav: standard nav with "Strategies" active.

Page header strip: title "Genealogy" in primary, subtitle in dim secondary "23 active
variants · 4 lineages · 14 ghost branches · current best Sharpe 1.87". Right side: filter
chips "All lineages ▾", "Active only", "Show ghosts" toggle (off, slate); ghost "Search
variant…", primary amber-gold "Open ladder →".

Page body: main content (flex) plus the chat rail (40px collapsed).

Main content: a force-directed node graph filling ~70% of the page. The canvas background
has a subtle dotted grid (#1A2029 dots on #0E1014). Four lineages are visible,
distinguished by node fill color (amber-gold, sky, amber-warm, slate-purple). Each node is
a small filled circle (16–30px diameter, sized by trade count); the largest node has a
faint amber-gold halo indicating it is the current ladder leader.

Edges between parent and child nodes are thin lines, color-matched to the parent's lineage.
Edge stroke style encodes the mutation type:
- Solid line: prose mutation
- Dashed line: param mutation
- Dotted line: tool mutation

Ghost branches (rejected mutations) are dimmer nodes (~30% opacity), connected to their
parent with a faded edge and a small "✕" glyph. They cluster at the periphery of their
lineage.

A drawer is open on the right side of the canvas (~360px wide), pinned to the canvas not
the page edge, slate panel surface, showing details for a selected node:
- Header: "btc-momentum-v3.2 · Lineage A" in primary, with a "● Active" amber-gold status
  chip below.
- Lineage trail breadcrumb in dim mono: "btc-momentum (seed) → v2 → v2.1 → v3 → v3.1 →
  v3.2".
- Three-row metric strip: "Sharpe 1.87 (▲ +0.23 vs parent)", "Trades: 142", "Born:
  2026-06-09".
- Section "program.md (truncated)" with 6 visible markdown lines and an "Expand" link.
- Section "Mutation from parent": three small chips — "+RSI(21)→RSI(14)", "prose: regime
  detection logic", "+atr tool".
- Section "Finding" with a slate card: "Favors trending regimes; reduces position size after
  consecutive losses." Confidence pill "● Med" in amber-gold.
- Footer: ghost "Open in Inspector →" + ghost "Compare with parent" + ghost (Persona B
  only, conditional) "View NFT on Mantle ↗".

Floating top-right of the canvas: zoom controls (− 1.0 +) and a layout toggle ("Force
directed" active / "Radial" / "Timeline").

Mood: a genealogy tree should feel alive. The tree teaches "lineages explore differently"
at a glance — color groups cluster, ghost branches show the gate's pickiness, the haloed
leader draws the eye. Calm but information-dense.
```

---

### 13. Mutation Diff Inspector  —  `/strategies/<bundle_hash>/diff`

```
App screen of the xvn mutation diff inspector — comparing a child variant against its
parent. Dark mode, desktop 16:9. Chat rail docked on the right, collapsed.

Top nav: standard nav with "Strategies" active.

Page header strip: title "btc-momentum-v3.2 (child) vs btc-momentum-v3.1 (parent)" in
primary. Subtitle in dim mono "blake3:a83…f12 → blake3:b91…d04 · cycle 01H8N7Z · merged
2026-06-09 22:14". Right side: ghost "Run again", ghost "Open ledger", primary amber-gold
"Draft variant from this →" (Move I).

Page body: three-pane horizontal split, plus a finding panel below.

Three panes side by side:

1. Left pane "Prose diff" (~33%): a markdown red/green diff view of the program.md changes.
   Header strip "program.md · ±18 lines". Visible diff: 4 red lines (removed) showing prior
   regime-detection logic, 6 green lines (added) showing new logic that includes ATR-based
   chop detection. Line numbers in dim mono on the left gutter. A small "fold unchanged
   regions" toggle at the top.

2. Center pane "Param diff" (~33%): a typed parameter table. Header "params · ±3 changes".
   Three rows: "rsi.period · 21 → 14", "atr.window · 14 (added)", "stop.atr_mult · 2.0 →
   1.7". Each row has a left-aligned field path in mono, an old-value chip in slate (with
   strikethrough), an arrow "→", a new-value chip in amber-gold. The "(added)" row shows a
   "+" glyph instead of an old value.

3. Right pane "Tool diff" (~33%): a chip-based diff. Header "tools · +1 / −0". Two sections
   stacked: "Removed tools" (empty state in dim "no tools removed"), "Added tools" with one
   chip "atr" in amber-gold outline with a small "+" glyph. Below the chips, a one-line
   dim-mono explanation: "atr exposes ATR(period) for chop detection".

Below the three panes — Finding panel (full width):

Section header "Judge finding (Sonnet · metrics-blind)" with a small "● Verified
metrics-blind" amber-gold chip on the right. Finding card:
- Summary in primary: "Variant favors trending regimes with explicit ATR-based chop
  detection. Reduces position size after two consecutive losses; tighter stop discourages
  give-back during whipsaws."
- Three chip rows:
  - Regime affinity: "Trending" (amber-gold), "HighVol" (amber-gold).
  - Failure modes: "Range-bound markets" (slate), "Sharp gap-down moves" (slate).
  - Confidence: "● Med" (amber-gold).
- Footer: "Token cost: 4,200 · Judge model: claude-sonnet-4.6".

Below the finding panel — Inversion-pair check panel:
- Header "Inversion-pair eval · ● Real signal" in amber-gold.
- One line of evidence: "Forward Δ-Sharpe = +0.18 · Reverse Δ-Sharpe = −0.12 · Bootstrap
  CI rules out noise (p=0.018)".

Mood: forensic. Three orthogonal views of the same change, then the LLM's interpretation,
then the falsification check. The user can scan three panes and form their own hypothesis
BEFORE reading the judge's; the inversion-pair line confirms or refutes the gate's call.
Power-user diagnostic surface.
```

---

### 14. Mutator-Skill Ladder  —  `/eval/mutator-ladder`

```
App screen of the xvn mutator-skill ladder — the second ladder, where the LLM mutator
itself is being measured. Dark mode, desktop 16:9. Chat rail docked on the right,
collapsed.

Top nav: standard nav with "Eval" active.

Page header strip: title "Mutator skill · Strategy ladder" in primary. Subtitle in dim
secondary "Two ladders, side by side · ε=0.07 · session 2026-05-15 → 2026-06-15". Right
side: time-window pills "24h", "7d" (active, amber-gold outline), "30d", "All session".

Page body: two-column split with a vertical slate divider, ~50/50.

LEFT HALF — Strategy ladder (the existing one, summarized):

Section header "Strategy ladder · top by Sharpe" with a "Open full ladder →" link.

A clean tabular ranking of 7 visible strategy variants. Columns: rank (mono "1"–"7"),
variant name (with lineage color dot prefix), Sharpe (mono, colored), max DD (mono
cinnabar), n trades (mono), lineage depth (mono "L3", "L1", etc.). Rank #1 has a small
amber-gold ▲ "leader" glyph. Status row shows "23 active variants in 4 lineages · live
updates".

RIGHT HALF — Mutator-skill ladder (NEW):

Section header "Mutator ladder · skill metrics" with a small "What is this?" tooltip icon
in dim text.

A KPI tile row of 4 tiles spanning the half-width:
- "Acceptance rate" tile: monospace "27%" big. Sub-line in dim: "104 proposed · 28
  accepted".
- "Calibration" tile: "0.81" big with a small ▲ glyph. Sub-line: "claimed Δ-Sharpe vs
  realized · ideal=1.00".
- "Regime bias" tile: a horizontal mini bar chart with 3 bars labeled "Trend +0.41", "Chop
  +0.12", "Range −0.09". The Trend bar in amber-gold extends rightmost.
- "Token efficiency" tile: monospace "0.0064" big. Sub-line: "Sharpe gain per 1k mutator
  tokens".

Below the tiles, a tabular breakdown by parent type. Header "By parent type · last 7d".
Five columns: Parent type (e.g., "TA · classical", "Onchain · flow", "LLM-driven ·
trader", "Hybrid"), Acceptance % (mono, with mini horizontal bar), Mean realized gain
(mono, colored), Calibration error (mono), n proposals (mono). Four visible rows.

Below the table, a small footer line in dim mono: "Mutator model: claude-haiku-4-5 · Total
mutator tokens this session: 384,200".

Right edge — Chat rail collapsed (40px) with the speech-bubble icon.

Mood: recursive measurement. The thing optimizing is itself being optimized over. The
side-by-side framing makes the conceptual payoff immediate — *here are the strategies, here
is the strategy that produced them, both ranked.*
```

---

### 15. Ladder with Provenance  —  `/eval/ladder` augmented

```
App screen of the xvn strategy ladder, augmented with lineage provenance for every entry.
Dark mode, desktop 16:9. Chat rail docked on the right, collapsed.

Top nav: standard nav with "Eval" active.

Page header strip: title "Strategy ladder · with provenance" in primary. Subtitle in dim
secondary "23 active variants · 4 lineages · ranked by Sharpe (Δ-Sharpe held-out window)".
Right side: tabs "Live" (active, amber-gold underline), "Backtest", "Combined"; ghost
"Open genealogy →"; primary amber-gold "Compare top 3 →".

Page body: a single wide table dominating the page (max-width 1280px, centered). Sticky
header. 8 visible rows.

Columns:
- Rank (mono "1"–"8"; rank #1 has an amber-gold ▲ leader glyph)
- Variant name with a lineage color dot prefix (amber-gold dot for Lineage A, sky for B,
  amber for C, slate-purple for D)
- Sharpe (mono, amber-gold)
- Max DD (mono, cinnabar)
- Win rate (mono, with tiny donut)
- n trades (mono)
- Lineage depth: a chip showing "L0" / "L1" / "L2" / "L3" / "L4" — shaded gradient (deeper
  generation = darker amber-gold tint)
- Parent: a clickable hash chip in mono "blake3:a83…f12" (only visible on rows where
  parent_hash is non-null; seed strategies show "—")
- Born from: a small line of dim-mono text — "+atr tool, prose tweak", "param: rsi.period
  21→14", "(seed)", "+regime detection", etc. Truncated with hover-to-expand.
- Actions: ghost "View →" + small ghost icon for genealogy "↗".

Two specific rows of interest:
- Rank #1 row, "btc-momentum-v3.2", Lineage A. Lineage depth chip "L3" (amber-gold tinted).
  Parent "blake3:b91…d04". Born from "+atr tool, prose tweak". Sharpe "1.87" with a small
  ▲ "+0.23 vs parent" in dim text below.
- Rank #4 row, "ghost: btc-momentum-v3.5", with a small "● Quarantined: noise-suspect"
  amber chip after the variant name. The whole row is at ~70% opacity. Action: only
  "Inspect →" ghost; no genealogy link (quarantined variants are excluded from the tree
  view by default).

Below the table — small footer card "Lineage summary · last 7d":
- Four lineage tiles in a row, each with: lineage color dot, lineage name, "best Sharpe"
  metric in mono, member count, depth range "L0–L3".
- Tiny chip on each tile: lineage's status — "● Healthy" amber-gold / "● Mode-collapse
  risk" amber / "● Stalled" slate.

Right edge — Chat rail collapsed (40px) with the speech-bubble icon and a small amber-gold
dot indicating an unread comment from the agent ("Lineage B's diversity-decay rate dropped
below 0.4 — want to inject novelty pressure?").

Mood: ranked leaderboard with deep provenance. Every entry is one click from "where did
it come from?" and one click from "what changed since the parent?" The lineage depth chips
and "born from" column turn a scoreboard into a genealogy at a glance.
```

---

### 16. Marketplace Tab  —  `/marketplace` (Persona B; visible only with `--features marketplace`)

```
App screen of the xvn marketplace tab — the Persona B surface where strategy lineages are
exposed on Mantle via ERC-8004. Dark mode, desktop 16:9. Chat rail docked on the right,
collapsed.

Top nav: standard nav. NOTE: this nav has an additional item "Marketplace" appearing AFTER
"Journal" — visible only because --features marketplace is enabled. "Marketplace" is the
active tab (amber-gold underline). Top right shows daemon pill, LLM pill, AND a new wallet
pill: "● 0xa83…f12 · 0.42 ETH" in amber-gold outline.

Page header strip: title "Marketplace · Mantle mainnet" in primary. Subtitle in dim
secondary "4 lineages on chain · 9 NFTs minted · 18 attestations posted · session
01H8…RTZ". Right side: ghost "Switch to Sepolia", ghost "View on Mantlescan ↗", primary
amber-gold "Anchor all final" with a small lock icon (action gated behind a confirmation
modal).

Page body: four panels in a 2×2 grid.

Top-left panel "Lineages on chain" (~50% wide):
- Tabular list of 4 rows. Columns: lineage color dot, lineage name, NFT token ID (mono
  "#0042"), parent lineage chip, born (relative time), best Sharpe (mono amber-gold), anchor
  status pill ("● Anchored 6h ago" amber-gold / "● Pending anchor" amber / "● Not yet
  minted" dim).
- Two visible rows show "Anchored", one shows "Pending", one shows "Not yet minted".
- Each row has a right-aligned ghost icon "↗" linking to Mantlescan.
- Footer in dim mono: "Last mint tx: 0x4f8a…dc11 · 14 minutes ago".

Top-right panel "Attestations · in-house" (~50% wide):
- Header sub-title in dim mono "2 attesters operated by xianvec · v1 · external
  participation in v2".
- Two attester cards stacked, each ~70px tall:
  - Card 1: "regime-verifier · NFT #0007" with a "● Active" amber-gold dot. Sub-line "27
    endorsements · 4 questions · 0 rejects · last action 2 minutes ago". A right-aligned
    ghost "View on chain ↗".
  - Card 2: "diversity-check · NFT #0008" with "● Active". Sub-line "21 endorsements · 6
    questions · 0 rejects".
- Below the attester cards, a "Recent verdict feed" sub-section. Three rows:
  - "regime-verifier · ENDORSE · btc-momentum-v3.2 · 2 minutes ago · rationale: 'Trending
    claim verified against trace'."
  - "diversity-check · QUESTION · btc-momentum-v3.2 · 2 minutes ago · rationale: 'Embedding
    distance to v3.1 < 0.18 threshold'."
  - "regime-verifier · ENDORSE · eth-mr-v4.1 · 18 minutes ago".
  - A small "● Disagreement" cinnabar chip next to the row pair where regime endorses but
    diversity questions — visually grouped.

Bottom-left panel "Anchor history" (~50% wide):
- A vertical timeline of anchor events, newest at top. Each row is ~40px tall: a timestamp
  mono, an event icon ("◆ Mint" / "◇ Merkle" / "✦ Commit"), a target chip (lineage name or
  "SessionCommitment"), a tx hash chip (mono, clickable), and a gas cost in mono.
- Six visible rows, mixing mints and Merkle anchors.
- Footer line: "Total gas this session: 0.014 ETH · ~$42".

Bottom-right panel "Operator actions" (~50% wide):
- Four primary action cards stacked, each ~60px tall, each with a left icon, a title, a
  one-line description, and a right-aligned button:
  - "Mint missing NFTs" · "1 lineage not yet minted (Lineage D)" · primary amber-gold "Mint
    now" button.
  - "Anchor a lineage" · "Post counterfactual-chain Merkle root for a single lineage" ·
    ghost "Choose…" button.
  - "Anchor all final" · "Hackathon-end action: post LineageEnd receipt for every active
    lineage" · primary amber-gold "Anchor all" button (gated, requires confirmation).
  - "Run attesters" · "Force both attesters to score recent unscored bundles" · ghost "Run
    now" button.
- Each card has a small dim-mono cost estimate beneath the description: "~est. 0.001 ETH ·
  ~$3".

Right edge — Chat rail collapsed (40px) with the speech-bubble icon.

Mood: operator command bridge for the on-chain layer. Persona A never sees this tab;
Persona B (judges, marketplace participants) lands here to verify provenance and run
operator actions. Calm, dense, with explicit gas costs everywhere — this is a
wallet-aware surface and the user should always know what's about to spend gas.
```

---

## Deferred archetypes  (preserved for post-v1 reference)

These five archetypes are not in the v1 wireframe scope per `ui-elements.md` v0.2 §16
but their prompts are kept here verbatim for the day they get picked up. Spec status
noted on each.

### 17. Canvas  —  spatial node graph (DEFERRED — post-hackathon)

```
App screen of a node-graph canvas for composing an xvn trading strategy visually. Dark mode,
desktop 16:9. Full-bleed canvas with a left tool drawer and a right inspector panel.

Canvas content: five rounded rectangle nodes arranged left-to-right with connecting wires:
"Data · ohlcv" → "② Regime classifier" → "③ Signal interpreter" → "④ Decision arbiter" →
"Broker · alpaca-paper". Each node has a colored top-stripe (amber-gold for LLM slots, slate for
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

Floating top-right: zoom controls (− 100% +) and a "Run preview eval" amber-gold button.

Mood: spatial, expressive, designer-oriented power tool.
```

### 18. Power Notebook  —  cell-based REPL (DEFERRED — `/lab` post-hackathon)

> Distinct from `/journal` (Lab Notebook §4 above). The Lab Notebook is a chronological
> findings journal; the Power Notebook is a Jupyter-style REPL for L4 researchers
> programmatically driving the MCP API. Defer until L4 power users actually request it.

```
App screen of an xvn notebook for programmatically building a trading strategy via the MCP
API. Dark mode, desktop 16:9. Single centered column ~860px wide on a darker page surface,
mimicking a Jupyter / Observable notebook.

Top bar: file name "building_eth_mr.xvnnb", a kernel status pill "xvn-engine · ready" in
amber-gold, and "Run all" / "Restart" ghost buttons.

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

Active cell has a amber-gold left border. Below the last cell: an empty "+ new cell" affordance.

Mood: code-first, programmable, L4 researcher tool.
```

### 19. Spreadsheet  —  parameter sweep matrix (DEFERRED — paid-tier batch eval)

```
App screen of an xvn parameter sweep — paid-tier batch eval rendered as a heatmap-style
matrix. Dark mode, desktop 16:9.

Top bar: wordmark, breadcrumb "Sweep · eth-mr · bull-q1-25", a chip "16 runs · 4 running
· 12 done", a primary amber-gold "Run sweep" button (disabled, sweep already running).

Page body: a large rectangular grid taking ~70% of the page. Rows are bb_period values
(14, 20, 30, 50). Columns are rsi_oversold values (20, 25, 30, 35, 40). Each cell shows a
sharpe number in monospace; cell background color encodes value via a green-to-cinnabar
gradient (high values amber-gold-tinted, low values cinnabar-tinted, ~50% saturation, never pure).
The cell at bb_period=20, rsi_oversold=20 reads "1.62" with a small ▲ glyph and a subtle
amber-gold ring indicating it is selected.

Right rail (300px): "Drilldown · run 01H8N · bb20 / rsi20" header; below it a small equity
line chart, then a metrics list (sharpe 1.62, dd −7.1%, trades 47, tokens 29k), then three
finding rows with severity dots.

Below the heatmap, two side-by-side smaller heatmaps labeled "seed #1234" and "seed
#5678", each 4×5, intensity-shaded, showing seed-stability.

Mood: research-lab spreadsheet, data-dense, inviting drilldown.
```

### 20. Pass-Ribbon (Ticker)  —  ambient persistent footer (DEFERRED — Move H)

> The ideonomy evaluation surfaced this as Move H (ambient deployment ticker), deferred
> in favor of the higher-leverage moves A/B/E/F/I. Pick this up after the v0.2 bundle
> ships.

```
UI fragment — render only a horizontal strip ~64px tall at the bottom of a 1440px desktop
viewport, dark mode, docked, thin slate top-border, soft upper drop-shadow.

Strip content, left to right:
- A amber-gold pulsing dot, then "live" label.
- Three live deployment pill cards, each ~340px wide:
  - "paper-eth-mr-v3 · long_open ETH 0.05 · ●passed · 12s ago" with a amber-gold up-tick.
  - "paper-btc-mom · flat BTC · ●passed · 41s ago" with a slate dot.
  - "paper-eth-mr-v3 · close ETH +1.2% · ●passed · 2m ago" with a small amber-gold trade glyph.
- A vertical divider.
- A small "All deployments ↗" link in amber-gold and a tiny pulse-bar mode toggle button (which
  would collapse the strip to an 8px just-status-colors bar).
- Far right: a small expand-up chevron for the full /live view.

Aspect: very wide and short. The strip persists across page navigation — the design must
read as "ambient", not as a primary surface.
```

### 21. Slot Machine (starter spinner)  —  L1 onboarding (DEFERRED — see slot-machine spec)

> See `docs/superpowers/specs/2026-05-08-slot-machine-design.md` — the slot machine is a
> meta-strategy generator, not a UI archetype. The L1 "starter spinner" UI prompt below
> is a separate concept that uses slot-machine framing as an onboarding ramp; it's
> deferred until the slot-machine engine ships.

```
App screen of an xvn "starter spinner" — a single hero card that suggests a complete
strategy by spinning three reels. Dark mode, desktop 16:9, but the active surface is one
centered card ~960px wide on a dimmed page background.

Card title: "Not sure where to start? Spin a starter." Subtitle: "We'll suggest a complete
strategy you can run in paper mode in under a minute."

Card body: three large reel-style picker columns side by side, each ~260px wide and
~340px tall, each with a beveled inner edge giving a slot-machine feel but in a flat,
modern style (no skeuomorphic bevels — use amber-gold accents). Each reel shows three values
stacked vertically with the middle value highlighted by a amber-gold bar:
- Reel 1 "Template": "Trend", "Breakout", "Mean reversion" (highlighted), "Momentum",
  "Range".
- Reel 2 "Asset": "ETH", "BTC" (highlighted), "SOL", "WBTC", "AVAX".
- Reel 3 "Risk": "Aggressive", "Balanced", "Conservative" (highlighted).

Below the reels, a thin status row: "Suggested: Mean reversion · BTC · Conservative · est.
12k tokens / week · paper mode default".

Card footer: three buttons in a row — ghost circular "↻ Pull again", primary amber-gold "Keep
this →", and ghost "Tweak in inspector".

Mood: playful but restrained, low-stakes onboarding for L1 users.
```

---

## Iteration ladder  (the spectrum from rough draft → handoff)

GPT Image 2 is best at the rough-draft end. Walk down the spectrum after the first
generation:

1. **Draft pass.** Use the prompt verbatim. Generate 2–3 variations. Pick the
   one whose *hierarchy* is strongest — which screen makes you instantly
   understand where to look first. Don't worry about copy fidelity yet.

2. **Hierarchy tightening.** Re-prompt with: "Same screen, but make the
   [primary action / primary metric / live region] visually dominant. Reduce
   competing emphasis on [X]." Generate 1–2 variants.

3. **Copy fidelity.** Re-prompt with: "Same screen, but the exact copy on
   [region X] must read '[verbatim string from the spec]'. Keep all other
   layout identical." GPT Image 2 is strong at text — use it. Repeat for any
   region whose copy drifted.

4. **Density calibration.** If the screen is too sparse, re-prompt: "Increase
   information density — show 8 rows in the table instead of 4; add a
   secondary metrics strip below the chart." If too dense, the inverse.

5. **Component handoff.** Once you have a picked-direction screen, stop
   iterating in image space and go to code. Use the screen as a reference
   image alongside shadcn/ui or Tailwind. The image is now an *intent doc*,
   not a deliverable.

---

## Style nudges  (from the picked dimensions)

- **Visibility (what is emphasized vs collapsed):** every screen makes a
  visibility decision. Wizard hides complexity (visual progress is a
  *summary*, not a panel of every field); Inspector surfaces every field plus
  a live preview pane; Lab Notebook surfaces history; Control Tower surfaces
  *what changed since last visit*. When iterating, ask: "what is this screen
  choosing to hide, and is the choice load-bearing?"

- **Autonomy (AI-led vs user-led):** signal posture in the chrome. AI-led
  surfaces (Wizard, Chat Rail, Slot Machine) show typing indicators,
  suggested-reply chips, AI avatar dots, amber-gold pulsing cursors. User-led
  surfaces (Inspector form half, Notebook, Spreadsheet) show cursors, command
  palettes, code gutters. The v0.2 design deliberately *mixes* postures
  within a single screen (Inspector's split editor: form on the left is
  user-led, live preview on the right is AI-led) — but this is the
  exception, not the rule. When mixing, separate them spatially.

- **Cycle re-entry:** every diagnostic surface (Run Detail, Compare, Live
  Cockpit's veto panel) MUST surface a "Draft variant from this →" button. If
  a generated screen shows findings or vetoes without that button, re-prompt
  to add it. The cycle's most-traveled edge (Diagnose → Hypothesize) is
  load-bearing for the whole product.

- **Chat rail presence:** every authenticated route shows the chat rail in
  some state. If a generated screen omits it entirely, re-prompt: "Add the
  chat rail collapsed on the right edge — 40px icon strip with the
  speech-bubble icon and a small amber-gold unread dot." The rail's existence
  signals "the AI is here" even when the user isn't engaging it.

- **Age (era of the design):** explicitly target "contemporary 2026 fintech
  tooling". Counter-prompt against: skeuomorphic glass, drop-shadow-heavy
  2010s flat, oversaturated consumer-app gradients, AI-generated gloss. The
  reference family is Linear / Vercel / Stripe / TradingView, not Robinhood
  and not Bloomberg.

- **No 100% white borders in dark mode** (project rule): if a generated
  screen has pure-white borders or `#fff` lines, regenerate with the explicit
  instruction "borders are 1px slate (#1F2630), never white, never #FFF".
