# GPT Image 2 — XVN Dashboard Prompt Library v1 (Folio · Dark + Light)

> **Status:** v1 · 2026-05-10. Cut of `gptprompts.md` to the v1-test surface.
> **Companion to:** [`v1-shipping-plan.md`](../../v1-shipping-plan.md), [`docs/design/ui-elements.md`](./ui-elements.md), [`docs/design/themes.md`](./themes.md), [`docs/design/gptprompts.md`](./gptprompts.md) (the full library, including deferred archetypes).
> **Theme:** Folio (warm dark + classical serif headings, sans body) — the canonical xvn theme. This file ships **both dark and light variants** of Folio so the design team can render every v1 route in both modes and pick one for production. v1 ships **one** mode in product (no theme switcher); both renders exist only for the design selection process.
> **Out of v1 scope:** marketplace, live cockpit (`/live/<id>`), lab notebook (`/journal`), autoresearcher routes, deferred archetypes (Slot Machine, Spreadsheet, Power Notebook, Canvas, Pass-Ribbon). See `v1-shipping-plan.md` §"Routes NOT shipping for v1 test."

---

## How to use

1. Open ChatGPT with GPT Image 2.
2. Paste **one** of the two "Folio Shared Design System" blocks below — Dark for dark-mode renders, Light for light-mode renders. Paste it once per session.
3. Paste any single "Prompt Library — v1" prompt block. GPT renders that route in the mode established by the design system block. Each prompt is independent — no need to re-paste the design system between routes within the same session.
4. Save renders as `<route>-folio-{dark|light}.png` next to existing `folio1.png` / `quant1.png`.
5. Render every v1 route in both modes. The design handoff reviews both, then picks one mode for the v1 visual treatment.

For the global chrome elements (top nav, chat rail, command palette, toasts), the design system block already specifies their look. The dedicated chrome prompts later in this file render them in isolation when the design team wants to lock those components without a full-page context.

---

## Folio Shared Design System — DARK MODE (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: contemporary 2026 fintech tooling, with a quietly classical typographic
register. Restrained, technical, confident. Inspirations: NousResearch (the quieter pages),
Linear, Vercel, TradingView. Avoid skeuomorphism, avoid corporate stock illustration, avoid
parchment textures, avoid alchemical sigils, avoid wizard-hat or scroll iconography, avoid
cartoonish ornament, avoid sparkles, avoid emoji icons, avoid "AI generated" gloss.

Theme name: Folio Dark. The visual register hints at considered typography on a warm dark
ground — flirting with classical, never arcane.

Palette (DARK MODE — Folio Dark):
- Background: warm graphite #0E1014 (primary surface), #15171C (elevated surface), #1C1F25
  (panel surface). Slightly less blue than typical fintech dark mode.
- Borders: warm slate #232730, 1px solid, never pure white, never #FFF.
- Text: warm off-white #ECE6D9 for primary (slight cream), muted #9A9387 for secondary,
  dim #5E5A52 for tertiary metadata.
- Accent (single): warm amber-gold #D4A547 for primary actions, status-good, equity-up.
  This is the single biggest visible identity. Equity-down is cinnabar.
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
- Pill chips for tags / status (e.g., "paper", "conservative") — same 4px radius family.
- Status dots: filled circle (●) plus a one-word label. The dot color shifts with status:
  amber-gold for good states (running/passed/completed), warm amber for warnings,
  cinnabar for danger/vetoed/failed, slate-blue for info/queued, dim for offline.
- Buttons: primary is amber-gold fill with dark text; secondary is ghost with warm-slate
  border; danger is ghost with cinnabar text. All 4px radius. No gradients, no drop
  shadows, no "illuminated initial" caps. Sentence case.
- Tables: thin row dividers in warm-slate, monospace numerics, sticky header.
- Sparklines and equity charts: thin 1.5px line, amber-gold when up, cinnabar when down,
  soft gradient fill, no axis chrome unless specified.
- Icons: thin-line 1.5px stroke (Lucide style); never colorful glyphs.
- Texture: a 1–1.5% grain noise overlay on elevated surfaces (cards, modals); page
  background stays flat; chart data areas stay flat. The grain should be invisible at
  first glance — perceptible only when looked for.

Top nav (every authenticated route): "xvn" wordmark on the left, then nav links
"Setup · Strategies · Eval · Settings", then on the right: a chat-rail toggle icon
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

## Folio Shared Design System — LIGHT MODE (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: contemporary 2026 fintech tooling, with a quietly classical typographic
register. Restrained, technical, confident. Inspirations: NousResearch, Linear, Vercel,
TradingView, Stripe Press hardback book interiors (for the warm paper feel). Avoid
skeuomorphism, avoid corporate stock illustration, avoid parchment textures, avoid
alchemical sigils, avoid wizard-hat or scroll iconography, avoid cartoonish ornament,
avoid sparkles, avoid emoji icons, avoid "AI generated" gloss.

Theme name: Folio Light. Same considered typographic register as Folio Dark, rendered
on warm cream paper instead of warm graphite.

Palette (LIGHT MODE — Folio Light):
- Background: warm cream #F4EFE6 (primary surface — paper, NOT pure white), softer cream
  #F8F4ED (elevated card surface), oat #ECE6D9 (nested panel surface). Surfaces are warm,
  paper-tinted, never blue-tinted. Never pure white #FFFFFF.
- Borders: warm slate #B5AC9C (faded slate), 1px solid, never pure black #000, never
  #111111. The border color is a desaturated equivalent of the dark-mode warm slate.
- Text: ink-warm-black #1C1814 for primary (warm off-black, not pure black), warm-graphite
  #5E5345 for secondary, dim-buff #9C8E78 for tertiary metadata.
- Accent (single): warm amber-gold #B8862E for primary actions, status-good, equity-up.
  Slightly darker than dark-mode amber-gold (#D4A547) for legibility on cream paper. Same
  identity, same role — chart equity-up lines are amber-gold.
- Status: amber #C7822A for warning, cinnabar #B23C32 for danger / equity-down (slightly
  desaturated for legibility on cream), slate-blue #4F77A5 for info.
- Brand wordmark "xvn" rendered in Cormorant Garamond italic lowercase, ink-warm-black.
  Same wordmark as Folio Dark — only the ink color changes.

Typography: identical to Folio Dark.
- Display / heading: Cormorant Garamond. Sizes 24–48px. Italic permitted on subtitles.
- Body / UI: Inter. Sizes 13–16px.
- Numeric: JetBrains Mono. Sizes 12–14px. Tabular figures ON.

Components: identical structure to Folio Dark; only colors invert.
- Cards: 4px rounded corners, 1px warm-slate (#B5AC9C) border, on softer-cream background
  (#F8F4ED), elevated above the cream page (#F4EFE6).
- Pill chips: same 4px radius family, warm-slate border on cream fill.
- Status dots: filled circle (●) plus one-word label. amber-gold #B8862E for good states,
  amber #C7822A for warnings, cinnabar #B23C32 for danger / vetoed / failed, slate-blue
  #4F77A5 for info / queued, dim-buff #9C8E78 for offline.
- Buttons: primary is amber-gold fill #B8862E with cream text #F4EFE6; secondary is ghost
  with warm-slate border, ink-warm-black text; danger is ghost with cinnabar text. 4px
  radius, no gradients, no shadows.
- Tables: thin row dividers in warm-slate, monospace numerics, sticky header. Header row
  on oat panel surface (#ECE6D9) for a quiet section header.
- Sparklines and equity charts: 1.5px line, amber-gold for up, cinnabar for down, soft
  10-15% opacity gradient fill in the same color, no axis chrome.
- Icons: 1.5px stroke (Lucide style), ink-warm-black; never colorful.
- Texture: 1–1.5% paper grain on elevated card surfaces — warmer / more visible than in
  dark mode because we're now ON paper, not under it. The cream page itself stays flat.

Top nav: same structure as Folio Dark. "xvn" wordmark left, "Setup · Strategies · Eval ·
Settings" middle, chat toggle + daemon pill + LLM pill + avatar right. Active route gets
an amber-gold underline.

Chat rail: 360px expanded, 40px collapsed. Surface is oat panel #ECE6D9 with a 1px
warm-slate left border (no shadow on light mode — paper rail sits flush against page).
Header "xvn agent · ● Online" with amber-gold dot.

Status pattern: filled dot (●) plus one-word label. Same vocabulary as Folio Dark.

Density: moderate-to-high. Same as Folio Dark.

Voice in copy: plain English. Same as Folio Dark.

Render quality: sharp 1x desktop screen, 16:9 by default, exactly as if it were a real
app screenshot — pixel grid clean, no painterly textures, no bokeh, no faux-3D.
```

---

## Prompt Library — v1 in-scope routes

Each prompt is a single block. Paste verbatim **after** one of the two design system blocks above. The prompt itself does not specify dark or light — that is established by which design system block you paste first. Render every prompt in both modes.

---

### 1. Control Tower — `/` (default landing for second-and-subsequent visits)

```
App screen of the xvn home page — a "what needs your attention" dashboard for a power user
who has been away from the app for a day. Mode and palette per the design system block
above. Desktop 16:9. The chat rail is docked on the right, collapsed (40px icon strip
showing the speech-bubble glyph and a small amber-gold unread dot).

Top nav: lowercase "xvn" wordmark, then "Setup", "Strategies", "Eval", "Settings". "Eval"
is currently inactive; the home page is not in primary nav. Right side: chat rail toggle
(with the amber-gold unread dot), a "● Connected" daemon pill, "● Anthropic" LLM pill, and
a circular avatar with initials.

Page header strip: title "Home" in primary text color, subtitle in secondary text "5 drafts
· 12 runs since you were last here · 18 hours ago". Right side of strip: a primary
amber-gold button "Open setup agent" and a ghost button "New strategy". Below the header,
a small pill row of time-window selectors: "Today", "7d" (active, amber-gold outline),
"30d", "All".

Page body: two-column grid (8/12 + 4/12).

Left column ("operational"):
1. "Needs attention" card. Title in primary, count chip "3" in amber-gold. Three rows visible:
   - Cinnabar severity dot · "paper-eth-mr-v3 dropped −4.1% yesterday" · "Open ↗" link in
     amber-gold.
   - Amber severity dot · "Critical finding: regime fit mismatch (eth-mr-v3, scenario
     crypto-bear-q3-2024)" · "Open ↗".
   - Slate-blue info dot · "I noticed 3 of your last 5 evals failed in chop. Want me to
     draft a chop-aware variant?" with an "Ask the agent →" link in amber-gold.
2. "Eval queue" card. Two rows visible:
   - "Queued: btc-momentum-v2 on chop-q2-25 · ETA ~3min" with a thin progress bar at 12%
     in amber-gold and a "Cancel" ghost button.
   - "Running: eth-mr-v3 on bull-q1-25 · 41%" with a progress bar at 41% and ghost
     "Open ↗" link.

Right column ("knowledge & cycle re-entry"):
1. "Recent findings" card with "Open all findings →" link in amber-gold at top-right. Three
   rows showing severity dot · finding kind ("Regime fit mismatch", "Overtrading",
   "Underperf in chop") · truncated summary · a strategy chip · a "Draft variant from this
   →" ghost mini-button.
2. "In your workshop" card. Three draft rows: name · template chip ("Catches uptrends",
   "Buys dips", "Trades the range") · status pill ("Eval-ready" amber-gold outline /
   "Draft" slate / "Validating" amber) · "Open in Inspector →" link.
3. Below the workshop card, a single thin line: "You've forked btc-momentum 4 times this
   week — see lineage →" in dim text, with a small "Coming soon" chip in slate.

Mood: ops console for a power user returning to in-flight work. Calm, scannable, every
card answers "what should I look at first?" The amber-gold accents draw the eye to the
attention-grabbing items first.

Note: this v1 view intentionally has NO "Live deployments" rollup card and NO live
cockpit links — long-running deploy cockpits are deferred. Eval queue replaces the
deploys panel as the only "things in motion" surface for v1.
```

---

### 2. Wizard — `/setup` (first-run landing + on-demand full page)

```
App screen of the xvn setup agent — an AI-led conversation that builds a trading strategy
for the user. Mode and palette per the design system block above. Desktop 16:9. The chat
rail is hidden on this route (the chat IS the page).

Top nav: standard nav as defined in the design system. "Setup" active (amber-gold
underline). Top right shows daemon and LLM pills.

Page body: two-column split, ~58/42.

Left column (chat, ~58%): scrollable thread of messages. Header strip reads "xvn setup
agent · ● Online" in primary. Three visible message exchanges:
- Assistant message in panel-surface card with amber-gold left border, amber-gold avatar
  dot: "Hi! I'm the xvn setup agent. I'll help you build or pick an AI trading bot.
  What's your goal today?"
- User reply in tinted panel-surface card on the right: "I want a bot that catches
  uptrends in BTC but exits fast when chop starts."
- Assistant follow-up: "Got it — using the 'Catches uptrends' template with a chop-aware
  exit. Let me set up the slots." Below this message, an inline tool-call card (collapsed)
  reads "Tool: create_strategy(template='trend_follower', name='btc-trend-chopaware')"
  with a small "▸ result" toggle in dim text.

Below the thread, a row of three quick-reply chips in amber-gold outline: "Try a free
strategy", "Build from a template", "Diagnose a recent run".

Composer at the bottom: a single-line textarea with placeholder "type your reply…", a "+"
attachment button (ghost), and an amber-gold arrow-send button labeled "Send" with a
small "⌘↵" keyboard hint.

Right column (visual progress, ~42%): card titled "Strategy in progress" with a status
pill "● Drafting" in amber-gold outline. Below the title:
- Section "Template" with selected chip "Catches uptrends" in amber-gold outline + small
  "Change" link in dim text.
- Section "Agents" — three slim rows: "② Regime — claude-sonnet-4.6 · ● Filled" with
  amber-gold check; "③ Intern — claude-sonnet-4.6 · ● Filled"; "④ Trader —
  claude-sonnet-4.6 · ⏳ Drafting" with a small amber-gold spinner and an "Edit prompt"
  ghost link.
- Section "Mechanics" — chip row: "Cadence: 15m", "Asset: BTC/USD", "Stop: ATR×2".
- Section "Risk" — segmented radio with three options where "Conservative" (default,
  selected with amber-gold background) is highlighted. A tiny "Show details" toggle.
- Section "Last eval" — empty state in dim text: "No eval yet — finish drafting and I'll
  run one."

Sticky footer of the right column: primary amber-gold "Save draft" + ghost "Open in
Inspector" + ghost (disabled) "Run paper trade".

Mood: AI-led, conversational, with a parallel visual of what's being built so the user
can SEE the strategy taking shape. Posture is "AI is doing the work, user is steering."
```

---

### 3. Wizard first-run no-key state — `/setup` empty state for fresh installs

```
App screen of the xvn setup agent on first run, when the user has no LLM provider key
configured yet. Mode and palette per the design system block above. Desktop 16:9. Chat
rail hidden (this is the /setup route).

Top nav: "xvn" wordmark, "Setup" active, all other nav items in dim (the tabs are
present but visually de-emphasized to signal that the user is not yet onboarded). Right
side: NO daemon pill, NO LLM pill (the LLM isn't connected yet), just the avatar with
initials.

Page body: a single centered column ~640px wide.

Top of column: a quiet display title in Cormorant Garamond italic 32px "Welcome to xvn."
(period included, in amber-gold). Subtitle in Inter 16px secondary text: "Before we can
draft a strategy, the agent needs an LLM provider key. This stays on your machine."

Below: a card titled "Add a provider" with the title in Cormorant Garamond 20px. Inside
the card:
- A radio-group of provider options: "Anthropic" (selected, amber-gold dot), "OpenAI",
  "OpenAI-compatible (custom endpoint)", "Local (candle)". Each radio has a small
  one-line subtitle: "claude-sonnet-4.6, opus-4.7", "gpt-4o, o-series", "any base URL +
  /v1/chat/completions", "in-process inference, no network".
- For "Anthropic" (selected): an API-key input with placeholder "sk-ant-..." and a
  small eye-icon ghost button to toggle visibility, plus a "Test connection" ghost
  button.
- A "Set as default for new strategies" checkbox (checked by default) in 13px Inter.

Below the card: a primary amber-gold button "Save and open the agent" full-width within
the column. Ghost link below it: "Skip — I'll add a provider later" (dim text).

Below that: a faint horizontal slate hairline, then a footer line in dim 13px:
"Already configured xvn elsewhere? Import settings from a file →" with the link in
amber-gold.

Mood: warm, restrained welcome. The page reads like the inside cover of a printed
manual — one question, one action, one line of reassurance. The amber-gold appears only
on the welcome word and the primary action.
```

---

### 4. Inspector — `/authoring/<draft_id>` with live-preview split for LLM slots

```
App screen of the xvn strategy Inspector — a structured editor for one strategy bundle.
The user is currently on an LLM slot section (the "Trader" slot), so the center column
is rendered as a live-preview split. Mode and palette per the design system block above.
Desktop 16:9. The chat rail is docked on the right, collapsed to 40px icon strip.

Top nav: standard nav, "Strategies" active (amber-gold underline).

Page body: three columns plus the chat rail. Left rail 220px (bundle outline), center
flex (split editor), right rail 280px (validation + token estimate + bundle preview),
chat rail 40px collapsed.

Left rail — Bundle outline:
- "Manifest" header with collapsible chevron, two sub-items "Identity" and "Eval
  attestations" indented.
- "Layers" header with seven sub-items: "① Data", "② Regime classifier [LLM]" (with
  amber-gold [LLM] chip), "③ Intern [LLM]", "④ Trader [LLM]" (highlighted with amber-gold
  left bar — this is the active section), "⑤ Entry / Exit rules", "⑥ Risk", "⑦ Execution".
- "Skills" header with one sub-item "Attached skills".
- "Validation" footer line with "● 2 warnings, 0 errors" in amber.
- Tree footer in dim mono: "Bundle hash: 0xa83…f12  [copy]".

Center column — split editor for the "Trader" slot. Header strip: "④ Trader — LLM slot"
in primary. Below the header, a 50/50 horizontal split with a draggable divider in the
middle (thin slate handle).

Left half of split (form):
- "Use this agent" toggle in amber-gold (on, with the toggle disabled because Trader is
  required).
- "Model" dropdown set to "claude-sonnet-4.6". Beside it, a small "Provider: Anthropic"
  chip in slate.
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
- Sticky bottom: primary amber-gold "Run eval" button + ghost "Save draft" + ghost
  (disabled, "Coming soon" chip in slate) "Publish…".

Right edge — Chat rail collapsed (40px). Just the speech-bubble icon at the top.

Mood: power-user depth tool, but the live-preview pane makes it FEEL like the AI is still
in the loop — every prompt edit re-fires against the fixture and the user SEES the
behavior change, not infers it. The form on the left is the dial; the preview on the
right is the engine note.
```

---

### 5. Strategies list — `/strategies` (drafts table)

```
App screen of the xvn strategies index — a tabular directory of every strategy bundle
the user has drafted. Mode and palette per the design system block above. Desktop 16:9.
Chat rail docked right, collapsed (40px).

Top nav: standard, "Strategies" active.

Page header strip: title "Strategies" in primary. Subtitle in dim secondary "8 drafts ·
3 eval-ready · 12 runs across all drafts". Right side: a primary amber-gold "New
strategy" button with a "▾" dropdown chevron (opens template picker), and a ghost
button "Import bundle…".

Filter bar below the header: a row of chip filters — "All" (active), "Eval-ready",
"Drafting", "Validating". To the right: a "Sort: Most recent ▾" dropdown and a search
input "Search by name, template, or hash…" with a small ⌘K hint at the right edge of
the input.

Strategies table: 8 rows, sticky header. Columns:
- Name (Cormorant Garamond Bold for primary name, dim secondary subtitle below)
- Template chip (slate outline, e.g., "Catches uptrends", "Buys dips", "Trades the range",
  "Mean reversion", "On-chain")
- Status pill ("Eval-ready" amber-gold outline / "Drafting" slate / "Validating" amber)
- Forked from (slate chip if forked, dim "—" if root)
- Best Sharpe (mono, amber-gold for ≥1, dim for none)
- Last run (relative time, dim secondary)
- ⋯ row actions menu (ghost three-dot)

Row examples:
- "btc-trend-chopaware" · template "Catches uptrends" · status "Eval-ready" · forked
  from "btc-momentum-v1" · Sharpe 1.62 · "8m ago"
- "eth-mr-v3" · template "Mean reversion" · status "Eval-ready" · forked from
  "eth-mr-v2" · Sharpe 1.41 · "2h ago"
- "btc-momentum-v2" · template "Catches uptrends" · status "Validating" · forked from
  "btc-momentum-v1" · Sharpe — · "running now"
- "stablecoin-flow-v1" · template "On-chain" · status "Drafting" · forked from "—" ·
  Sharpe — · "yesterday"
- "sol-range-v1" · template "Trades the range" · status "Eval-ready" · forked from "—" ·
  Sharpe 0.84 · "3d ago"

Footer below the table: a single thin line in dim 13px text "8 of 8 drafts shown" with
ghost link "Show archived (4) →" on the right.

Mood: a clean, scannable directory. The Cormorant primary names give the table a
classical-roster feel — like the cast list of a play printed in a programme. The amber-gold
appears only on Sharpe values that crossed 1, marking the "ready" candidates at a glance.
```

---

### 6. Run list — `/eval/runs`

```
App screen of the xvn eval runs list — a tabular directory of every evaluation run, with
filters and bulk-compare action. Mode and palette per the design system block above.
Desktop 16:9. Chat rail docked right, collapsed (40px).

Top nav: standard, "Eval" active.

Page header strip: title "Eval runs" in primary. Subtitle in dim secondary "12 runs · 7
strategies · 4 scenarios". Right side: tabs "Mine" (active) / "All" / "Published evals
(0)" — note "Published evals" is dim with a small "Coming soon" chip. Below the tabs, a
header actions row: primary amber-gold "New run" + ghost "Compare selected (2)" (enabled
because 2 rows are checked).

Filter bar: a multi-select chip row "eth-mr-v3 ✕  btc-momentum-v1 ✕" + "Add strategy +",
then "All scenarios ▾", then "All / Backtest / Paper" segmented control with "All"
selected, then a "Date range" picker, then a "Sort: Most recent ▾" dropdown.

Run table: 8 rows, sticky header. Columns:
- ☐ checkbox
- Run ID (mono ULID short, 6 chars + dim "...")
- Strategy
- Scenario
- Mode pill ("Backtest" slate / "Paper" amber-gold outline)
- Status with dot ("● Completed" amber-gold / "● Running 42%" with inline progress bar /
  "● Queued" slate-blue / "● Failed" cinnabar)
- Sharpe (mono)
- Total return (mono, amber-gold for positive, cinnabar for negative)
- Max DD (mono, cinnabar)
- Win rate
- n trades
- Tokens (mono, dim)
- Started (relative)
- ⋯ ghost three-dot row actions

Two rows are checked; their checkboxes are amber-gold filled. The Compare button at the
top is enabled and shows "(2)" beside its label.

Below the table: a footer in dim 13px text "Showing 8 of 12. Older runs load on scroll."

Mood: a scannable, dense market-page-style table. The status dots and amber/cinnabar
mono numerics let the eye triage 8 rows in two seconds. The two checked rows are a
visual setup for "click Compare to overlay them."
```

---

### 7. Run detail — `/eval/runs/<run_id>` (completed backtest)

```
App screen of an xvn evaluation run detail page — a completed backtest run showing all
its metrics, trades, and findings. Mode and palette per the design system block above.
Desktop 16:9. The chat rail is docked on the right, collapsed (40px).

Top nav: standard nav, "Eval" active.

Page header strip: title "Run 01H8N7Z · eth-mr-v3 backtest" in primary. Subtitle in dim
secondary "Strategy: eth-mr-v3 · Scenario: Crypto bull Q1 2025 · Backtest". Right side: a
"● Completed" amber-gold status pill, then ghost "Re-run", ghost "Extract findings",
ghost "Download tape", ghost "Compare with…", and a primary amber-gold "Draft variant
from this run →" button.

Page body: main content (flex) plus chat rail (40px collapsed).

1. KPI tile row — 4 tiles spanning full width:
   - "Total return" tile: "+18.4%" big in amber-gold with a small "vs +12.1% buy & hold"
     in dim below.
   - "Sharpe" tile: "1.62" with a small ▲ glyph and "+0.41 vs baseline" beneath.
   - "Max drawdown" tile: "−7.1%" in cinnabar with "vs −9.2% baseline" beneath.
   - "Win rate" tile: "58%" with a tiny donut showing 27/47 wins.

2. Equity curve panel — wide chart ~320px tall. Two lines: a thicker amber-gold line for
   the strategy and a thinner slate line labeled "Buy & hold". Toggle above the chart:
   "Show drawdown shading" (off, slate). A faint shaded region beneath the equity line
   shows drawdown periods.

3. Trade markers panel — a horizontal price chart of ETH/USD over the same time range
   with markers at every decision point: ▲ buy in amber-gold, ▼ sell in cinnabar, ○ flat
   in slate, ✕ close in primary text. Hover state shown on one marker with a tooltip
   "2025-01-15 08:00 · long_open · 0.05 · fill 2845.20 · realized PnL +1.4% · ▸ decision
   JSON".

4. Trade ledger table — sortable, 6 visible rows. Columns: "Time" (mono), "Asset",
   "Side" (chip amber-gold/cinnabar), "Qty" (mono), "Entry" (mono), "Exit" (mono), "PnL"
   (mono, colored), "Holding period", "Fees", "Decision" (link "View ↗").

5. Findings panel — header "Findings · 4" with a "Re-extract" ghost button. Four rows:
   - Slate-blue info dot · "regime_drift · Mid-bull regime shifted to chop on 2025-02-18;
     strategy adapted within 6 bars" · "Evidence ↗" link · primary amber-gold "Draft
     variant from this →" ghost button.
   - Amber warning dot · "drawdown_concentration · 60% of total drawdown occurred in 3
     consecutive trades 2025-03-01..03" · "Evidence ↗" · "Draft variant from this →"
     (amber-gold).
   - Cinnabar critical dot · "tail_risk · single trade lost 4.2%, 2x next-largest loss" ·
     "Evidence ↗" · "Draft variant from this →" (amber-gold).
   - Slate-blue info dot · "underutilization · agent went flat for 12 consecutive bars
     2025-02-27..28" · "Evidence ↗" · "Draft variant from this →".

   Bottom-right corner of the panel: severity legend "● info  ● warning  ● critical".

Right edge — Chat rail collapsed (40px) with speech-bubble icon and a small amber-gold
unread dot.

Mood: post-flight debrief, dense data with clear hierarchy — KPIs first, equity story
second, trades third, findings fourth. The "Draft variant from this →" buttons on every
finding are the cycle-closure affordance: every diagnostic line is one click away from a
new hypothesis.
```

---

### 8. Compare view — `/eval/compare?ids=…`

```
App screen of an xvn comparison view — three evaluation runs of the same strategy across
different scenarios, overlaid for direct comparison. Mode and palette per the design
system block above. Desktop 16:9. Chat rail collapsed.

Top nav: standard, "Eval" active.

Page header strip: title "Compare runs" in primary. Subtitle in dim mono showing three
ULIDs as clickable tokens "01H8N7Z, 01J2P9R, 01K9R5T". Right side: ghost "Add another
run…", ghost "Save comparison", ghost "Export PNG", primary amber-gold "Draft variant
from this comparison →".

Page body:
1. Equity overlay panel — large chart ~360px tall with three lines in distinct accent
   colors: amber-gold (run A, "eth-mr-v3 on bull-q1-25"), slate-blue (run B, "eth-mr-v3
   on chop-q2-25"), amber (run C, "eth-mr-v3 on bear-q3-24"). Shared time axis (relative
   time, 0 to 90 days). Synced crosshair shown on one of the lines. Above the chart,
   three pill toggles per line with color swatches and on/off visibility toggles.
   Chart-mode selector top-right: "Equity" (active, amber-gold outline) / "Cumulative
   return" / "Drawdown".

2. Trade markers panel — a strategy radio row at the top with three options "eth-mr-v3
   on bull-q1-25" (selected, amber-gold), "...on chop-q2-25", "...on bear-q3-24". Below,
   a single-asset chart with that strategy's markers visible.

3. Metrics comparison table — 3 columns (one per run) plus a metric column. Best value
   per row gets an amber-gold cell border. Rows: "Total return" (+18.4%, +3.1%, −2.4%),
   "Sharpe" (1.62, 0.41, −0.18), "Max drawdown" (−7.1%, −12.4%, −18.9%), "Win rate"
   (58%, 51%, 42%), "n trades" (47, 39, 32), "Tokens used" (51,200, 48,900, 45,400).
   Sortable column headers with caret glyphs.

4. Findings panel — flat list across all runs in the set, grouped by kind. Filter chips
   at top: "info", "warning", "critical". Five visible rows, each with: severity dot,
   finding kind, summary (truncated), source-run chip in slate, "Draft variant from this
   →" amber-gold ghost button.

Mood: side-by-side diagnostic. The eye can scan three runs simultaneously and ask "what
do the winners share that the losers don't?" The "Draft variant from this →" buttons turn
that diagnostic into a generative hypothesis.

v1 cut: no drawdown overlay, no regime-shaded chart background, no NL Q&A surface
beneath the metrics table — those are post-v1.
```

---

### 9. Settings shell + Providers — `/settings/providers` (default sub-page)

```
App screen of xvn settings — the Providers sub-page. Mode and palette per the design
system block above. Desktop 16:9. Chat rail docked right, collapsed (40px).

Top nav: standard nav, "Settings" active (amber-gold underline).

Page body: two-column layout — a left sidebar 240px wide and a content area on the right.

Left sidebar (sticky):
- Sidebar title "Settings" in Cormorant Garamond 22px primary text, with a thin slate
  divider beneath.
- Nav list, vertical, each item ~36px tall:
  - "Providers" (active — amber-gold left bar 3px wide, primary text)
  - "Brokers"
  - "Daemon"
  - "Identity" (dim text, with a tiny "Read-only" chip in slate)
  - "Danger zone"
  - thin slate divider
  - "Marketplace" (dim text, with a "Coming soon" chip in slate)
  - "Autoresearch" (dim text, "Coming soon" chip)
- Sidebar footer in dim 12px mono: "xvn 0.1.0 · build a83…f12".

Content area — Providers:

Header strip: title "Providers" in Cormorant 32px, subtitle in Inter 14px dim "Where the
LLM agents in your strategies get their tokens. Each provider is a registry entry in
config/default.toml."

Header right: primary amber-gold "Add provider" button.

Provider table — 4 rows, sticky header. Columns:
- Name (primary)
- Kind chip ("Anthropic" / "OpenAI-compatible" / "Local (candle)") in slate outline
- Default radio (amber-gold filled if default)
- Status with dot ("● OK" amber-gold / "● Untested" slate-blue / "● Failed" cinnabar)
- Last tested (relative time, dim mono)
- ⋯ row actions menu (ghost three-dot)

Row examples:
- "anthropic-prod" · Anthropic · ● Default selected · ● OK · "2 min ago" · ⋯
- "openai-fallback" · OpenAI-compatible · — · ● OK · "1 day ago" · ⋯
- "groq-fast" · OpenAI-compatible · — · ● Untested · "—" · ⋯
- "local-candle" · Local (candle) · — · ● OK · "3h ago" · ⋯

Below the table, a "Default for new strategies" callout card on panel surface: "New
strategies created by the wizard or CLI will use anthropic-prod / claude-sonnet-4.6 by
default. Change at any time without affecting existing strategies." with a "Change
default →" ghost link in amber-gold.

Below that callout, a "Findings extractor provider" card: "The findings extractor uses
anthropic-prod / claude-sonnet-4.6 (default). Override per-run with the
--findings-provider flag on `xvn eval run`." with a "Change…" ghost link.

Right edge — Chat rail collapsed (40px) with route-context chip behind: "Context:
provider settings".

Mood: an admin page that signals "you have full control" without overwhelming. The
provider table reads as a small ledger, not a dashboard. The amber-gold appears only on
Default and OK statuses — the cells where the user's eye should land first.
```

---

### 10. Settings · Brokers — `/settings/brokers`

```
App screen of xvn settings — the Brokers sub-page. Mode and palette per the design
system block above. Desktop 16:9. Chat rail collapsed.

Top nav: "Settings" active.

Page body: same two-column layout as the Providers page. Left sidebar identical, with
"Brokers" now active (amber-gold left bar). Right content area:

Header strip: title "Brokers" in Cormorant 32px, subtitle in Inter 14px dim "Where xvn
sends orders. v1 supports Alpaca paper for evaluation. Live brokers are deferred."

Header right: primary amber-gold "Connect a broker" button.

Broker cards — vertical stack of 3 cards, each ~120px tall:

Card 1 — Alpaca paper (connected, amber-gold left bar 3px wide):
- Title row: "Alpaca · Paper" in primary 18px Cormorant, "● Connected" amber-gold pill,
  ghost "Test connection" link.
- Detail row in dim Inter 13px: "API key: PKxxxxxxxxxxxxxxxxx · Account: paper-default ·
  Symbol coverage: BTC/USD" with a "Edit ▾" ghost link.
- Footer row: "Used by 4 paper-mode eval runs in the last 7 days · Last fill 18 minutes
  ago" in dim 12px.

Card 2 — Alpaca live (disconnected, slate left bar):
- Title row: "Alpaca · Live" in primary, "● Not connected" slate pill, ghost "Connect
  →".
- Detail in dim: "Live trading requires explicit operator confirmation per session."

Card 3 — Orderly · Mantle (deferred, very dim):
- Title row: "Orderly · Mantle" in dim primary, "Coming soon" slate chip.
- Detail in dim: "Non-custodial perpetual exchange on Mantle. Wired in v1.x once the
  trading-key store ships."

Below the broker cards, a "Symbol coverage" callout card: "Alpaca paper currently
supports BTC/USD only. Multi-asset symbol mapping is a v1.x follow-up. Eval scenarios
that reference non-BTC assets will fail in paper mode." with a small slate-bordered code
chip showing `crates/xvision-execution/src/alpaca.rs`.

Mood: same admin tone as Providers. The Alpaca-paper card carries the active session;
the others are visibly available but not yet load-bearing. The dim treatment on
"Orderly · Mantle" makes the deferral honest without hiding it.
```

---

### 11. Settings · Daemon — `/settings/daemon`

```
App screen of xvn settings — the Daemon sub-page. Mode and palette per the design
system block above. Desktop 16:9. Chat rail collapsed.

Top nav: "Settings" active.

Page body: two-column. Left sidebar identical, "Daemon" active. Right content area:

Header strip: title "Daemon" in Cormorant 32px, subtitle in Inter 14px dim "The
local xvn engine process that backs the dashboard."

Header right: ghost "Restart daemon" button (cinnabar text — destructive but ghost).

Status card — large card on panel surface, ~200px tall:
- Top row: "● Connected" amber-gold dot at 24px, then "xvn engine · running" in
  Cormorant 24px primary.
- Detail row in mono 13px: "PID 48721 · started 18h 12m ago · Process owner: edkennedy ·
  Listening on 127.0.0.1:7878".
- A 2-line metrics row: "Heartbeat OK · last beat 0.4s ago" / "DB: ~/.xvn/xvn.db · 12
  migrations applied · 24.8 MB".
- Right side of the card: a small line chart 200×60 of CPU + memory over the last hour,
  amber-gold line for CPU, slate-blue for RAM.

Below the status card, a "Migrations" details card (collapsed by default, with a "▸ show
all" chevron):
- One-line summary: "12 of 12 migrations applied. Latest: 004_search_index.sql · 18h
  ago."
- (When expanded) a small mono table of migration filenames + applied-at timestamps.

Below: a "Logs" card. Header "Recent log lines" with a ghost "Open log file →" link.
Body shows 5 monospace 11px log lines in a slate-tinted code block:
- "2026-05-10T18:42:07Z INFO  api::eval::run started run=01K8R5T strategy=eth-mr-v3"
- "2026-05-10T18:42:09Z INFO  alpaca: ● connected (paper)"
- "2026-05-10T18:43:14Z DEBUG eval::executor::backtest tick=42 nav=1018.42"
- "2026-05-10T18:44:01Z WARN  budget: provider 'groq-fast' returned 429, retrying in 1.0s"
- "2026-05-10T18:44:02Z INFO  budget: retry succeeded"

Below logs: a "Healthchecks" card with three status rows:
- "● DB connection" amber-gold
- "● Default LLM provider reachable" amber-gold
- "● Alpaca paper reachable" amber-gold

Mood: a quiet, monitoring-style page. No counter-intuitive controls. The user can see
that everything is fine without having to interpret anything.
```

---

### 12. Settings · Identity (read-only stub) — `/settings/identity`

```
App screen of xvn settings — the Identity sub-page in its v1 read-only stub state. Mode
and palette per the design system block above. Desktop 16:9. Chat rail collapsed.

Top nav: "Settings" active.

Page body: two-column. Left sidebar identical, "Identity" active (with the "Read-only"
chip beside it visible in the sidebar). Right content area:

Header strip: title "Identity" in Cormorant 32px, subtitle in Inter 14px dim
"Cryptographic identity that signs eval attestations and (in v1.x) anchors lineage to
ERC-8004 on Mantle. v1 is read-only — minting and edit flows ship with the blockchain
plan."

Header right: NO primary action (read-only). Only a ghost "Copy fingerprint" button.

Identity card — large card on panel surface:
- Top row: "Operator identity · ● Local" amber-gold dot, then a mono ULID-style id
  "01H8N7Z6P3QXVNF4KRTM7..." that's truncated with a "[copy]" ghost link.
- A small "Fingerprint" sub-row in mono dim text: "ED25519 · 64bd a83f 1207 e445 ..."
- "Created" line: "8 days ago · stored in ~/.xvn/keys/operator.ed25519".

Below the identity card, a quiet "What this signs" list card with three rows:
- "✓ Eval attestations (when you run `xvn eval run`)" — amber-gold check
- "✓ Findings (when extracted)" — amber-gold check
- "○ ERC-8004 lineage anchors (deferred — v1.x)" — dim circle with "Coming soon" chip
- "○ Marketplace listings (deferred — v1.x)" — dim circle with "Coming soon" chip

Below: a callout in slate-bordered card with reduced contrast: "Mint, edit, rotate, and
recover flows ship with the wallet plan. Until then your operator key is the only
identity, stored encrypted at rest in the path above." with no actions.

Mood: deliberately under-built. The page communicates "you have an identity, here's
what it signs, but the controls are not here yet." Honest about deferral without hiding
the surface.
```

---

### 13. Settings · Danger zone — `/settings/danger`

```
App screen of xvn settings — the Danger zone sub-page. Mode and palette per the design
system block above. Desktop 16:9. Chat rail collapsed.

Top nav: "Settings" active.

Page body: two-column. Left sidebar identical, "Danger zone" active (with no
amber-gold left bar — the active treatment is a 3px cinnabar left bar instead, signaling
caution). Right content area:

Header strip: title "Danger zone" in Cormorant 32px primary, subtitle in Inter 14px
cinnabar "Operations that delete data, halt running work, or reset state. Each requires
explicit typed confirmation."

Header right: NO primary action.

Danger cards — vertical stack of four cards, each ~140px tall, with a 1px cinnabar
left bar (not a full border — restraint):

Card 1 — "Halt all evals":
- Title in Cormorant 18px, subtitle in dim Inter 13px "Stops every queued and running
  eval. Already-completed runs are kept. Resumable: no — re-queue manually."
- Right side: a "Halt all" ghost button with cinnabar text, plus the phrase "Type
  HALT-ALL to confirm" in dim 12px.

Card 2 — "Reset SQLite database":
- Title in Cormorant 18px, subtitle "Wipes runs, findings, attestations, chat sessions,
  and search index. Strategy bundles on disk are NOT touched."
- Ghost cinnabar "Reset DB…" button + typed-confirmation hint "Type RESET-DB to confirm".

Card 3 — "Forget all chat sessions":
- Title, subtitle "Clears every saved chat thread across every route. The wizard starts
  fresh on next visit. Strategy bundles unaffected."
- Ghost cinnabar "Forget chats…" button + "Type FORGET-CHATS to confirm".

Card 4 — "Uninstall xvn data" (most destructive, with a slightly thicker 2px cinnabar left
bar):
- Title in Cormorant 18px, subtitle "Removes ~/.xvn/ entirely — strategy bundles, DB,
  keys, logs, everything. The xvn binary is not touched. This action is not reversible."
- Ghost cinnabar "Wipe ~/.xvn…" button + "Type UNINSTALL-XVN to confirm".

Below the cards, a final footer line in dim 12px: "Operator: edkennedy · workstation:
mbp-fern.local · last destructive action: never".

Mood: respectful but unflinching. The cinnabar appears only on the left bars and the
button text — never on backgrounds or large fields. Typed-confirmation phrases hint at
the gravity without being theatrical.
```

---

### 14. Chat rail (fragment, expanded) — global chrome

```
UI fragment, not a full screen — render only a vertical right-side rail, 360px wide and
~900px tall (the height of a typical desktop window minus top-nav). Mode and palette per
the design system block above. Surface: panel surface (#1A2029 in dark, #ECE6D9 in
light) with a 1px slate border on the left edge and a subtle drop-shadow on dark mode
only (light mode rail sits flush, no shadow). The page beneath can be a faded blurred
surface to the left for context.

Header strip at the top of the rail (~100px tall):
- Title "xvn agent · ● Online" in primary with a small amber-gold dot.
- Below the title, a context chip in slate-darker pill: "Context: Run 01H8N7Z (eth-mr-
  v3, bull-q1-25)" with a small dropdown chevron at the right.
- Below the chip, a row of three small ghost icon buttons: "↻ start fresh", "🔍 search
  history", "≡ collapse rail" (which would shrink to 40px icon strip).

Chat scroll region (middle, ~600px tall):
- 5 message exchanges, alternating assistant on left (amber-gold left border, panel
  card, small amber-gold avatar dot) and user on right (tinted panel card aligned right).
- Most recent assistant message is mid-stream — has a small amber-gold pulsing cursor at
  the end of its last word, indicating typing.
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
  attach a chart, attach a finding, etc.) and a right-aligned primary amber-gold "Send"
  button with "⌘↵" hint.
- Footer row in dim mono: "Tokens this session: 4,820" on the left and "Open in /setup
  ↗" link on the right.

Mood: ambient AI presence — feels like a co-pilot that's always there but never
demanding. The amber-gold accents (avatar dot, send button, action chips, pulsing
cursor) draw the eye to where the AI is actually doing something, while the panel body
recedes.

Aspect: tall and narrow. No top nav, no other panels — only the rail.
```

---

### 15. Command palette (modal overlay) — global chrome (⌘K)

```
UI fragment — render a centered modal overlay on a dimmed (~50% darkened in dark mode,
~25% darkened in light mode) page background. The modal is 640px wide and ~520px tall,
mode and palette per the design system block above. Elevated surface with a 1px slate
border and a soft drop-shadow.

Top of the modal: a wide search input (~50px tall) with a left-anchored "⌘K" pill in
dim mono and a placeholder "Jump to a strategy, run, finding, or scenario…" in dim
text. The input has focus (visible cursor at the start, faint amber-gold outline ring).

Below the search input, a thin slate divider, then a scrollable result list grouped by
category. Each group has a small uppercase dim-text header followed by 1–3 result rows.
Each result row is ~40px tall and has: a category-appropriate icon on the left (Lucide
thin-stroke), the result name in primary, a secondary metadata line in dim ("Strategy ·
Catches uptrends · Eval-ready"), and a right-aligned "↵" key hint glyph in dim.

Visible groups (in this order):

STRATEGIES (3 rows):
- "eth-mr-v3" with subtitle "Mean reversion · ETH · Eval-ready"
- "btc-momentum-v1" with subtitle "Momentum · BTC · Eval-ready"
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

## Out of v1 scope (not rendered in this file)

The following routes and archetypes are **not** part of v1 test and have no prompts in this file. The originals live in [`docs/design/gptprompts.md`](./gptprompts.md) — refer there when their plans are picked up.

| Route / archetype | Reason | Original prompt |
|---|---|---|
| `/live/<deployment_id>` Flight Deck cockpit | Plan 2c (durable scheduler + live daemon) deferred | gptprompts.md §5 |
| `/journal` Lab Notebook | Lab Notebook plan deferred (post-v1) | gptprompts.md §4 |
| `/marketplace` listings grid | Plan 5 (blockchain) deferred | gptprompts.md §16 |
| `/settings/marketplace` | Plan 5 deferred | gptprompts.md §18 |
| `/settings/autoresearch` | AR-1/2/3 deferred | gptprompts.md §17 |
| `/autoresearch/live` (evening cycle viewer) | AR-1/2/3 deferred | gptprompts.md §11 |
| `/autoresearch/lineage` (genealogy tree) | AR-1/2/3 deferred | gptprompts.md §12 |
| `/strategies/<bundle_hash>/diff` (mutation diff) | AR-1/2/3 deferred | gptprompts.md §13 |
| `/eval/mutator-ladder` | AR-1/2/3 deferred | gptprompts.md §14 |
| `/eval/ladder` (provenance) | AR-1/2/3 deferred | gptprompts.md §15 |
| Pass-Ribbon (ambient ticker footer) | deferred (post-v1 chrome) | gptprompts.md §22 |
| Canvas (spatial node graph) | deferred archetype | gptprompts.md §19 |
| Power Notebook (`/lab`) | deferred archetype | gptprompts.md §20 |
| Spreadsheet (parameter sweep) | deferred archetype | gptprompts.md §21 |
| Slot Machine starter spinner | deferred archetype | (slot-machine-design spec, no prompt yet) |

When any of these are picked up post-v1, lift their prompt from `gptprompts.md`, swap the design system header to point at this file's Folio Dark/Light blocks, and add the route to the table above.

---

*Document version: v1 · 2026-05-10. Lives at `docs/design/gptprompts-v1.md`. Cross-references: `v1-shipping-plan.md`, `docs/design/ui-elements.md`, `docs/design/themes.md`, `docs/design/gptprompts.md` (full library).*
