# XVN — 5 candidate themes (test set)

> **Companion to:** `themes.md` (canonical Folio + Stack), `gptprompts.md`
> (Folio prompt library), `quant1.png` / `folio1.png` (existing renders).
>
> **Status:** v0.1 · 2026-05-10. Five additional themes to A/B against the
> existing two. Each is intentionally distinct on at least two of: era,
> mode (light/dark), type system, color register, surface texture.
>
> **How to test:** open ChatGPT with GPT Image 2. For each theme, paste the
> "Shared Design System" block first, then the matched "Control Tower"
> prompt. The Control Tower archetype is the same one rendered in
> `folio1.png` and `quant1.png`, so the resulting images are directly
> comparable across all seven themes.

---

## The 5 directions at a glance

| | Pitch | Mode | Type system | Risk |
|---|---|---|---|---|
| **Atelier** | Swiss grid, geometric sans, monochrome + 1 cobalt | light | Akzidenz / Inter / JetBrains | low — reads as "yet another sans-fintech" if execution is sloppy |
| **Plate** | Trading dashboard as an FT market page | light (salmon paper) | Tiempos serif body + display | medium — could read as boomer-coded |
| **Lattice** | CAD blueprint with dimension lines on data | dark (cyan-on-graphite) | IBM Plex Mono everywhere | medium — aerospace-cosplay if shallow |
| **Onyx** | Brutalist black mode, asymmetric grid, neon accent | dark (near-void) | Söhne / Helvetica massive | high — easily reads as fashion not finance |
| **Risograph** | Duotone print, paper grain, complementary inks | light (cream paper) | Reckless display + Söhne | high — risks "Etsy / craft fair" |

Compared to the existing themes:
- Folio (warm dark + classical serif) — already covered
- Stack (HyperCard / System 7 pixel) — already covered
- Quant (Bloomberg phosphor) — already covered

The 5 below intentionally don't repeat any of those vocabularies.

---

# A. Atelier

**Pitch:** A trading dashboard built like a Müller-Brockmann poster. Off-white paper, geometric sans, ruthless 8px grid, monochrome surfaces, single cobalt accent. Quiet differentiation through typographic discipline alone.

**Reference family:** Müller-Brockmann posters, Lufthansa identity, Dieter Rams Braun, Linear at its most restrained, NYT graphics dept., Otl Aicher Munich '72.

**Risks:** "Looks like every other fintech sans-serif." Mitigation is execution craft (tight tracking, ruthless baseline grid, single accent never broken). The differentiator is the *grid hygiene*, not the colors.

## A.1 Shared Design System (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: Swiss International Typographic Style applied to 2026 fintech tooling.
Off-white paper, ruthless geometric grid, single chromatic accent. Inspirations: Josef
Müller-Brockmann posters, Otl Aicher's Munich '72 identity, Dieter Rams' Braun catalogues,
the New York Times graphics desk. Avoid skeuomorphism, avoid corporate stock illustration,
avoid drop shadows, avoid gradients, avoid cute icons, avoid emoji, avoid "AI generated"
gloss, avoid serifs anywhere.

Theme name: Atelier. The visual register hints at obsessive grid discipline and quiet
typographic confidence on a printed-paper ground.

Palette (LIGHT MODE primary — Atelier theme):
- Background: off-white paper #F7F6F3 (primary surface), warm-white #FFFFFF (elevated
  cards), oat-shade #EFEDE7 (nested panels). No tinted darks; surfaces stay near-paper.
- Borders: 1px ink hairline #111111, never gray. The hairline is the entire border
  language — no shadows, no rounded corners, 0px radius everywhere.
- Text: ink-black #111111 for primary (never pure #000000), graphite #555555 for
  secondary, light-graphite #999999 for tertiary metadata.
- Accent (single, never broken): cobalt #1A36F5 for primary buttons, links, status-good,
  equity-up. The cobalt is the entire identity — every other surface is paper or ink.
- Status: cobalt #1A36F5 for good/running/passed, ink-black #111111 + diagonal cross-hatch
  for warning (color is reserved for cobalt only — warnings use form, not color), red-ochre
  #B5483B for danger / equity-down only. Info uses cobalt at 50% opacity.
- Brand wordmark "xvn" rendered in Akzidenz-Grotesk Bold (or Inter Display tight-tracked
  to -2%), all lowercase, ink-black. No monogram, no symbol, no underline, just the wordmark.

Typography:
- Display / heading: Akzidenz-Grotesk Bold (fallbacks: Inter Display tight-tracked, then
  Helvetica Neue Bold). Sizes 24, 32, 48, 64px on a strict typographic scale. Tracking
  -1% to -2% on display sizes. Used for page titles, card titles, brand mark.
- Body / UI: Inter Regular. Sizes 13, 14, 16px ONLY (strict scale, no in-betweens).
  Tracking 0%. Used for labels, body copy, helper text.
- Numeric: JetBrains Mono Regular. Sizes 12, 14px ONLY. Tabular figures ON. Used for
  prices, hashes, IDs, percentages, all numeric data.

Components:
- Cards have 0px corner radius (square corners, always) and a 1px ink hairline border.
  No drop shadow ever. Elevation is implied by surface tone, not by light.
- Status indicators: small filled square (■) in 6×6px, plus a one-word label. Square not
  circle — the square reads honest to the grid. Color reserved for cobalt (good) and
  red-ochre (danger). Warnings use a hatched-cell pattern, not a color.
- Buttons: primary is cobalt fill #1A36F5 with paper text #F7F6F3, 0px radius, 1px cobalt
  border, 14px Akzidenz Bold all-lowercase. Secondary is paper fill with 1px ink border
  and ink text. Danger is paper fill with 1px red-ochre border and red-ochre text. No
  gradients, no shadows, no hover-bloom. Hover state: 1px border thickens to 2px.
- Tables: 8px row height multiples, 1px ink hairlines between rows AND columns (full
  grid, not just rows), monospace numerics right-aligned, headers in Akzidenz Bold 12px
  uppercase with -1% tracking.
- Sparklines and equity charts: 1px ink line, no fill, no gradient. Up-segments stay ink;
  down-segments are red-ochre. The single cobalt is reserved for the current value
  marker only — a 4×4 cobalt square at the chart's right edge.
- Icons: 1px geometric line, ruler-and-compass logic, 16×16 grid. Never illustrative,
  never thick-stroke. Lucide thin-line is acceptable; custom Aicher-style pictograms are
  better.
- Texture: NONE. No grain, no noise, no paper texture. The discipline IS the texture.
  Surfaces are flat #F7F6F3 paper.

Top nav (every authenticated route): "xvn" wordmark on the left in Akzidenz Bold ink,
then nav links "Home · Strategies · Eval · Live · Journal" in Inter 14px with 24px
inter-item spacing. Active route: 1px cobalt underline 2px below the baseline. Right
side: chat-rail toggle (a 16×16 square with two horizontal lines), daemon status
("■ connected" in Inter 12px uppercase tracked +5%), LLM provider ("■ anthropic"),
circular avatar 28×28px ink ring with initials.

Chat rail (every authenticated route except /setup): collapsible right-side rail,
360px wide expanded, 32px collapsed. Surface is paper #F7F6F3 with a 1px ink hairline on
the left edge (no gradient, no shadow). Header reads "xvn agent ■ online" in Inter 12px
uppercase tracked +5%.

Status pattern: filled 6×6 square (■) plus a one-word label IN UPPERCASE with +5% letter
tracking, never standalone color. Use "PASSED / MODIFIED / VETOED" for risk, "QUEUED /
RUNNING / COMPLETED / FAILED" for runs.

Density: high. The grid is the design — fill it. Information density should feel like a
Swiss railway timetable, not like a SaaS marketing page.

Voice in copy: plain English in lowercase. Page titles lowercase ("home", "strategies").
Button copy lowercase ("open setup agent", "new strategy"). The lowercase is the brand.
```

## A.2 Control Tower prompt (paste after the design system)

```
App screen of the xvn home page — a "what needs your attention" dashboard for a power
user who has been away for a day. LIGHT MODE Atelier theme, desktop 16:9. Chat rail
docked right, collapsed (32px ink-hairline strip).

Top nav: lowercase "xvn" wordmark in Akzidenz Bold ink, then nav "home" (active, 1px
cobalt underline), "strategies", "eval", "live", "journal". Right: chat toggle, "■
CONNECTED", "■ ANTHROPIC", circular ink-ring avatar.

Page header: title "home" in Akzidenz Bold 48px ink-black; subtitle in Inter 14px
graphite "2 deploys · 5 drafts · 12 runs since 18 hours ago". Right: a cobalt-fill
button "open setup agent" (lowercase, 0px radius), and a ghost button "new strategy"
(1px ink border, lowercase). Below: a ruled time-window row "TODAY · 7D · 30D · ALL"
in Inter 12px uppercase tracked +5%, "7D" active with a 1px cobalt underline.

Body: 8/12 + 4/12 grid with full 1px ink-hairline column rule between.

Left column:
1. Card "needs attention" with a small "3" counter in cobalt at the right of the title.
   Three rows separated by 1px ink hairlines:
   - Red-ochre ■ · "paper-eth-mr-v3 dropped −4.1% yesterday" · cobalt link "open ↗".
   - Hatched-square ■ · "regime fit mismatch (eth-mr-v3, scenario crypto-bear-q3-2024)"
     · cobalt link.
   - Cobalt ■ · "I noticed 3 of your last 5 evals failed in chop. draft a chop-aware
     variant?" with a cobalt "ask the agent →" link.
2. Card "live now" with cobalt counter "2". Two deployment rows: strategy name in Inter
   14px, "PAPER" pill (1px ink border, 12px uppercase), "■ RUNNING" status, P&L today
   in JetBrains Mono 14px (cobalt for + values, red-ochre for − values), 80px wide
   1px-line sparkline (no fill), and "open cockpit ↗" cobalt link.
3. Card "eval queue" with one row "queued: btc-momentum-v2 on chop-q2-25 · ETA ~3min"
   and a thin 2px-tall progress rail at 12% in cobalt.

Right column:
1. Card "recent findings" with cobalt "open journal →" top-right. Three rows: severity
   square · finding kind ("regime fit mismatch", "overtrading", "underperf in chop") ·
   truncated summary in Inter 13px graphite · strategy chip (1px ink border) · ghost
   mini-button "draft variant from this →".
2. Card "in your workshop". Three draft rows: name · template chip ("catches uptrends",
   "buys dips", "trades the range") · status pill ("EVAL-READY" 1px cobalt border,
   "DRAFT" 1px ink, "VALIDATING" 1px hatched) · cobalt "open in inspector →" link.
3. Footer line: "you've forked btc-momentum 4 times this week — see lineage →" in Inter
   13px graphite, with a "COMING SOON" pill (1px ink, uppercase 11px).

Mood: a railway timetable for a trading agent. Calm, scannable, ruthlessly aligned.
Cobalt only ever appears on the actions you should take. Everything else is ink on
paper.
```

---

# B. Plate

**Pitch:** Trading dashboard rendered as a Financial Times market page. Salmon-oat paper, Tiempos serifs *for body and headings*, classical newspaper rules and gutters, bottle-green for up and brick-red for down. The dashboard equivalent of a heritage financial publication.

**Reference family:** Financial Times print edition, Wall Street Journal hedcuts, The Economist briefing pages, Bloomberg Businessweek interior pages, Stripe Press hardbacks, Robert Hunter editorial illustration.

**Risks:** Reads as "boomer-coded" / "pre-internet." Younger users may find it stiff. Mitigation: editorial-modern execution (Tiempos not Caslon-revival; layout discipline of *contemporary* FT, not 1950s broadsheet).

## B.1 Shared Design System (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: a heritage financial newspaper rendered as a 2026 trading dashboard.
Salmon-oat paper, transitional serifs throughout, double-rule column dividers, classical
newspaper hierarchy. Inspirations: Financial Times print edition (the salmon paper, the
hedcut illustrations, the column rules), Bloomberg Businessweek interior pages, The
Economist briefing pages, Stripe Press hardback book interiors. Avoid skeuomorphism,
avoid stock photography, avoid gradients, avoid neon, avoid emoji, avoid sans-serif body
type, avoid the "tech bro fintech" register.

Theme name: Plate. The visual register reads as if a 130-year-old financial paper of
record decided to ship a trading product.

Palette (LIGHT MODE primary — Plate theme):
- Background: salmon-oat paper #FBF0DC (primary surface — the FT salmon, slightly oat),
  warm-pink #FDF6E8 (elevated cards), buff #F4E8D0 (nested panels). Surfaces are warm,
  paper-tinted, never blue-tinted.
- Borders: 1px ink-brown #1F1611, often DOUBLE-RULED (two 0.5px ink lines, 2px gap)
  between major regions and columns — the newspaper-page rule. 0px corner radius.
- Text: ink-brown #1F1611 for primary (never pure black), warm-graphite #6B5D4E for
  secondary, dim-buff #9A8A75 for tertiary metadata.
- Accent: bottle-green #154734 for primary buttons, links, equity-up, good states. Brick
  #B22222 for danger, equity-down, vetoed. Single ink-blue #1B3A6B for info / queued /
  links. Three accents only — every color in the system is one of these.
- Brand wordmark "XVN" rendered in Tiempos Headline Bold UPPERCASE, ink-brown. (Note:
  uppercase here, not lowercase — the heritage-press register justifies it.) No monogram.

Typography:
- Display / heading: Tiempos Headline (or Source Serif Pro Display) in Bold and Regular.
  Sizes 28, 36, 48, 64px. Tight tracking 0%. Used for page titles, card titles, brand mark.
- Body / UI: Tiempos Text (or Source Serif Pro Text) in Regular and Italic — *body in
  serif*, this is the deliberate departure. Sizes 14, 16px. Used for labels, body copy,
  helper text. Italic permitted for chart annotations and bylines.
- Numeric: JetBrains Mono Regular. Sizes 12, 14px. Tabular figures ON. Used for prices,
  hashes, IDs, percentages — every numeric value.

Components:
- Cards have 0px corner radius and either NO border (relying on 1px hairlines between
  sections) or a single 1px ink-brown rule at top and bottom (newspaper-section style),
  never four-sided. Elevation by paper tone only.
- Section dividers: DOUBLE RULE — two 0.5px ink-brown lines with a 2px gap between.
  This is the dominant border language; use it between every major region.
- Status indicators: small bullet (●) plus a one-word label IN SMALL CAPS (Tiempos Text
  Bold 11px, +10% tracking, true small-caps not synthesized). "● PASSED" in bottle-green,
  "● VETOED" in brick, "● QUEUED" in ink-blue.
- Buttons: primary is bottle-green fill #154734 with salmon text #FBF0DC, 0px radius,
  Tiempos Text Bold 14px sentence case. Secondary is salmon fill with 1px ink-brown
  border and ink-brown text. Danger is salmon fill with 1px brick border and brick text.
  No gradients, no shadows.
- Tables: newspaper market-page style. Tabular numerics right-aligned in JetBrains Mono.
  1px ink-brown rule above the header, 1px below, 1px below the last row. NO row
  separators between data rows (the alternation comes from row-padding, not lines).
  Column headers in Tiempos Text Bold 11px small-caps.
- Sparklines and equity charts: 1.5px line, no fill, bottle-green for positive segments
  and brick for negative. Charts get a 1px hairline frame (no gradient, no axis chrome).
  Always include a small "PRICE: USD" italic Tiempos label in the corner — like a chart
  caption in a newspaper.
- Icons: thin-line 1px stroke, slightly classical (a heraldic-ish star, an FT-style
  pictogram). NO Lucide. The icon set should look like FT briefing-page glyphs.
- Hedcuts: where avatars appear, render as black-line stippled hedcut portraits in the
  WSJ style — a 28×28 ink-brown stipple drawing on salmon. (For seeded users, use
  initials in Tiempos Bold.)

Top nav: "XVN" wordmark in Tiempos Headline Bold ink-brown on the left, then a horizontal
double-rule, then nav "Home · Strategies · Eval · Live · Journal" in Tiempos Text Bold
14px small-caps with 24px spacing. Active route: bottle-green underneath. Right side:
chat-rail toggle, daemon status "● Connected", LLM "● Anthropic", hedcut avatar.

Chat rail: 360px expanded, 40px collapsed. Surface is buff #F4E8D0 with a 1px ink-brown
left rule and a top double-rule. Header in Tiempos Bold 14px small-caps "XVN AGENT ·
ONLINE". Composer placeholder is in Tiempos Italic.

Status pattern: filled bullet (●) plus a small-caps one-word label.

Density: high. Newspaper density. Information per inch should match the FT markets page,
not a SaaS landing page.

Voice in copy: editorial-formal, like newspaper section headers. Page titles in Title
Case ("Home", "Strategies"). Button copy in sentence case ("Open setup agent"). Status
labels in small-caps. Allow short Latin-derived terms ("ad interim", "pro tem.") for
status-strip annotations only.
```

## B.2 Control Tower prompt (paste after the design system)

```
App screen of the xvn home page — a market-page trading dashboard for a power user.
LIGHT MODE Plate theme, desktop 16:9. Chat rail docked right, collapsed (40px buff
strip with double-rule left edge).

Top nav: "XVN" wordmark in Tiempos Headline Bold ink-brown left, horizontal double-rule
under, nav in Tiempos small-caps Bold "HOME" (active, bottle-green underline),
"STRATEGIES", "EVAL", "LIVE", "JOURNAL". Right: chat toggle, "● CONNECTED",
"● ANTHROPIC", a 28×28 hedcut-stipple avatar.

Page masthead: title "Home" in Tiempos Headline Bold 64px ink-brown, with an italic
Tiempos byline below in warm-graphite "2 deploys · 5 drafts · 12 runs since you were
last here · 18 hours ago". Right: a bottle-green-fill primary "Open setup agent" and a
ghost "New strategy". Beneath: a small-caps time-window row "TODAY · 7D · 30D · ALL",
"7D" active with a bottle-green underline.

Below the masthead: a horizontal double-rule, then a 8/12 + 4/12 layout with a
DOUBLE-RULE column gutter between.

Left column ("OPERATIONAL" — small-caps Tiempos Bold section header):
1. "Needs Attention" card. Section title in Tiempos Headline Bold 22px, with a Tiempos
   Italic "(3)" inline. Three rows separated by 1px ink hairlines, each row has:
   severity bullet (● brick / ● ink-blue / ● bottle-green) · headline in Tiempos Text
   Bold 16px ("paper-eth-mr-v3 dropped −4.1% yesterday") · italic Tiempos byline ·
   ink-blue "Open ↗" link.
2. "Live Now" card with "(2)" italic inline. Two rows: strategy name in Tiempos Bold,
   "PAPER" small-caps chip, "● RUNNING" small-caps in bottle-green, P&L today in
   JetBrains Mono ("+0.62%" bottle-green or "−0.41%" brick), an 80px sparkline (no fill,
   bottle-green/brick segments), and an ink-blue "Open cockpit ↗" link.
3. "Eval Queue" card. One row "Queued: btc-momentum-v2 on chop-q2-25 · ETA ~3min" with
   a thin 2px-tall progress rail at 12% in ink-blue.

Right column ("KNOWLEDGE & CYCLE RE-ENTRY" — small-caps section header):
1. "Recent Findings" with ink-blue "Open journal →" top-right. Three rows: bullet ·
   finding kind in small-caps Tiempos Bold ("REGIME FIT MISMATCH", "OVERTRADING",
   "UNDERPERF IN CHOP") · summary in Tiempos Text · strategy chip · ghost "Draft
   variant from this →".
2. "In Your Workshop". Three draft rows: name in Tiempos Bold · template chip · status
   small-caps pill ("EVAL-READY" bottle-green outline / "DRAFT" ink outline / "VALIDATING"
   ink-blue outline) · ink-blue "Open in Inspector →".
3. Footer italic line in dim-buff: "You've forked btc-momentum 4 times this week — see
   lineage →" with a small-caps "COMING SOON" pill.

Mood: an FT briefing page that happens to drive a trading bot. Calm, dignified,
information-dense. The bottle-green and brick do all the directional work; the salmon
paper does all the warmth.
```

---

# C. Lattice

**Pitch:** Trading workbench as a CAD blueprint. Graphite ground, technical-cyan strokes, dimension lines and callout brackets on data values, isometric grid background. Engineering-drawing register applied to financial data.

**Reference family:** Apollo flight manuals, Boeing 737 cockpit checklists, AutoCAD legacy interface, MIT Media Lab posters, MakerBot/Shapeways technical view, the Lockheed Skunk Works visual identity, Field Notes engineering edition.

**Risks:** "Aerospace cosplay" if shallow. The differentiator must be the actual *use* of dimension brackets, leader lines, and section callouts on real data — not just cyan-on-graphite as decoration.

## C.1 Shared Design System (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: a CAD blueprint or engineering technical drawing rendered as a 2026
trading dashboard. Graphite ground with technical-cyan rules, leader lines, dimension
brackets, callout flags, isometric reference grid. Inspirations: Apollo flight manuals,
Boeing 737 cockpit checklists, AutoCAD legacy interface, the Lockheed Skunk Works
identity, Field Notes engineering edition, MIT Media Lab posters. Avoid skeuomorphism,
avoid stock photography, avoid neon "cyberpunk" gloss (this is engineering, not gaming),
avoid emoji, avoid round corners.

Theme name: Lattice. The visual register hints at a flight-manual or machine-shop
drawing translated into a live data surface.

Palette (DARK MODE primary — Lattice theme):
- Background: graphite #0B0F12 (primary surface), gunmetal #11171B (elevated cards),
  dark-graphite #161D22 (nested panels). Surfaces are cool, slightly blue-tinged but
  not navy.
- Background grid: at 1.5% opacity, a 32px isometric reference grid (three-axis: 30°,
  90°, 150°) in technical-cyan. Visible only as a faint scaffolding.
- Borders: 1px technical-cyan #00D9FF for primary frames; 1px graphite #2A3640 for
  secondary dividers. 0px radius everywhere — square corners are doctrine.
- Text: drawing-white #E6F1FF for primary (cool, slightly cyan-cast), cyan-graphite
  #8DA3B0 for secondary, dim-graphite #4A5560 for tertiary metadata.
- Accent: technical-cyan #00D9FF for primary actions, links, status-good, equity-up.
  Hazard-amber #FFB800 for warning. Magenta #FF2D7E for danger / failure / equity-down.
  Cyan-amber-magenta is the entire chromatic system.
- Brand wordmark "XVN" rendered in IBM Plex Mono Bold UPPERCASE, drawing-white. With a
  small 4×4 cyan square as a "datum point" to the left, like an engineering origin marker.

Typography:
- Display / heading: IBM Plex Mono Bold UPPERCASE. Sizes 14, 18, 24, 32px (tighter
  scale than other themes — engineering drawings don't have giant headers). Letter
  tracking +5% on headings. Used for page titles, card titles, section headers.
- Body / UI: IBM Plex Sans Regular. Sizes 12, 13, 14px. Used for descriptive prose only —
  engineering drawings have minimal prose, so body type is rare.
- Numeric / data: IBM Plex Mono Regular. Sizes 11, 12, 14px. Tabular ON. Used for prices,
  IDs, dimensions, ALL data values. Mono is the dominant register.

Components:
- Cards: 1px cyan border on TOP-LEFT, 1px cyan border on BOTTOM-RIGHT, leaving the other
  two edges open — like an engineering callout frame. Card titles always include a
  "section number" in IBM Plex Mono ("§01 NEEDS ATTENTION", "§02 LIVE NOW") in cyan-graphite.
- Dimension brackets: numeric values get an optional dimension-line treatment — a 1px
  cyan horizontal line above the number with small inward-facing 4px ticks at each end,
  like engineering dimension annotation. Use sparingly: only for the most important
  number on each card (e.g., portfolio P&L).
- Leader lines: callouts use a 1px cyan line bending at 90° from a numbered tag (a small
  cyan circle with a number inside, like "①") to the data point. Only on charts and
  critical alerts.
- Status indicators: small filled square (■) 8×8px in cyan / amber / magenta, plus a
  one-word label in IBM Plex Mono UPPERCASE +5% tracking. "■ NOMINAL" (cyan), "■ CAUTION"
  (amber), "■ FAULT" (magenta). Engineering language, not consumer language.
- Buttons: primary is graphite fill with 1px cyan border, cyan text in IBM Plex Mono
  Bold UPPERCASE. Hover: border thickens to 2px. Press: cyan fills the inside. Secondary:
  same but with cyan-graphite border. Danger: same but with magenta border and magenta
  text. 0px radius, never any shadow.
- Tables: 1px cyan-graphite hairlines for grid (full grid like graph paper, not just
  rows). Headers in IBM Plex Mono Bold UPPERCASE 11px +5% tracking. Numerics
  right-aligned. Row hover state: 1px cyan top and bottom rules appear on the active row.
- Sparklines and equity charts: 1.5px cyan line for up-segments, 1.5px magenta for
  down-segments, NO fill (engineering drawings don't have fills, they have lines). Plot
  points are 4×4 cyan squares at every data tick. Optional crosshair on hover: a 1px
  cyan vertical and horizontal line, with the value displayed in a small cyan rectangle
  at the right edge.
- Icons: 1px cyan-stroke geometric pictograms — a square wave, a circuit junction, a
  caliper, a wrench, a compass, a target. Always 16×16, integer pixel grid. Engineering
  symbol set, not Lucide.
- Texture: NONE on cards. The isometric grid in the background is the texture.

Top nav: "XVN" wordmark in IBM Plex Mono Bold with a cyan datum-square left of it. Nav
in IBM Plex Mono Bold UPPERCASE +5% tracking: "HOME · STRATEGIES · EVAL · LIVE ·
JOURNAL". Active route: 1px cyan underline. Right side: chat-rail toggle (a small
square-with-chevron), daemon "■ NOMINAL", LLM "■ ANTHROPIC", a square avatar (NOT round
— engineering drawings don't have round avatars) with initials in IBM Plex Mono.

Chat rail: 360px expanded, 40px collapsed. Surface is gunmetal with a 1px cyan left
border. Header in IBM Plex Mono Bold "XVN AGENT · ■ NOMINAL". Composer placeholder is
"INPUT QUERY…" in dim-graphite.

Status pattern: filled square (■) plus a one-word UPPERCASE label in IBM Plex Mono.
Use engineering vocabulary: "NOMINAL / CAUTION / FAULT / STANDBY / OFFLINE".

Density: very high. Engineering drawings pack maximum information per area. Aim for
flight-manual density.

Voice in copy: technical-engineering. Page titles in UPPERCASE with section numbers
("§01 HOME"). Button copy in UPPERCASE ("OPEN AGENT", "NEW STRATEGY"). Numeric values
should include units when meaningful ("18.3 H" for hours, "+1142.30 USD"). Allow brief
engineering shorthand ("OPS", "P&L", "QTY").
```

## C.2 Control Tower prompt (paste after the design system)

```
App screen of the xvn home page — a flight-manual-style trading dashboard for a power
user. DARK MODE Lattice theme, desktop 16:9. Chat rail docked right, collapsed (40px
gunmetal strip with cyan left border).

Background: graphite with a 1.5%-opacity 32px isometric grid (visible only as faint
scaffolding).

Top nav: a small cyan datum-square, then "XVN" wordmark in IBM Plex Mono Bold
drawing-white, then nav UPPERCASE "HOME" (active, 1px cyan underline), "STRATEGIES",
"EVAL", "LIVE", "JOURNAL". Right: chat toggle, "■ NOMINAL", "■ ANTHROPIC", a 28×28
square avatar with mono initials.

Page header: section number "§01" in cyan-graphite IBM Plex Mono, then title "HOME"
in IBM Plex Mono Bold UPPERCASE 32px drawing-white. Below, a single-line technical
log: "DEPLOYS=2 · DRAFTS=5 · RUNS=12 · LAST_SEEN=18.0H AGO" in IBM Plex Mono 12px
cyan-graphite. Right: a primary "OPEN SETUP AGENT" button (graphite fill, 1px cyan
border, cyan text) and a secondary "NEW STRATEGY". Below: a time-window row
"WIN: TODAY | 7D | 30D | ALL" with "7D" inside a 1px cyan box.

Body: 8/12 + 4/12 grid with 1px cyan-graphite vertical rule between columns.

Left column:
1. Card "§02 NEEDS ATTENTION" with a small "[03]" count in cyan-graphite at the right.
   Top-left and bottom-right corners drawn with cyan; other two corners open. Three
   rows, each separated by a 1px cyan-graphite hairline:
   - Magenta ■ · "PAPER-ETH-MR-V3 ▼ −4.10% (24H)" with a leader-line "①" callout that
     points to the value · cyan link "OPEN ↗".
   - Amber ■ · "REGIME FIT MISMATCH (eth-mr-v3 / crypto-bear-q3-2024)" · cyan link.
   - Cyan ■ · "OBS: 3/5 LAST EVALS FAILED IN CHOP. DRAFT CHOP-AWARE VARIANT?" with a
     cyan "ASK AGENT →" link.
2. Card "§03 LIVE NOW" with count "[02]". Two rows: strategy name in IBM Plex Mono ·
   "PAPER" cyan-outline pill · "■ NOMINAL" status · P&L in IBM Plex Mono with a small
   dimension-bracket above the value ("+0.62%" cyan or "−0.41%" magenta) · 80px IBM
   Plex Mono sparkline (line only, cyan/magenta segments, 4×4 plot squares) · cyan
   "OPEN COCKPIT ↗".
3. Card "§04 EVAL QUEUE". One row "QUEUED: BTC-MOMENTUM-V2 / CHOP-Q2-25 / ETA 3.0M"
   with a thin progress rail at 12% in cyan, ticks every 25%.

Right column:
1. Card "§05 RECENT FINDINGS" with cyan "OPEN JOURNAL →". Three rows: severity ■ ·
   finding kind in IBM Plex Mono Bold UPPERCASE ("REGIME FIT MISMATCH", "OVERTRADING",
   "UNDERPERF IN CHOP") · summary in IBM Plex Sans 13px · strategy chip · ghost button
   "DRAFT VARIANT FROM THIS →".
2. Card "§06 IN YOUR WORKSHOP". Three draft rows: name · template chip · status
   ("■ EVAL-READY" cyan, "■ DRAFT" cyan-graphite, "■ VALIDATING" amber) · cyan
   "OPEN IN INSPECTOR →".
3. Footer line in dim-graphite IBM Plex Mono: "FORKED BTC-MOMENTUM x4 THIS WEEK — SEE
   LINEAGE →" with a "[ COMING SOON ]" cyan-graphite tag.

Mood: a Lockheed Skunk Works flight-checklist that happens to manage trading agents.
The dimension brackets and section numbers make every value feel measured and
load-bearing.
```

---

# D. Onyx

**Pitch:** Brutalist anti-design black mode. Massive Helvetica/Söhne display set in 96-240px, asymmetric grid, single neon accent (electric chartreuse OR hot magenta — pick one), no borders, no shadows, deliberate ugly-on-purpose. The trading dashboard equivalent of a Balenciaga product page or a couture investor deck.

**Reference family:** Balenciaga.com, Vetements lookbooks, MSCHF, late-90s deconstructionist David Carson, Ray-Ban current campaign sites, Off-White, Vasjen Katro's "Baugasm" series.

**Risks:** Highest LARP risk of the five. Reads as "fashion not finance" if the data discipline doesn't hold up underneath. Differentiator: the *grid itself* must be the design move — asymmetric placement, oversized type cropped at the edges, but every data value still legible and right.

## D.1 Shared Design System (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: brutalist anti-design 2026. Near-void black ground, oversized
all-caps Helvetica/Söhne display set massive (96-240px), asymmetric grid, single neon
accent. The trading dashboard equivalent of a couture brand investor page or a
Balenciaga product surface. Inspirations: Balenciaga.com, Vetements lookbooks, MSCHF,
Off-White, late David Carson, Vasjen Katro's "Baugasm". Avoid skeuomorphism, avoid
gradients, avoid shadows, avoid emoji, avoid Lucide-style icons, avoid round corners,
avoid centered-symmetric layouts (asymmetry is the design).

Theme name: Onyx. The register reads "luxury-brand product page that happens to be a
trading dashboard."

Palette (DARK MODE primary — Onyx theme):
- Background: void #050505 (primary surface) — never pure #000, slightly off so it
  reads as "ink" not "absence". No second elevated surface; everything floats on the
  void. Optional muted ground #0A0A0A only for very large nested regions.
- Borders: NONE by default. Whitespace and asymmetric grid placement do all the work.
  Where a border is required (data tables), 1px paper-white #FAFAFA hairlines, 0px
  radius.
- Text: paper-white #FAFAFA for primary (never gray), graphite #888888 for secondary,
  dim-graphite #444444 for tertiary metadata.
- Accent (single, choose ONE for the whole product — do not mix): electric-chartreuse
  #C6FF00 OR hot-magenta #FF1FB1. Default to electric-chartreuse for trading apps (the
  green register reads as "live / active / pulse"). The accent is used for primary
  actions, equity-up, status-good, attention-flags. Equity-down is just paper-white
  (the *absence* of accent reads as down).
- Status: chartreuse for good states, paper-white for neutral/down states, no separate
  "warning" or "danger" colors — escalation comes from text size and weight, not from
  hue. (This is the brutalist move.)
- Brand wordmark "XVN" rendered in Söhne Breit Bold (or Helvetica Neue Bold) UPPERCASE
  at 240px or larger, often cropped at the edge of the viewport. The wordmark IS the
  layout in many cases.

Typography:
- Display / heading: Söhne Breit Bold or Helvetica Neue Bold UPPERCASE. Sizes 96, 144,
  192, 240px (huge). Tracking -3% to -5% — TIGHT. Headings are often cropped at the
  page edge or broken across two lines mid-word. Used for page titles, section markers,
  brand mark. The display type is the visual hierarchy.
- Subhead: Söhne Halbfett or Helvetica Neue Medium UPPERCASE. Sizes 16, 24, 32px.
  Tracking +5% — wide spacing on small caps as counterpoint to tight display.
- Body / UI: Söhne Buch or Helvetica Neue Regular. Sizes 13, 14px. Lowercase. Used for
  labels, body copy, helper text.
- Numeric: Söhne Mono OR plain Söhne tabular figures (NOT JetBrains Mono — too
  programmer-coded for this register). Sizes 12, 14px. Tabular ON.

Components:
- Cards: NO BORDER, NO BACKGROUND. Cards are defined entirely by whitespace and
  asymmetric placement. A "card" is a region of related content surrounded by 48-96px
  whitespace from neighbors. Spacing is the structural element.
- Status indicators: a 12×12 chartreuse square (■) before status-good labels.
  Status-bad / down states have NO marker (the absence of chartreuse is the marker).
  Labels in Söhne Halbfett UPPERCASE +5% tracking, 12px.
- Buttons: primary is paper-white fill #FAFAFA with void text #050505, 0px radius,
  Söhne Halbfett UPPERCASE 14px +5% tracking. Hover: invert to chartreuse fill with
  void text. NO border, NO shadow. Secondary: void fill with 1px paper-white border,
  paper-white text. Danger: void fill with paper-white text and a 1px paper-white
  border, with the word "DELETE" or "STOP" in a slightly larger size (the size IS the
  warning). NO red. NO color-coded danger.
- Tables: 1px paper-white hairlines on top and bottom of the table only. NO row
  separators, NO column separators. Numerics right-aligned, lots of row padding (24px
  vertical). Headers in Söhne Halbfett UPPERCASE +5% tracking.
- Sparklines and equity charts: 2px paper-white line for the entire chart (no color
  segmentation). Up-direction is implied by the line going up. The current-value
  marker is a small chartreuse square at the right end. NO fill, NO axis chrome, NO
  grid.
- Icons: NONE by default. Where unavoidable, use plain UTF-8 glyphs (▲ ▼ ✕ → ↗) in
  paper-white at body-text size. NO illustrative iconography. The lack of icons is
  the design.
- Texture: NONE. Pure flat fields.

Top nav: "XVN" wordmark in Söhne Breit Bold UPPERCASE at 32px paper-white on the
LEFT EDGE (no padding from the viewport edge — the type touches the edge). Nav far
right: "HOME / STRATEGIES / EVAL / LIVE / JOURNAL" in Söhne Halbfett UPPERCASE 12px
+10% tracking, separated by " / " characters in graphite. Active route: chartreuse
text. NO underlines. Far right of nav: chat toggle, "CONNECTED" in 11px tracked +10%,
"ANTHROPIC" similar, a square 24×24 paper-white-stroked avatar with initials in mono.

Chat rail: 360px expanded, 24px collapsed (a thin paper-white vertical hairline, no
icon). When expanded, the surface is void with a 1px paper-white left hairline.
Header is "XVN AGENT — ONLINE" in Söhne Halbfett UPPERCASE 11px +10% tracking.

Status pattern: chartreuse 12×12 square (■) for good states, no marker for neutral or
bad, label in Söhne Halbfett UPPERCASE +5% tracking.

Density: deliberately MEDIUM-LOW. Onyx is anti-information-density — the brutalist
register requires breathing room. 50% of the screen should be void.

Voice in copy: imperative, terse, all-caps for high-rank items. Page titles in
massive UPPERCASE ("HOME", "STRATEGIES"). Button copy UPPERCASE ("OPEN AGENT", "NEW
STRATEGY"). Body text in lowercase sentence case. Allow couture-flavored phrases
("RUNTIME // 18H", "QUEUED // BTC-MOMENTUM-V2") — the slash-double is the
fashion-house separator.
```

## D.2 Control Tower prompt (paste after the design system)

```
App screen of the xvn home page — a brutalist couture trading dashboard. DARK MODE
Onyx theme, desktop 16:9. Chat rail docked right, collapsed (24px paper-white
hairline only).

Top nav: "XVN" in Söhne Breit Bold UPPERCASE 32px paper-white, FLUSH against the
viewport's LEFT edge (no padding). Nav far-right in Söhne Halbfett 12px UPPERCASE
+10% tracking, separated by " / ": "HOME" (chartreuse, active) / "STRATEGIES" /
"EVAL" / "LIVE" / "JOURNAL". Then far right: chat toggle (a 16×16 chartreuse
square), "CONNECTED" 11px, "ANTHROPIC" 11px, square avatar.

Page hero: title "HOME" set MASSIVE in Söhne Breit Bold UPPERCASE 240px paper-white,
LEFT-ALIGNED and CROPPED at the right viewport edge — only the "HOM" is fully
visible, the "E" is half-cropped off the page. Subtitle below in Söhne Halbfett 14px
graphite UPPERCASE +5%: "DEPLOYS // 02   DRAFTS // 05   RUNS // 12   LAST SEEN //
18H AGO". Right side of the hero (above the cropped "E"): a paper-white-fill primary
button "OPEN SETUP AGENT" and a ghost button "NEW STRATEGY".

Below, after 96px of whitespace: a time-window row "TODAY / 7D / 30D / ALL" in Söhne
Halbfett UPPERCASE 12px +10% tracking, with "7D" highlighted in chartreuse.

Body: ASYMMETRIC 7/12 + 5/12 grid with 96px gutter (no rules, just whitespace).

Left column:
1. Section marker "01 // NEEDS ATTENTION" in Söhne Halbfett UPPERCASE 16px graphite
   +5% tracking. Then a count number "03" in Söhne Breit Bold 96px chartreuse.
   Below, three rows separated by 32px whitespace (no hairlines):
   - "PAPER-ETH-MR-V3 ▼ −4.10%" in Söhne Halbfett UPPERCASE 18px paper-white, with a
     small graphite byline below ("24H DROP") and "OPEN ↗" in chartreuse 14px.
   - "REGIME FIT MISMATCH" similarly, with "ETH-MR-V3 / CRYPTO-BEAR-Q3-2024" byline.
   - "■ 3 OF 5 LAST EVALS FAILED IN CHOP" with the chartreuse square marker, byline
     "DRAFT CHOP-AWARE VARIANT?" and chartreuse "ASK AGENT →" link.
2. Section marker "02 // LIVE NOW" with chartreuse "02" Söhne Breit Bold 96px.
   Two deployment rows, each with: strategy name in Söhne Halbfett UPPERCASE 24px
   paper-white, "PAPER" pill in 11px UPPERCASE +10% (1px paper-white border, 0px
   radius), "■ RUNNING" in 11px chartreuse, P&L in Söhne tabular 24px paper-white
   ("+0.62%" — note: NOT colored chartreuse, just paper-white; the going-up line in
   the sparkline is the directional cue), an 80px line-only sparkline ending in a
   small chartreuse marker, and "OPEN COCKPIT ↗" link in chartreuse 12px.
3. Section "03 // EVAL QUEUE": "QUEUED // BTC-MOMENTUM-V2 // CHOP-Q2-25 // ETA 3M"
   in Söhne Halbfett UPPERCASE 16px paper-white, with a 2px-tall progress rail at
   12% in chartreuse below.

Right column:
1. Section "04 // FINDINGS" with chartreuse count "12" Söhne Breit Bold 96px. Three
   rows in 32px-spaced whitespace:
   - "REGIME FIT MISMATCH" 16px UPPERCASE paper-white, byline graphite, strategy
     chip with 1px paper-white border, "DRAFT VARIANT →" chartreuse link.
   - "OVERTRADING" similarly.
   - "UNDERPERF IN CHOP" similarly.
2. Section "05 // WORKSHOP" with chartreuse count "03" 96px. Three draft rows:
   name UPPERCASE Söhne Halbfett 18px · template chip · status pill ("EVAL-READY"
   chartreuse outline / "DRAFT" paper-white outline / "VALIDATING" paper-white
   outline) · "OPEN IN INSPECTOR →" chartreuse link.
3. Footer line: "FORKED BTC-MOMENTUM x4 THIS WEEK — SEE LINEAGE →" in graphite 14px
   UPPERCASE +5%, with a "// COMING SOON //" tag.

Mood: a Balenciaga product page that ships trading bots. 50%+ of the screen is void
black. The chartreuse only appears where you should look. Every action has so much
whitespace around it that it reads as deliberate, not crowded.
```

---

# E. Risograph

**Pitch:** Trading dashboard rendered as a Risograph print. Cream paper ground, two-color duotone (riso-blue + fluoro-pink), screen-printing misregistration as deliberate texture, slightly off-grid letterpress feel. Editorial-craft sensibility on real financial data.

**Reference family:** Risograph prints, Mike Mills posters, late-90s indie publishing (Index magazine, ANP Quarterly), Aesop branding, Hereford Lithographs, Studio Manuel Raeder.

**Risks:** Highest "looks unprofessional" risk — Etsy/craft-fair adjacency. Mitigation: structured grid and real data density underneath; only the ink and paper reads as craft. The data layer is as disciplined as Atelier.

## E.1 Shared Design System (paste once per session)

```
You are designing screens for "xvn" — a desktop web dashboard for an AI crypto trading
agent platform aimed at builders and traders. Every screen below uses this design system;
hold these rules constant across the whole family so the screens look like one product.

Aesthetic and era: a Risograph-printed editorial poster rendered as a 2026 trading
dashboard. Cream paper ground, two-color duotone (riso-blue + fluoro-pink) with
overprint-purple as the third color, screen-printing misregistration as a 1px texture,
paper grain throughout. Inspirations: Risograph prints from Hato Press / Risolve /
Stanley's Post, Mike Mills posters, Index magazine interior pages, Aesop branding,
Studio Manuel Raeder, Hereford Lithographs. Avoid skeuomorphism, avoid gradients
(use overprint instead), avoid stock photography, avoid emoji, avoid 100%-flat-vector
look (the texture IS the brand).

Theme name: Risograph. The register reads "limited-edition art print that happens to
manage your trading bots."

Palette (LIGHT MODE primary — Risograph theme):
- Background: paper-cream #F4EFE6 (primary surface), warm-cream #F8F4ED (elevated
  cards, very subtle), oat #ECE6D9 (nested panels). All surfaces have a 2-3% paper
  grain noise overlay — visible at all zoom levels.
- "Inks": only THREE colors exist, mixed via overprint:
  - riso-blue #2A4FBF
  - riso-fluoro-pink #FF36A0
  - overprint-purple #6A2780 (where blue and pink overlap)
- Borders: 1px riso-blue, 0px corner radius. Some borders intentionally MISREGISTER —
  the blue line is offset 1-2px from where it "should" be, like a real screen-print
  registration error. Don't do this on every border (looks broken); do it on accents.
- Text: ink-warm-black #1C1814 for primary (warm off-black, not pure), warm-graphite
  #5E5345 for secondary, dim-buff #9C8E78 for tertiary metadata.
- Accent: riso-blue #2A4FBF for primary actions, links, equity-up, status-good.
  Fluoro-pink #FF36A0 for danger, equity-down, vetoed, attention. Overprint-purple
  #6A2780 for neutral / queued / "in process" states.
- Brand wordmark "xvn" rendered in Reckless Bold (or GT Sectra Bold) — modern editorial
  serif with quirk — all lowercase, ink-warm-black. Optional: the wordmark itself
  prints in fluoro-pink with a 1.5px riso-blue offset behind it (deliberate
  misregistration as identity).

Typography:
- Display / heading: Reckless (or GT Sectra Display). Sizes 24, 32, 48, 64px. Italic
  permitted on subtitles and chart annotations. Used for page titles, card titles,
  brand mark. The slightly-quirky serifs (asymmetric stress, sharp terminals) are
  the editorial-modern feel.
- Body / UI: Söhne Buch (or GT America Regular). Sizes 13, 14, 16px. Used for labels,
  body copy, helper text. Sans-serif body (in contrast to Plate's serif body).
- Numeric: JetBrains Mono Regular. Sizes 12, 14px. Tabular ON.

Components:
- Cards: 1px riso-blue border, 4px corner radius (slightly soft — a hand-printed feel,
  not pill-shaped). Card backgrounds have 2-3% paper grain. Some cards have a
  deliberate 1.5px misregistration on the bottom-right corner (the blue line shifts
  down-right of where it should be) — use sparingly, like a press signature.
- Status indicators: small filled circle (●) plus a one-word lowercase-italic label in
  Reckless Italic 13px. "● running" in riso-blue, "● vetoed" in fluoro-pink, "● queued"
  in overprint-purple.
- Buttons: primary is riso-blue fill #2A4FBF with paper-cream text, 4px radius, Söhne
  Halbfett 14px sentence case. The button has a 1.5px fluoro-pink "shadow" offset
  down-right (overprint registration error as visual depth). Secondary: paper-cream
  fill with 1px riso-blue border, riso-blue text. Danger: paper-cream fill with 1px
  fluoro-pink border, fluoro-pink text. Hover: the offset "shadow" pulls in by 1px.
- Tables: 1px riso-blue hairlines for grid (full grid). Headers in Reckless Bold 12px
  italic. Numerics right-aligned in JetBrains Mono.
- Sparklines and equity charts: 1.5px line, riso-blue for up-segments, fluoro-pink
  for down-segments. NO fill, but optional 10%-opacity riso-blue (for up periods) or
  fluoro-pink (for drawdowns) blocks below the line — the half-transparency reads as
  "second ink layer over the first." Plot points: 4×4 squares at every tick.
- Icons: hand-drawn pictograms in 1.5px riso-blue stroke, slightly imperfect (a bell
  with a slightly off-center clapper, a magnifier with a hand-wobbled circle). 16×16
  on integer pixel grid but with 1px texture noise. Reference: Mike Mills' poster
  pictograms.
- Texture: 2-3% paper grain on all surfaces. Cards may include a subtle 5% riso-blue
  diagonal-line pattern in their background (4-line diagonal, 1px lines, 4px gap).
  The grain reads as "real paper under ink."

Top nav: "xvn" wordmark in Reckless Bold lowercase (with the optional pink-blue
misregistration on the wordmark only). Nav in Söhne Buch 14px lowercase: "home ·
strategies · eval · live · journal" with 24px spacing. Active route: 1px riso-blue
underline that misregisters by 1px (the underline is offset slightly from the
baseline). Right side: chat toggle, "● connected" daemon, "● anthropic" LLM,
circular avatar with 1px riso-blue stroke and initials in Söhne mono.

Chat rail: 360px expanded, 40px collapsed. Surface is warm-cream with a 1px riso-blue
left border and 2-3% paper grain. Header in Reckless Italic "xvn agent · online"
with the bullet in riso-blue.

Status pattern: filled circle (●) plus a lowercase-italic Reckless one-word label.
"running / passed / completed" in riso-blue, "warning / pending" in
overprint-purple, "vetoed / failed / danger" in fluoro-pink.

Density: medium-high. Editorial-page density. Less dense than Atelier or Plate, but
not airy like Onyx — leaves room for the texture to read.

Voice in copy: editorial-warm, lowercase, slightly literary. Page titles lowercase
italic ("home.", "strategies."). Button copy sentence case ("Open setup agent").
Allow short editorial phrases ("p.s. — the chop-aware variant is overdue.")
sparingly.
```

## E.2 Control Tower prompt (paste after the design system)

```
App screen of the xvn home page — a Risograph-printed trading dashboard. LIGHT MODE
Risograph theme, desktop 16:9. Chat rail docked right, collapsed (40px warm-cream
strip with 1px riso-blue left border).

Background: paper-cream #F4EFE6 with 2-3% paper grain noise, plus a subtle 5%
riso-blue 4-line diagonal pattern in the page margins.

Top nav: "xvn" wordmark in Reckless Bold lowercase, with the wordmark itself printed
in fluoro-pink and a 1.5px riso-blue offset behind it (deliberate misregistration
as identity). Nav lowercase Söhne 14px: "home" (active, 1px riso-blue underline
offset 1px below baseline), "strategies", "eval", "live", "journal". Right: chat
toggle, "● connected" with riso-blue dot, "● anthropic" with riso-blue dot,
circular avatar.

Page header: title "home." in Reckless Bold Italic 56px ink-warm-black (with the
period in fluoro-pink). Subtitle in Reckless Italic 16px warm-graphite "2 deploys ·
5 drafts · 12 runs since you were last here · 18 hours ago." Right: a riso-blue-fill
"Open setup agent" button (with the 1.5px fluoro-pink offset shadow), and a ghost
"New strategy" with a 1px riso-blue border. Below: a time-window row "today · 7d ·
30d · all" in lowercase Söhne 13px, "7d" inside a riso-blue 4px-radius pill.

Body: 8/12 + 4/12 grid with 1px riso-blue hairline between (with a 2px
misregistration shift halfway down — a deliberate "press error" detail).

Left column:
1. Card "needs attention." in Reckless Bold Italic 24px with a small fluoro-pink
   "(3)" inline. Card background has 2-3% grain plus subtle diagonals. Three rows
   separated by 1px riso-blue hairlines:
   - Fluoro-pink ● · "paper-eth-mr-v3 dropped −4.1% yesterday" with the percent value
     in JetBrains Mono fluoro-pink · "open ↗" link in riso-blue.
   - Overprint-purple ● · "regime fit mismatch (eth-mr-v3, scenario
     crypto-bear-q3-2024)" · riso-blue link.
   - Riso-blue ● · "p.s. — 3 of your last 5 evals failed in chop. draft a chop-aware
     variant?" with a riso-blue "ask the agent →" link.
2. Card "live now." with fluoro-pink "(2)". Two rows: strategy name in Reckless Bold
   Italic 18px · "paper" 4px-radius pill (1px overprint-purple border) · "● running"
   in riso-blue · P&L in JetBrains Mono ("+0.62%" riso-blue or "−0.41%" fluoro-pink) ·
   80px sparkline (riso-blue/fluoro-pink segments, 4×4 plot points, 10% riso-blue
   block fill below the line) · "open cockpit ↗" riso-blue link.
3. Card "eval queue." One row "queued: btc-momentum-v2 on chop-q2-25 · eta ~3min"
   with a 2px-tall progress rail at 12% in overprint-purple.

Right column:
1. Card "recent findings." with riso-blue "open journal →" top-right. Three rows:
   severity ● · finding kind in Reckless Bold Italic 16px ("regime fit mismatch",
   "overtrading", "underperf in chop") · summary in Söhne · strategy chip · ghost
   mini-button "draft variant from this →".
2. Card "in your workshop." Three draft rows: name in Reckless Bold Italic ·
   template chip ("catches uptrends", "buys dips", "trades the range") · status
   pill ("● eval-ready" riso-blue, "● draft" overprint-purple, "● validating"
   fluoro-pink) · riso-blue "open in inspector →" link.
3. Footer italic line in dim-buff: "p.s. — you've forked btc-momentum 4 times this
   week — see lineage →" with a small "coming soon" pill in overprint-purple
   outline.

Mood: a limited-edition risograph print of a financial workspace. The paper grain
and misregistration read as craft, but the data underneath is as disciplined as a
Bloomberg terminal. The fluoro-pink only appears on bad news; everything calm is
riso-blue.
```

---

## How to A/B test

Render each theme's Control Tower at 16:9, save as `theme-{name}1.png` next to
`folio1.png` and `quant1.png`. Side-by-side, the seven candidates should answer:

1. **Distinctiveness** — would a marketplace listing photo of this be unmistakable?
2. **Trust** — does this look like a tool serious people use to run real money?
3. **Density** — does the data scan at the speed a power user needs?
4. **Stickiness** — would you open it for the third time without effort?

The existing decision rule from `themes.md` (run Stack pilot first, fall back to
Folio) extends naturally: if Stack pilot fails, before defaulting to Folio, do a
half-day pilot of the top 1–2 themes from this set. Best candidates to slot in:
**Atelier** (lowest risk, comparable to Folio), **Lattice** (highest distinctiveness
without LARP risk), **Onyx** (highest distinctiveness if execution craft is there).

*Document version: 0.1 · 2026-05-10. Lives at
`/Users/edkennedy/Code/xvision/docs/design/themes-candidates.md`. Cross-references:
`themes.md`, `gptprompts.md`, `quant1.png`, `folio1.png`.*
