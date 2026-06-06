# Handoff · xvn operator documentation site

## Overview

A four-page operator documentation site for **xvn** (xvision) — non-custodial AI trading agents. The site is designed for two audiences in parallel: human operators (hackathon judges, first-100 users) and AI agents reading the repo. Every page is plain semantic HTML (no JS required to read), has a Markdown mirror at the same path, and carries a "Context for AI agents" front-matter block with the route, summary, key terms, and do-not list.

Pages:

| Page | Source mirror | Purpose |
|---|---|---|
| `index.html` | `index.md` | Overview, quickstart, "for agents", safety model |
| `manual.html` | `manual.md` | Operator manual — 4 tiers, 16 milestones, dashboard auth, observability |
| `cli.html` | `cli.md` | CLI reference — 16 verbs, A/B compare, remote CLI, exit codes |
| `architecture.html` | `architecture.md` | Pipeline, risk, eval framework, ERC-8004; inline SVG system diagram |

Plus machine-facing files at the root of `docs/`:

- `llms.txt` — short routing index for embedded agents
- `sitemap.json` — structured page index (id, route, summary, key terms, sub-sections)

## About the design files

The files in this bundle are **design references created in HTML** — working prototypes that show the intended look, behavior, and information architecture. They are **not** production code to ship as-is.

Your task is to **recreate these designs in the target codebase's existing environment** (Next.js + MDX, Astro, Docusaurus, VitePress, Nextra, Hugo, Mintlify, etc.) using its established patterns. If no static-site environment exists, prefer **Next.js App Router + MDX** or **Astro + Starlight** — both fit the structure cleanly and ship the Markdown mirrors and machine files naturally.

The Markdown mirrors (`*.md`) in this bundle are content-faithful and can be used directly as the source-of-truth content. The HTML files are the layout and design specification.

## Fidelity

**High-fidelity.** Final colors, typography, spacing, density, code-block treatment, and interaction behavior are all specified. Recreate pixel-accurately. The design system (colors, type scale, spacing, code styling, callouts, pills, tables, kv lists) is fully captured in `docs/docs.css` — port the same tokens into the target environment.

## Design system

### Colors

Two themes, persisted to `localStorage`. Dark is the default.

#### Dark (default)

| Token | Hex | Use |
|---|---|---|
| `--bg` | `#000000` | Page background |
| `--surface` | `#0A0A0A` | Card / panel surface |
| `--surface-2` | `#0E0E0E` | Hover, sub-surface |
| `--surface-side` | `#131109` | Sidebar background (unused at present — sidebar uses `--bg`) |
| `--surface-code` | `#0E0E0E` | Code-block background |
| `--inline-code-bg` | `rgba(0, 230, 118, 0.08)` | Inline code chip background |
| `--border` | `#1A1A1A` | Default border |
| `--border-strong` | `#2A2A2A` | Emphasised border |
| `--border-soft` | `#121212` | Hairlines / dividers |
| `--text` | `#FFFFFF` | Body text |
| `--text-2` | `#9CA3AF` | Secondary text |
| `--text-3` | `#5F6670` | Muted / metadata |
| `--text-4` | `#3A3F47` | Disabled / numerals |
| `--accent` | `#00E676` | Gold accent · brand |
| `--accent-soft` | `rgba(0, 230, 118, 0.14)` | Accent tinted background |
| `--accent-deep` | `#00B85F` | Deeper gold (hover, etc.) |
| `--link` | `#E2BC65` | Links |
| `--warn` | `#FFB020` | Warnings |
| `--danger` | `#FF4D4D` | Errors / destructive |
| `--info` | `#5FA8FF` | Info callout |
| `--ok` | `#7FA672` | Success |

#### Light

| Token | Hex |
|---|---|
| `--bg` | `#FBF6E8` |
| `--surface` | `#FFFCF1` |
| `--surface-2` | `#F5EFDC` |
| `--surface-code` | `#F2EBD3` |
| `--border` | `#E5DBBB` |
| `--border-strong` | `#CFC196` |
| `--border-soft` | `#EDE5C8` |
| `--text` | `#1A1812` |
| `--text-2` | `#5A5340` |
| `--text-3` | `#8A8268` |
| `--text-4` | `#B0A88A` |
| `--accent` | `#8C6A1F` |
| `--accent-soft` | `rgba(140,106,31,0.10)` |
| `--link` | `#8C6A1F` |
| `--warn` | `#A6620B` |
| `--danger` | `#A6362D` |
| `--info` | `#355E8F` |
| `--ok` | `#4F7045` |

Theme is applied via `data-theme="dark|light"` on `<html>`.

### Typography

| Role | Family | Weight | Notes |
|---|---|---|---|
| Body / UI | `Inter` | 400 / 500 / 600 / 700 | `font-size: 13.5px` body default |
| Editorial · brand mark, page H1 italic accent, lede paragraph | `Cormorant Garamond` | 500 (italic for brand + lede) | Used sparingly — never for body |
| Code | `JetBrains Mono` | 400 / 500 / 600 | `font-variant-numeric: tabular-nums` |

Type scale (compact density · default):

| Level | Size | Weight | Treatment |
|---|---|---|---|
| H1 (page title) | 28px | 600 | letter-spacing −0.018em · line-height 1.15 · italic Cormorant span for the accent phrase |
| Page lede | 19px | 500 | Cormorant Garamond · color `--text-2` · max-width 64ch |
| H2 (section) | 18px | 600 | Bottom border `1px solid --border-soft` · `§n` JetBrains-Mono number prefix in `--text-4` |
| H3 (sub-section) | 13px | 600 | UPPERCASE · letter-spacing 0.06em · `--text-2` |
| H4 | 13.5px | 600 | Sentence case |
| Body | 13.5px | 400 | line-height 1.55 |
| Code (block) | 12px | 400 | line-height 1.65 · tab-size 2 |
| Code (inline) | ~12.2px (0.9em) | 400 | Gold text · gold tinted bg · 1px gold-15% border · `white-space: nowrap` |

Comfortable density bumps body to 14.5px / lh 1.65 and proportionally up.

### Spacing & radii

- `--radius-card`: 4px (and 4px throughout — small, sober)
- Section spacing (H2): 40px above, 10px below; H3 28/8; H4 20/4; paragraph 8/14
- Sidebar gutter: 28px from page edge; sidebar width 264px
- Right TOC width: 240px
- Main column max-width: 880px (page-level) but paragraph max-width 70ch
- Top bar: 56px fixed height with `backdrop-filter: blur(12px)` on a tinted background

### Layout grid

```
+------------------------------------------------------------+
|  topbar (56px, sticky)                                     |
+----------+------------------------------------+------------+
| sidebar  |  main                              |  toc       |
| 264px    |  minmax(0, 1fr), max 880px         |  240px     |
| sticky   |  padding 40px 56px 96px            |  sticky    |
+----------+------------------------------------+------------+
```

Shell `max-width: 1440px; margin: 0 auto;`.

Breakpoints:

- `≤ 1100px` — TOC hides; shell becomes sidebar + main.
- `≤ 760px` — sidebar hides; main becomes the full column with reduced padding.

### Components

#### Top bar

- Brand mark `<span class="mark">xvn</span>` in italic Cormorant Garamond at 28px
- Eyebrow sub-label `DOCS · v0.9-ALPHA` in 11px JetBrains Mono uppercase
- Mock search field — 460px max, `Search docs, commands, schemas…` placeholder, `⌘K` keyboard pill
- Right cluster: `Copy as Markdown` button, theme toggle, tweaks toggle, GitHub link

#### Sidebar nav

- Background matches page `--bg` (no separate surface), `1px solid --border-soft` right divider
- Groups: 10.5px UPPERCASE eyebrow heading, then `<ul>` of links
- Each top-level link has a 2-char `<span class="num">` (e.g. `00`, `01`, `02`, `03`) in JetBrains Mono 10.5px, color `--text-4` (or `--accent` when active), width 22px
- Active state: text `--text`, background `--accent-soft`, `box-shadow: inset 2px 0 0 var(--accent)` (gold left strip)
- **Nested sub-list** (always visible, not collapsed):
  - `<ul class="nested">` with `margin-left: 19px` and `border-left: 1px solid --border-soft`
  - Each nested `<a>` has `margin-left: 6px; padding-left: 10px;` so the vertical guideline sits centered under the parent's section number and the active highlight does not touch the line
  - Nested font 12.5px, color `--text-3`, hover `--text-2`

#### Right TOC (on this page)

- 240px column, sticky from top:56
- Items 12.5px in `--text-3`
- Border-left hairline on the `<ul>` with each link adding `border-left: 1px solid transparent` that turns gold (`--accent`) when active
- Two indent levels supported via `li.lvl-3` (smaller font, deeper padding-left)

#### Agent context block (front-matter for AI)

A defining brand element — every page has one near the top.

- 1px border, 4px radius, 2px gold left bar (`::before` strip)
- Header strip with status dot, `CONTEXT FOR AI AGENTS` label in 10.5px JetBrains Mono uppercase, right-aligned hint like `also at /docs/llms.txt`
- Body: `<dl>` in a CSS grid `110px 1fr`, `dt` in `--text-3`, `dd` in `--text`
- Tag chips inside `dd` — `display: inline-block`, 1px border, 3px radius, 11px text, `white-space: nowrap`
- Toggleable via `data-context="hidden"` on `<html>`

#### Code blocks

- Wrapped in `<div class="code">` containing `<div class="code-head">` and `<pre><code>`
- Head: `lang` label + optional `file` label + right-aligned `Copy` button
- Body: monospace 12px, line-height 1.65, soft border, no rainbow highlight
- Manual token classes for syntax: `.tk-cmd` (gold), `.tk-flag` (info-blue), `.tk-str` (gold), `.tk-num` (gold-deep), `.tk-com` (italic muted), `.tk-var` (white), `.tk-type` (info-blue)
- The light theme inverts these to readable equivalents on cream paper

#### Callouts

`<div class="callout {info|warn|danger|ok}">` with `<span class="icon">` and `<div>` body. Left border 3px in the relevant accent color, neutral 1px border on the rest.

#### Tables

- Header row: 11.5px UPPERCASE in `--text-2` on `--surface` background, bottom border 1px
- Cells: 12.5px on `--bg`, 8/12 padding, soft hairlines
- Rows hover with `--surface-2` background

#### Key-value list (`<dl class="kv">`)

- Two-column grid `168px 1fr`, hairline above and per-row below
- `dt`: JetBrains Mono 11.5px `--text-3`
- `dd`: regular 13px `--text`

#### Feature row (home page)

- 4-column grid with vertical hairlines between cells
- Each cell: 10.5px UPPERCASE eyebrow in `--text-3`, then 14px line in `--text`
- First cell `padding-left: 0`; last cell `padding-right: 0`; others equal horizontal padding

#### Cards (home page navigation)

- 1px border, 4px radius, `--surface` background
- Title row: 14px name + JetBrains Mono section number in gold
- Description in 12.5px `--text-2`
- Meta line in 11px JetBrains Mono `--text-4`

#### Pills / badges

`<span class="pill {accent|warn|danger|ok|info}">` — 10.5px JetBrains Mono, 1px border, 3px radius. Each variant tints both border and text using `color-mix(in srgb, var(--<role>) 50%, var(--border))`.

#### Tweaks panel

Bottom-right floating panel, `display: none` until opened. Four toggles, all persisted to `localStorage`:

1. **Theme** — `dark` / `light` (applied as `data-theme` on `<html>`)
2. **Density** — `compact` / `comfortable` (applied as `data-density`; tweaks body type scale)
3. **Right TOC** — `shown` / `hidden` (applied as `data-toc`)
4. **Agent context** — `shown` / `hidden` (applied as `data-context`)

Visual: 264px wide, dark surface, segmented controls with `.on` state in `--accent-soft`/`--accent`.

## Interactions

All client-side enhancements live in `docs/docs.js` and are **strictly optional** — the site reads cleanly with JS disabled. JS adds:

### 1. Theme + density + TOC + agent-context toggles

A small `PREFS` object is read from `localStorage['xvn-docs-prefs']` on load and re-applied. Toggling any control updates `<html>` data attrs and persists. The CSS variables key off these data attrs, so theme switch is a single repaint.

### 2. Copy-as-Markdown button (top bar)

Clicking calls `fetch(button.dataset.mdUrl)` and copies the response to the clipboard. The button text flips to "Copied as Markdown" for 1.8s. If the fetch fails the button shows "See <url>" so the user can navigate to the mirror directly.

### 3. Code-block Copy button

Each code block has a `Copy` button in its header. Click copies `pre.innerText`, flips to "Copied" in gold for 1.4s.

### 4. Scrollspy (sidebar + TOC simultaneously)

- On init the script collects every link inside `.toc` and every link inside `.sidebar` whose href points to the current page with a `#hash`. These become the **tracked** links.
- It also collects sidebar links whose href is the current page with **no** hash — these are the **page-level** links (one per page typically).
- All target headings are resolved by `id` and **sorted by `offsetTop` ascending** (so DOM order is guaranteed regardless of which side the link came from).
- On every `scroll` and `resize` the script:
  - Walks the sorted headings; the topmost one whose `getBoundingClientRect().top - 96` is `≤ 0` is the "current" id.
  - **Bottom-pin edge case**: if `scrollY + innerHeight >= documentElement.scrollHeight - 4` (near-bottom), the last heading is forced active. Without this, a trailing heading that never scrolls above the offset would stay un-highlighted.
  - When a sub-section becomes active, `.active` is added to every link pointing at it (both sidebar nested item and TOC item) and **removed** from page-level links.
  - When no sub-section is in view (top of page), the page-level link is restored to `.active`.

### 5. ⌘K / Ctrl+K focuses the search

Pure visual hint — flashes a 1px outline on the search element for 700ms. There is no real search implementation (the site is static).

## Information architecture

The sidebar is **identical on all four pages** so users can see what's available without clicking through. Nested sub-items are always visible — no expand/collapse interaction.

Groups, in order:

1. **Start here** — `Overview` with nested `What it does`, `Quickstart`, `Where to next`, `For agents`, `Safety model`
2. **Operator manual** — `Manual` with nested tier and runbook anchors
3. **Reference** — `CLI · xvn` with nested `All verbs`, `A/B compare`, `Strategy authoring`, `Eval & reports`, `Providers`, `Dashboard`, `Remote CLI · Tailscale`, `Operator surfaces`, `Exit codes`
4. **Concepts** — `Architecture` with nested `Thesis`, `System diagram`, `Pipeline`, `Stage 1 · Intern`, `Stage 2 · Trader`, `Risk layer`, `Stage 3 · Execution`, `Eval framework`, `ERC-8004 identity`, `Workspace layout`
5. **For machines** — `llms.txt`, `sitemap.json`, the page's `.md` mirror

The "pin" card at the top of the sidebar is the **only** sidebar element that varies per page — used for a status callout, a binary identifier, or a concept stack note.

## Agent-readability requirements

Treat these as non-negotiable when porting:

- **No JS required to read.** Every page is consumable as plain HTML.
- **Markdown mirrors at the same path.** `manual.html` ↔ `manual.md`, served from the same directory.
- **`<link rel="alternate" type="text/markdown" href="…">`** in `<head>` of each HTML page pointing at its mirror.
- **`<meta name="ai:summary|audience|key_terms|mirror.md|llms-txt">`** tags on every page.
- **Agent context block** at the top of every content page with `route`, `summary`, `key_terms` (as chip tags), `do_not`, `mirrors`.
- **Copy-as-Markdown button** in the top bar, configured per page via `data-md-url`.
- **`llms.txt`** at `docs/llms.txt` with the canonical route table and hard rules.
- **`sitemap.json`** at `docs/sitemap.json` with structured per-page entries (id, route, summary, key terms, sections).

## Architecture diagram

The architecture page contains a hand-rendered inline SVG diagram (1000×560 viewBox) showing the five lanes — Operator surfaces, Dashboard transport, Engine, Storage, External — and the optional ERC-8004 lane. Boxes are styled with classes (`.surface`, `.eval`, `.store`, `.ext`, `.chain`) that pull from the same color tokens as the rest of the site, so it themes correctly. Replace with a real diagram (Mermaid, Excalidraw, or hand-authored SVG) sourced from `architecture-diagram.mermaid` in the repo.

## Source content

All content was reconciled with the xvision repo at commit `a73b18f` on 2026-05-20:

- `README.md`
- `MANUAL.md` (587 lines)
- `architecture.md` (638 lines)
- `architecture-diagram.mermaid`
- `docs/runbook/dashboard-auth.md`
- `docs/HACKATHON-1-PAGER.md`
- `.claude/skills/xvision-cli/references/cli.md`

The Markdown mirrors in this bundle are the canonical content snapshot.

## Files in this bundle

```
docs/
├── index.html              # Home / overview
├── manual.html             # Operator manual
├── cli.html                # CLI reference
├── architecture.html       # Architecture + system diagram
├── docs.css                # Full design system, ~770 lines
├── docs.js                 # Theme, scrollspy, copy-as-md, ~170 lines
├── index.md                # Markdown mirror
├── manual.md
├── cli.md
├── architecture.md
├── llms.txt                # Agent routing index
└── sitemap.json            # Structured page index
```

## Recommended port targets

- **Next.js App Router + MDX** with `next-themes` for the dark/light toggle, `rehype-pretty-code` or `shiki` for syntax highlighting, and `contentlayer` (or vanilla MDX) for the Markdown sources. Generate the `.md` mirrors and `sitemap.json` at build time from the MDX frontmatter.
- **Astro + Starlight** is the closest off-the-shelf match — three-column layout, theme toggle, sidebar with nested items, and content collections. Port the design tokens via a Starlight theme override; render the agent-context block as a custom Astro component fed by MDX frontmatter.
- **Plain static HTML** — the bundle itself is already viable as a no-build deploy if you serve it under `/docs/` on any HTTP server.

Whatever the choice: keep the Markdown mirrors, the `llms.txt`, and the `sitemap.json` as first-class outputs of the build — they are the contract with embedded agents reading the site.
