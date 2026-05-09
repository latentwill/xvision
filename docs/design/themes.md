# XVN Themes

> **Companion to:** `ui-elements.md` (surface inventory) and `gptprompts.md`
> (image-prompt library). This doc owns the canonical visual tokens.
>
> **Status:** v0.2 · 2026-05-09. Two candidate themes documented as peers:
> **Folio** (low-risk, 2026-fintech-with-classical-typography) and **Stack**
> (higher-risk, pixel/HyperCard / System 7). The choice is deferred until
> the Stack pilot completes.

---

## The theme decision

xvn needs a single visual identity to ship in v1. Two candidates survived
the wider thematic exploration; both hold the v0.2 UX from `ui-elements.md`
unchanged and only differ in the visual register.

| | **Folio** | **Stack** |
|---|---|---|
| Pitch | "Considered, not LARP." | "A trading workbench as a System 7 control panel." |
| Reference family | NousResearch quieter pages, Linear, Stripe | Apple System 7, HyperCard, Susan Kare |
| Mode | dark primary | **light primary** (departure from trader default) |
| Type | Cormorant + Inter + JetBrains Mono | Chicago + Geneva + Monaco (bitmap, no AA) |
| Distinctiveness | medium | very high |
| Risk | low | high — "looks like a game" is the killer |
| Effort to ship | ~1 week | ~3 weeks + commissioned icon set |
| Recommendation | safer, cheaper, ship-ready | higher upside if pilot validates craft |

The decision rule: **run the Stack pilot first** (1–3 days, see §3.4
below). If it validates with users, ship Stack. If not, ship Folio.

A third candidate, **Terminal Quant** (Bloomberg phosphor revival), survives
as a future opt-in mode for dense data routes only — not in v1.

---

# 1. Folio

## 1.1 What it is

A restrained, slightly-classical visual register for a builder-grade trading
tool. Looks considered, not LARP. The shift from generic 2026-fintech comes
down to four small moves:

| Element | v0 (Linear/Vercel) | Folio |
|---|---|---|
| Heading typeface | Inter (sans-serif) | Cormorant Garamond (transitional serif) |
| Primary accent | electric mint `#5BE0A2` | warm amber-gold `#D4A547` |
| Surface temperature | cool navy-charcoal | warm graphite (less blue) |
| Card radius | 10px | 4px |

Everything else is unchanged: body type stays Inter, numerics stay JetBrains
Mono, status pattern stays `●` + word, icons stay Lucide thin-line, voice
stays plain English.

The instinct comes from NousResearch's quieter pages — typographic gravitas
of a serif heading on a warm dark surface — without any of the hermetic /
alchemical / manuscript apparatus. No sigils, no illuminated initials, no
parchment foxing, no `Codex` / `Trials` / `Floor` section renaming.

## 1.2 Palette

```
SURFACES
  surface-primary    #0E1014    page background        (was #0B0F14, less blue)
  surface-elevated   #15171C    cards                  (was #11161D)
  surface-panel      #1C1F25    nested panels          (was #1A2029)

BORDERS
  border-default     #232730    1px on cards & dividers (was #1F2630)

TEXT
  text-primary       #ECE6D9    headings, body         (was #E8ECF1, slight cream)
  text-secondary     #9A9387    labels, helper copy    (was #8B95A4)
  text-tertiary      #5E5A52    metadata, timestamps   (was #5A6573)

ACCENT
  accent-primary     #D4A547    primary buttons,       (was #5BE0A2 electric mint)
                                equity-up, pass states  ← single biggest swap

STATUS
  status-warn        #DB9230    warnings               (was #F4B23A)
  status-danger      #C8443A    errors, equity-down    (was #F26A6A — less neon)
  status-info        #6E94BA    info, links            (was #6EB4F2)
```

Every color is rotated slightly toward warm and slightly desaturated. Reads
as ink-and-pigment, not fluorescent. **Equity-up is gold** (not mint);
equity-down is cinnabar.

## 1.3 Type

```
DISPLAY     Cormorant Garamond            page titles, card titles, brand mark
            transitional serif            sizes 24–48px
            italic permitted on subtitles

BODY        Inter                         labels, body copy, helper text
            humanist sans                 sizes 13–16px  (UNCHANGED from v0)

NUMERIC     JetBrains Mono                prices, hashes, IDs, ULIDs
            tabular figures ON            sizes 12–14px  (UNCHANGED from v0)
```

**Body stays sans.** Don't swap body to serif — kills data scan speed. Serif
reserved for *display* (page title, card title, brand) where the eye lingers.

Fallbacks if Cormorant unavailable: **Spectral** or **EB Garamond** read
similarly. Last-resort system Georgia loses ~30% of the register.

## 1.4 Borders & frames

- 1px solid `border-default` on every card and table divider
- Card radius **4px** (was 10px) — hint of "printed page," not "pill"
- No ornament glyphs, no double-rules, no hand-drawn flourishes
- Region dividers: plain 1px hairline in `border-default`

## 1.5 Buttons

```
PRIMARY     amber-gold fill (#D4A547)
            text in surface-primary (#0E1014)
            4px radius · 1px border in same accent · sentence case

SECONDARY   ghost — surface-elevated background
            text in text-primary
            4px radius · 1px border in border-default

DANGER      ghost with text in status-danger (#C8443A)
            4px radius · 1px border in status-danger

LINK        text in status-info (#6E94BA)
            underline on hover only
```

No "illuminated initial" cap, no drop-shadow, no gradient on the primary.

## 1.6 Status & iconography

Status pattern unchanged from v0.2: filled circle (`●`) plus a one-word label.

```
●  Running    accent-primary (gold)        ●  Warning    status-warn
●  Completed  accent-primary (gold)        ●  Failed     status-danger
●  Passed     accent-primary (gold)        ●  Vetoed     status-danger
●  Queued     status-info                  ●  Offline    text-tertiary
```

Icons stay Lucide thin-line stroke (1.5px). No sigils. Trade markers:
`▲ buy` (gold), `▼ sell` (cinnabar), `○ flat` (warm dim), `✕ close`
(off-white).

## 1.7 Texture

A 1–1.5% grain noise overlay on elevated surfaces (cards, modals). Page
background stays flat; chart data areas stay flat. Should be invisible at
first glance — perceptible only when looked for.

## 1.8 Brand mark

`xvn` wordmark in **Cormorant Garamond italic**, all lowercase, in
`text-primary`. No monogram, no ouroboros, no symbol.

```
  xvn        ← italic Cormorant lowercase, that's it
```

Favicon: same wordmark cropped to just `x` at 16×16 in `accent-primary`.

## 1.9 Voice in copy

Unchanged from v0.2. Plain English on every L1 surface. No section renaming.

```
✓  "Buys dips"                      ✗  "Reverteth ad mediam"
✓  "Add an LLM key to begin"        ✗  "Inscribe thy key herein"
```

## 1.10 What Folio deliberately does NOT do

| Move not taken | Why held back |
|---|---|
| Cream/parchment cards on dark | confuses data-dense readability |
| Alchemical sigils for status | cosplay; breaks first-time legibility |
| Ouroboros monogram | over-branding |
| Section renaming (`Codex`, `Trials`, `Floor`) | cognitive load with no comprehension gain |
| Hand-ruled flourishes, ornament glyphs | reads as Etsy |
| Body type swap to serif | hurts data scan speed |
| "Illuminated initial" buttons | twee |
| Foxing dots on hover | invisible-frequency noise |

## 1.11 Folio pilot path

1. **Type only** (~half a day). Swap heading font Inter → Cormorant. Render
   Control Tower, Run Detail, Inspector. If that alone reads as a different
   brand register, ship it. Type swap does ~80% of the work.
2. **Type + accent** (~one day). If pilot 1 reads as "current xvn with serif
   headers," add the mint→amber-gold swap.
3. **Full token swap** (~one week). Only if the first two don't carry it.

Expect to ship at the end of pilot 2.

---

# 2. Stack

## 2.1 What it is

A trading workbench that reads as a System 7 control panel — pixel-perfect,
color-coded, dense as a HyperCard stack. The most distinctive identity
available in the trading dashboard space, with a real risk of reading as
"indie game / joke project" if execution craft falters.

The big upfront departure: **Stack is light-mode primary.** HyperCard was
paper-bright; that's where the gravitas comes from. A dark variant exists
but the canonical brand surface is light. This breaks trader-default-dark
habit and is the second-largest decision (after the type system).

## 2.2 Palette

```
GROUND
  paper-white      #F4F1E8    primary surface (paper-cream, slight warm)
  paper-mid        #E5E0D2    elevated surfaces, where pattern fills go
  paper-shadow     #D4CDB8    nested panels
  ink-black        #1A1614    text, borders (warm near-black, never pure #000)

FLAG COLORS  (indexed semantic markers, saturated but not neon)
  flag-red         #C8302C    danger / equity-down / vetoed
  flag-green       #2C8C3A    pass / equity-up / completed
  flag-blue        #2C5C9C    info / queued / links
  flag-yellow      #D4A027    warning / attention / primary action accent
  flag-violet      #7C3CA0    finding / new observation
  flag-orange      #D46428    in-flight / pending

DARK VARIANT (optional — for night sessions, not canonical)
  ground-dark      #1A1614    primary surface
  ground-mid-dark  #2A2422    elevated
  ink-light        #F4F1E8    text inverts
  flag colors stay the same — high-contrast on either ground.
```

Six semantic flags handle every status. No gradients, no shading.

## 2.3 Type

```
DISPLAY    Chicago    System 7 menu font, bitmap pixel font
                      sizes 12px or 24px ONLY (bitmap fonts don't scale smoothly)
                      page titles, brand mark, primary buttons

BODY/UI    Geneva     System 7 Finder font, bitmap
                      sizes 9px or 12px
                      labels, body, table cells, helper text

NUMERIC    Monaco     System 7 monospace, bitmap
                      sizes 9px or 12px
                      prices, hashes, IDs, percentages
```

**All bitmap, integer pixel sizes only, NO anti-aliasing on body type.**
Display type can have light AA at 24px+. The sharpness IS the bite of the
theme.

Browser SPA constraints: needs `image-rendering: pixelated` plus careful
font loading (web-safe Chicago/Geneva/Monaco require licensed bitmap fonts
or pixel-revival alternatives). **Confirm bitmap rendering pipeline works
before pilot 3 — this is a real engineering risk item.**

Fallbacks (modern revivals):
- Chicago → ChicagoFLF (free) or Berkeley Mono (slightly modernized)
- Geneva → Apple's bundled Geneva, Sevenless pixel revival, or last-resort
  JetBrains Mono at 11px (loses character)
- Monaco → Apple's bundled Monaco (still ships on macOS) or Berkeley Mono

## 2.4 Borders & frames

```
Card frame:       2px solid ink-black on top/left
                  1px solid ink-black on bottom/right
                  → creates the System 7 "raised window" effect
                  0px radius — square corners, always

"Well" frame:     inverse — 1px top/left, 2px bottom/right
                  → indicates "input area" / "data well" / "depressed"

Title bar:        18px tall optional row at top of primary cards
                  fill: 1px-on / 1px-off horizontal stripes in ink-black at
                  30% opacity over paper-shadow
                  title in Chicago 12pt, left-aligned

Window controls:  modal / drawer cards get a 12×12 close box on the left of
                  the title bar (empty square or with X glyph)

Drop shadow:      a 1-pixel offset duplicate in ink-black at 50% opacity,
                  1px down + 1px right
                  → NEVER a soft blur. Pixel-shifted only.
                  → Card reads as "lifted" through grid discipline, not blur.

Inner separators: 1px ink-black hairlines between regions inside a card
                  → no spacing-only separators; always a visible rule
```

**No rounded corners anywhere.** This is the discipline.

## 2.5 Buttons

```
PRIMARY      Chicago text in ink-black on flag-yellow background
             2px ink-black border, 0px radius
             1px pixel drop shadow (offset, not blurred)
             on hover: invert (flag-yellow text on ink-black)
             on press: drop shadow disappears (button "depresses")

SECONDARY    Chicago text in ink-black on paper-white
             1px ink-black border, 0px radius, no drop shadow

DANGER       Chicago text in flag-red on paper-white
             1px flag-red border

LINK         Geneva text in flag-blue
             1px ink-black underline ALWAYS visible (not just on hover)

CHECKBOX     16×16 square, 1px ink-black border
             checked: ✓ in ink-black centered
             RADIO: 12px circle, 1px ink-black border
             checked: filled black dot 6px centered
```

## 2.6 Status & iconography — squares, not dots

```
■  flag-green    running / passed / completed
■  flag-yellow   warning / attention / reportable
■  flag-red      danger / vetoed / failed
■  flag-blue     info / queued
■  flag-violet   finding / new observation
■  flag-orange   in-flight / pending
▢  empty square  offline / not started
```

A 6×6 pixel **square** reads more honest to the grid than a circle. All
status icons are 16×16 pixel art at integer scale, never anti-aliased.

**Icon set is a real budget line item.** Lucide thin-line is wrong for Stack.
Commission ~30 16×16 pixel-art icons from a single hand for consistency.
Reference: Susan Kare's original Mac icon set (folder, magnifier, gear,
clipboard, padlock, bolt, globe, envelope, document, etc.) plus the 6 status
squares plus xvn-domain icons (chart, ticker, deployment, scenario).

Trade markers on charts:
- `▲` 2px-tall pixel triangle in flag-green — buy
- `▼` 2px-tall pixel triangle in flag-red — sell
- `◇` diamond outline in ink-black — flat
- `✕` 2px-stroke X in ink-black — close

## 2.7 Charts & graphs (where Stack really differentiates)

```
EQUITY LINE     2px ink-black line on paper-white
                each segment colored by direction:
                  → up segments in flag-green
                  → down segments in flag-red
                The line is multi-color, NOT a gradient under it.

OHLC BARS       1px line-width bars with high/low whiskers
                colored flag-green (close > open) or flag-red
                volume below as 2px stacked bars in ink-black at 50% opacity

DRAWDOWN        secondary chart below equity, ~80px tall
                drawdown periods as filled rectangles in flag-red at 30% opacity

TIME AXIS       1px hairline tick marks every interval
                labels in Geneva 9pt

GRID            1px hairline cross-grid in paper-shadow color (~4% opacity)
                barely visible alignment aid

ANIMATION       step-change ONLY. No tween, no easing.
                Chart "ticks" forward like a HyperCard refresh.
                State changes are instantaneous.
```

**The pixel-grid discipline extends to motion.** No smooth animations
anywhere. Trades fire. Markers appear. State changes happen between frames,
not across them.

## 2.8 Pattern fills

Used on title bars and "well" backgrounds, never on body copy areas.

```
pattern-stripes-h    1px horizontal stripes (1on/1off) in ink-black on
                     paper-shadow → title bars

pattern-dots         1px dots on a 4×4 grid in ink-black on paper-mid
                     → muted/disabled states, drag-handles

pattern-checker      1px checker in ink-black on paper-shadow
                     → "transparent/empty" states (rare)

pattern-diagonal     1px diagonal lines (1on/3off)
                     → "active/dragging" states only
```

All patterns 1px scale, snap to pixel grid. Card body main areas stay flat
paper-white — no texture in the data area.

## 2.9 Brand mark

```
xvn
▬▬▬
```

Wordmark in Chicago 24pt, lowercase, ink-black on paper-white. A 2px-tall
flag-yellow horizontal rule beneath, spanning the wordmark's width.

Optional **stacked-card glyph** for header use only: three overlapping 16×16
pixel squares with offset shadows, to the left of the wordmark. Drop the
glyph in the favicon — favicon is just the `x` letter at 16×16 in flag-yellow
on ink-black.

## 2.10 Voice in copy

Plain English, unchanged from v0.2. The HyperCard reference doesn't ask for
tone changes.

One small nudge allowed: **command-name buttons can be slightly more
terminal-flavored** — `Run` instead of `Run eval`, `Stop` instead of `Stop
deployment` — but only on primary action buttons where surrounding context
disambiguates. Echoes early-Mac menu style.

## 2.11 What Stack deliberately does NOT do

| Move not taken | Why held back |
|---|---|
| Soft drop-shadows | breaks the pixel-grid discipline |
| Anti-aliased body type | bitmap-feel comes from sharp edges; AA undoes it |
| Rounded corners on cards/buttons | not in the system; everything rectilinear |
| Gradient fills on charts | the multi-color step-change IS the gradient |
| Cute pixel-art mascot | reads as game, not tool |
| 8-bit sound effects | this is a trading app, not Lemmings |
| Window-shake animations | step-changes only; no easing |

## 2.12 Stack pilot path

| Step | Cost | What you learn |
|---|---|---|
| 1 | half day | Hand-mockup Run Detail in Stack tokens. Test if the chart treatment alone (multi-color segments, pixel ticks, no easing) reads as deliberate craft. |
| 2 | one day | Add Chicago/Geneva/Monaco type. Render 3 screens: Control Tower, Run Detail, Inspector. |
| 3 | three days | Commission 5 placeholder pixel icons. Ship a clickable HTML prototype of one route. Show to 5 users (mix of trader-leaning and builder-leaning). Ask: *"would you trust this to run real money?"* |
| **KILL** | — | If ≥3 of 5 users say "looks like a game" or "not serious enough," abandon Stack and ship Folio. |
| **GO** | — | If users say "considered" / "deliberate" / "I'd use this," commit to Stack. Update this doc to v0.3 with Stack as primary; Folio archived as alternate. |

The two real risks:

1. **"Looks like a game / not serious enough."** Pixel aesthetics can read
   as indie game, NFT meme, or jokey side-project. Mitigation is high-craft
   execution (bitmap rendering perfect, icons commissioned not slapped).
   Higher craft floor than Folio. If even slightly half-hearted, tips into
   LARP.
2. **Light mode breaks trader default.** Most users reflexively look for
   dark mode. Ship the dark variant alongside, but the canonical surface is
   light. Test before committing.

The one real opportunity:

**Stack is the most distinctive identity available in the trading dashboard
space.** Marketplace listing photos would be unmistakable. Folio looks like
"thoughtful 2026 fintech" — a category that already has Linear, Vercel,
Stripe. Stack looks like nothing else.

---

# 3. Where Folio and Stack agree

Both keep these v0.2 baseline rules; the choice is genuinely about visual
vocabulary, not level of restraint.

| Held in common | Folio value | Stack value |
|---|---|---|
| Body type stays sans (not serif) | Inter | Geneva |
| Numeric type stays mono | JetBrains Mono | Monaco |
| Status pattern: shape + word label | `●` + word | `■` + word |
| Plain English voice, no section renaming | yes | yes |
| Wordmark only, no monogram or symbol | yes | yes |
| No alchemical / heraldic / esoteric iconography | yes | yes |
| UX from `ui-elements.md` v0.2 unchanged | yes | yes |

Stack is just as **disciplined** as Folio — different vocabulary, same level
of restraint.

---

# 4. Future opt-in: Quant mode

**Terminal Quant** (Bloomberg revival — phosphor amber/green on near-black,
IBM Plex Mono everywhere, ASCII frames, status code letters `ER WN IN OK`)
survives as a candidate opt-in theme variant for dense data routes only —
specifically the Run List table and the Decisions tab inside Run Detail.
User-toggleable from `/settings`.

Not in v1 scope under either Folio or Stack as primary. If the L4 power-user
audience materializes and asks for it, ship as a third theme.

---

*Document version: 0.2 · 2026-05-09. Lives at
`/Users/edkennedy/Code/xianvec/docs/design/themes.md`. Cross-references:
`ui-elements.md`, `gptprompts.md`. v0.1 was Folio-only; v0.2 adds Stack as
peer pending pilot.*
