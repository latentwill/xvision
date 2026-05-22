# Custom Chart Themes — Deep Dive

**Date:** 2026-05-22  
**Focus:** Chart color theming, design tokens, dark mode, accessibility, and fintech color systems

---

## Core Principles

### 1. Dark Mode Is the Default for Trading
Every serious data tool ships dark by default (Linear, Bloomberg Terminal, Datadog, Vercel). Light mode is the override. The reasoning is not aesthetic — it's about sustained reading sessions. A user reading green/red numbers for 14 hours needs dark surfaces so the colored data carries visual weight without the page shouting.

**Source:** [Pixel Show — Designing Data-Dense Dashboards](https://pixel-show.com/blog/designing-data-dense-dashboards)

### 2. One Semantic Role, Two Color Scales
The same hex value hits human eyes very differently on white vs near-black. On dark, `#22c55e` reads as saturated and energetic. On white, it looks neon and harsh.

**Solution:** Two scales for the same semantic role:

```css
:root {                    /* dark */
  --green: #22c55e;        /* vivid */
  --red:   #ef4444;
}
[data-theme="light"] {
  --green: #16a34a;        /* one notch darker */
  --red:   #dc2626;
}
```

Same meaning, different optical weight. The user perceives "profit" and "loss" with the same intensity in both themes.

### 3. Four-Step Elevation in Dark Mode
Two elevation steps is not enough in dark mode — every panel bleeds into adjacent panels. Use four:

```css
--bg-void:      #0a0a0a;  /* page — furthest back */
--bg-primary:   #111111;  /* main app shell */
--bg-secondary: #161616;  /* cards, panels */
--bg-surface:   #1c1c1c;  /* hover states, popovers */
```

Each step is small (6–7 hex points apart) but together they create just enough visual separation. Layer subtle borders (`rgba(255,255,255,0.06)`) on top.

---

## Color Coding for Financial Data

### Red/Green Is Not Universal
Per W3C WCAG 2.2, ~8% of users have deuteranopia or protanopia (red-green color blindness). Three patterns solve this:

1. **Pair color with shape/icon** — green-up-arrow / red-down-arrow
2. **Threshold-based traffic-light borders** — color the border, encode delta in text
3. **Grayscale baseline + color on anomaly** — most cells stay neutral; only outliers get color

**Source:** [DataBrain — Fintech Data Visualization](https://www.usedatabrain.com/blog/fintech-data-visualization)

### Recommended Pairings
- **Variance/status:** Red-amber-green for compliance, risk thresholds, KPI vs target
- **Directional data:** Blue scales for deposits over time, AUM growth (direction meaningful but not "good vs bad")
- **Grayscale:** Right choice for any visualization where the audience hasn't been trained on the color encoding

---

## TradingView Color Schemes (Community Favorites)

From [BYDFi community answers](https://www.bydfi.com/en/questions/what-are-the-best-color-schemes-for-tradingview-charts-in-the-cryptocurrency-industry):

| Scheme | Background | Accents | Use Case |
|--------|-----------|---------|----------|
| Dark Blue + Light Gray | Dark blue | Light gray highlights | Professional, sleek, reduces eye strain |
| Black + Green/Red | Black | Green (up), Red (down) | Maximum focus, familiar trader interface |
| Dark Gray + Light Blue | Dark gray | Light blue accents | Sophisticated, optimizes visibility |

---

## Lightweight Charts Customization

TradingView's Lightweight Charts allows full theme customization via options:

```js
const chart = LightweightCharts.createChart(container, {
  layout: {
    background: { color: '#222' },
    textColor: '#DDD',
  },
  grid: {
    vertLines: { color: '#444' },
    horzLines: { color: '#444' },
  },
});

// Axis borders
chart.priceScale().applyOptions({ borderColor: '#71649C' });
chart.timeScale().applyOptions({ borderColor: '#71649C' });
```

**Key customization points:**
- `layout.background` — chart canvas background
- `layout.textColor` — all text elements
- `grid.vertLines` / `grid.horzLines` — grid line colors
- `priceScale().borderColor` / `timeScale().borderColor` — axis borders
- Series-level colors (candlestick up/down, wick, border)

**Source:** [Lightweight Charts — Chart Colors Tutorial](https://tradingview.github.io/lightweight-charts/tutorials/customization/chart-colors)

---

## Fintech Design System References

### Stripe Design System
- **Primary:** Indigo `#533afd`
- **Body ink:** Deep navy `#0d253d` (never pure black)
- **Tabular figures:** Every money/numeric cell uses `font-feature-settings: "tnum"`
- **Gradient mesh:** Cream, sherbet orange, lavender, indigo, ruby pink
- **CTA pattern:** One filled button per band, pill-shaped, 8×16px padding

**Source:** [shadcn.io — Stripe Design System](https://www.shadcn.io/design/stripe)

### Wise Design System
- **Primary:** Lime green `#9fe870`
- **Canvas:** Pale sage `#e8ebe6`
- **Ink:** Near-black `#0e0f0c` (faint olive warmth)
- **Radius:** 24px on every button and card
- **Hero type:** Weight 900 at 64–126px (heaviest in fintech)
- **Semantic palette:** Positive `#2ead4b`, Negative `#d03238`, Warning `#ffd11a`

**Source:** [shadcn.io — Wise Design System](https://www.shadcn.io/design/wise)

---

## Design Tokens & CSS Variables

Modern fintech dashboards use token-based color systems:

```css
/* Semantic tokens */
--color-positive: #22c55e;      /* profit, up, success */
--color-negative: #ef4444;      /* loss, down, error */
--color-warning: #f59e0b;       /* alert, pending */
--color-info: #3b82f6;          /* neutral direction */

/* Surface tokens */
--surface-void: #0a0a0a;
--surface-primary: #111111;
--surface-secondary: #161616;
--surface-elevated: #1c1c1c;

/* Text tokens */
--text-primary: #ffffff;
--text-secondary: rgba(255,255,255,0.7);
--text-tertiary: rgba(255,255,255,0.5);

/* Border tokens */
--border-subtle: rgba(255,255,255,0.06);
--border-default: rgba(255,255,255,0.1);
```

**Source:** [Penpot — Developer's Guide to Design Tokens](https://penpot.app/blog/the-developers-guide-to-design-tokens-and-css-variables/)

---

## Conditional Density & Emphasis

Not every green needs to scream. Use a two-tier emphasis system:

```css
/* High emphasis — hero values, summary stats */
.pnl-large { color: var(--green); font-weight: 600; }

/* Low emphasis — repeating cells in tables */
.pnl-cell.win  { color: rgba(34, 197, 94, 0.85); }
.pnl-cell.loss { color: rgba(239, 68, 68, 0.85); }
```

Same semantic, different visual weight. The summary number screams; the same number repeated 80 times in a table whispers.

---

## Anti-Patterns to Avoid

1. **3D charts** — Always reduce data accuracy. No fintech use case justifies them.
2. **Dual-axis charts** — Almost always misleading. Use small multiples instead.
3. **Pie charts past 5 segments** — Viewers cannot compare slice sizes accurately.
4. **Default chart library defaults** — Most libraries ship with gradients, poor tooltips, and failing accessibility palettes. Spend the first two hours overriding defaults.
5. **Mixing currencies on a single axis** — Without explicit conversion rate annotation, this destroys precision.
6. **Hidden time zones** — Every timestamp must declare its timezone explicitly.

---

## Screenshots in This Folder

| File | Source | Description |
|------|--------|-------------|
| `shadcn-stripe-design.png` | shadcn.io | Stripe fintech design system tokens |
| `shadcn-wise-design.png` | shadcn.io | Wise fintech design system tokens |
| `bydfi-color-schemes.png` | BYDFi | Community color scheme recommendations |
| `databrain-fintech-viz.png` | DataBrain | Fintech data visualization best practices |
| `pixel-show-density.png` | Pixel Show | Data-dense dashboard design lessons |
| `lightweight-charts-custom.png` | TradingView | Lightweight Charts customization tutorial |
| `pineify-candle-colors.png` | Pineify | TradingView candlestick color guide |
| `penpot-tokens.png` | Penpot | Design tokens & CSS variables guide |

---

## Full URL List

See [`urls.md`](urls.md) for all collected links.
