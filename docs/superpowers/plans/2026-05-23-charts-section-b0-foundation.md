# Charts Section B0 — Foundation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the foundation that unblocks the four Charts dashboard canvases (B1–B4): sidebar entry, `/charts` route scaffolding, the `Strategy.color` schema migration, theme-token superset (warm palette + strategy rotation + heat ramp + typography), three new fixtures, the `/chart-lab/dashboards` review tab, and the v2 dashboard-overview payload endpoint stub.

**Architecture:** Reuses the Track A `frontend/web/src/components/chart/v2/` substrate (primitives, adapters, hooks, columnar payload, theme). Adds one sidebar nav entry between `Scenarios` and `Eval`, a `/charts` route with nested layout + four placeholder shells, and extends `Chart2ThemeDefinition` with five new token groups mirrored from `docs/design/trading-charts/XVN.zip` → `chart-theme.css`. One backend migration adds `Strategy.color` (nullable TEXT). One backend endpoint returns a fixture-backed `MultiStrategyEquityBundle` so B1 can wire UI immediately.

**Tech Stack:** Rust 2021 + sqlx migrations, axum routes, ts-rs exports, React 18 + TypeScript + Vite, Tailwind, TanStack Query, react-router-dom.

**Spec:** `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §2 (decisions 6–8), §3 (Track B / B0 milestone), §4-amended (new primitives + tokens + fixtures), §6 (payload schemas), §7 (IA + sidebar diagram), §11 (locked decisions).

**Prereqs:**
- A-M0 landed (it has — `frontend/web/src/components/chart/v2/*` exists).
- Migration number **034** reserved in `team/MANIFEST.md` (done in the same commit that lands the migration).
- Contract: `team/contracts/charts-section-b0.md`.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/migrations/034_strategies_color.sql` | Create | Add nullable `color TEXT` column to strategies table |
| `crates/xvision-engine/migrations/034_strategies_color.down.sql` | Create | Drop the column |
| `crates/xvision-engine/src/strategies/store.rs` *(or equivalent)* | Modify | Round-trip the `color` column in insert + load |
| `crates/xvision-engine/src/strategies/mod.rs` *(or wherever Strategy lives)* | Modify | Add `pub color: Option<String>` to the `Strategy` struct with `#[serde(default)]` + ts-rs export |
| `crates/xvision-engine/tests/strategies_color_roundtrip.rs` | Create | Migration + insert/load round-trip + back-compat (NULL → None) |
| `crates/xvision-engine/src/api/charts_dashboards.rs` | Create | `GET /api/v2/charts/dashboards/overview` returning `MultiStrategyEquityBundle` (fixture-backed in B0; real builder in B1) |
| `crates/xvision-engine/src/api/mod.rs` | Modify | `pub mod charts_dashboards;` + route registration |
| `crates/xvision-dashboard/src/routes.rs` | Modify | Mount `/api/v2/charts/dashboards/overview` |
| `frontend/web/src/api/types.gen/Strategy.ts` | Modify (ts-rs auto-export) | `color?: string \| null` added by ts-rs export |
| `frontend/web/src/api/types.gen/MultiStrategyEquityBundle.ts` | Create (ts-rs auto-export) | New shared type for B1+ |
| `frontend/web/src/components/chart/v2/types.ts` | Modify | Add `Annotation`, `AnnotatedChartPayload`, `LiquidationLevel`, `LiquidationHeatmapPayload` types per spec §6.2–§6.3 (B3/F-CHART-LIQHEAT consumers; defined now for stability) |
| `frontend/web/src/components/chart/v2/theme.ts` | Modify | Extend `Chart2ThemeDefinition` with `warm`, `strategyRotation`, `heatRamp`, `typography`, `radius` groups |
| `frontend/web/src/components/chart/v2/__fixtures__/multi-strategy-equity.json` | Create | 5 strategies × 240 daily points |
| `frontend/web/src/components/chart/v2/__fixtures__/annotations.json` | Create | 5 annotations from handoff |
| `frontend/web/src/components/chart/v2/__fixtures__/monthly-returns.json` | Create | 5 strategies × 17 months |
| `scripts/gen-chart-v2-fixtures.ts` | Modify | Add generators for the three new fixtures (idempotent re-run) |
| `frontend/web/src/styles/globals.css` | Modify | Add `.caps` utility class; ensure Google Fonts (Cormorant, Inter, JetBrains Mono) loaded once |
| `frontend/web/index.html` | Modify *(only if Google Fonts not already linked here)* | Single `<link>` to Cormorant + Inter + JetBrains Mono with one URL |
| `frontend/web/src/components/shell/Sidebar.tsx` | Modify | Insert `{ to: "/charts", label: "Charts", icon: "chart-pie" }` between `Scenarios` and `Eval` (gated behind `xvn.chartv2=1` cookie until B-rollout) |
| `frontend/web/src/components/shell/Sidebar.test.tsx` | Modify | Add tests: entry hidden when cookie absent; visible + correct order when cookie set |
| `frontend/web/src/components/primitives/Icon.tsx` *(or wherever `IconName` lives)* | Modify | Add `chart-pie` to `IconName` union (if missing) |
| `frontend/web/src/routes.tsx` | Modify | Mount `/charts/*` lazy route → `routes/charts/ChartsLayout` |
| `frontend/web/src/routes/charts/ChartsLayout.tsx` | Create | Subnav row (`Overview · Compare · Annotated · Hero`) + `<Outlet/>`; redirect index → `/charts/overview` |
| `frontend/web/src/routes/charts/ChartsOverview.tsx` | Create | Placeholder shell — renders `<EmptyState text="B1: Overview" />`; B1 fills with `DarkMinimalDashboard` |
| `frontend/web/src/routes/charts/ChartsCompare.tsx` | Create | Placeholder for B2 |
| `frontend/web/src/routes/charts/ChartsAnnotated.tsx` | Create | Placeholder for B3 |
| `frontend/web/src/routes/charts/ChartsHero.tsx` | Create | Placeholder for B4 |
| `frontend/web/src/routes/charts/ChartsLayout.test.tsx` | Create | Routes resolve under cookie; subnav links render |
| `frontend/web/src/routes/chart-lab/ChartLabDashboards.tsx` | Create | New tab — links to `/chart-lab/dashboards/{overview,compare,annotated,hero}` |
| `frontend/web/src/routes/chart-lab/index.tsx` | Modify | Add `Dashboards` tab to the existing tab array |
| `team/MANIFEST.md` | Modify | Reserve migration **034** in the registry |
| `team/contracts/charts-section-b0.md` | Create *(authored separately, alongside this plan)* | Track contract |
| `team/status/charts-section-b0.md` | Create *(authored by the claiming worker)* | Status file |

---

## Tasks

### Task 1 — Reserve migration 034 and write the SQL

**Files:**
- Modify: `team/MANIFEST.md` (already done in the planning commit; verify it's present before this task starts)
- Create: `crates/xvision-engine/migrations/034_strategies_color.sql`
- Create: `crates/xvision-engine/migrations/034_strategies_color.down.sql`

> Before touching the migration file, **verify migration 034 is yours** in `team/MANIFEST.md` (registry row 034 must read `charts-section-b0`). If the row has a different owner or is missing, stop and reconcile with the conductor before continuing.

- [ ] **Step 1: Locate the `strategies` table definition**

```bash
grep -rn "create table .*strategies\|CREATE TABLE .*strategies" crates/*/migrations/ | head -10
```

Identifies which crate (`xvision-engine` vs `xvision-core`) owns the
`strategies` table. The migration goes into that crate's migrations
dir per the two-dir convention in CLAUDE.md.

- [ ] **Step 2: Write the up migration**

```sql
-- crates/<owning-crate>/migrations/034_strategies_color.sql
-- Add per-strategy color for the Charts dashboard section (B0).
-- See docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md §11.5
-- Nullable: existing rows default to NULL; render layer falls back to the
-- strategyRotation palette by stable index at render time.
ALTER TABLE strategies ADD COLUMN color TEXT;
```

- [ ] **Step 3: Write the down migration**

```sql
-- crates/<owning-crate>/migrations/034_strategies_color.down.sql
ALTER TABLE strategies DROP COLUMN color;
```

> SQLite's `ALTER TABLE … DROP COLUMN` requires 3.35+. If the workspace pins an older SQLite, the down has to recreate the table without the column — verify `Cargo.toml` of the owning crate before writing the down.

- [ ] **Step 4: Run the migration test harness**

```bash
cd /Users/edkennedy/Code/xvision/.claude/worktrees/charts-section-b0
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-charts-b0"
cargo test -p <owning-crate> --test migrations
```

Expected: green. Migration up + down round-trips against the test schema.

- [ ] **Step 5: Commit**

```bash
git add team/MANIFEST.md crates/<owning-crate>/migrations/034_strategies_color.sql crates/<owning-crate>/migrations/034_strategies_color.down.sql
git commit -m "migration(034): add strategies.color TEXT column for charts B0"
```

### Task 2 — Add `Strategy.color` field + ts-rs export

**Files:**
- Modify: `crates/<owning-crate>/src/strategies/mod.rs` (or wherever `pub struct Strategy { … }` lives — grep `pub struct Strategy` to find)
- Modify: `crates/<owning-crate>/src/strategies/store.rs` (or equivalent — the insert + load functions)
- Create: `crates/<owning-crate>/tests/strategies_color_roundtrip.rs`

- [ ] **Step 1: Write the failing round-trip test**

```rust
// crates/<owning-crate>/tests/strategies_color_roundtrip.rs
use <owning_crate>::strategies::{Strategy, StrategyStore};
use sqlx::SqlitePool;

#[sqlx::test(migrations = "./migrations")]
async fn color_roundtrip_persists_when_set(pool: SqlitePool) {
    let store = StrategyStore::new(pool.clone());
    let mut s = sample_strategy();
    s.color = Some("#D4A547".to_string());
    let id = store.insert(&s).await.expect("insert");
    let loaded = store.load(&id).await.expect("load");
    assert_eq!(loaded.color, Some("#D4A547".to_string()));
}

#[sqlx::test(migrations = "./migrations")]
async fn color_defaults_to_none_when_absent(pool: SqlitePool) {
    let store = StrategyStore::new(pool.clone());
    let s = sample_strategy();              // no color set
    let id = store.insert(&s).await.expect("insert");
    let loaded = store.load(&id).await.expect("load");
    assert_eq!(loaded.color, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_row_deserializes_with_none(pool: SqlitePool) {
    // Insert a row directly bypassing the store helper to mimic a row that
    // landed before the field existed in the Rust type (real legacy rows
    // simply have NULL after migration 034 — same shape).
    sqlx::query("INSERT INTO strategies (id, name, … /* legacy columns */ ) VALUES (?, ?, …)")
        .bind("legacy-1")
        .bind("Legacy Strategy")
        // … fill in legacy columns ONLY (no color)
        .execute(&pool).await.unwrap();
    let store = StrategyStore::new(pool);
    let loaded = store.load("legacy-1").await.expect("load");
    assert_eq!(loaded.color, None);
}

fn sample_strategy() -> Strategy { /* minimal Strategy literal — fill from the actual struct definition you find in Step 2 */ }
```

The exact `sample_strategy()` body depends on Strategy's required fields — fill it in from the struct definition you read in Step 2.

- [ ] **Step 2: Run the test to verify it fails**

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-charts-b0"
cargo test -p <owning-crate> --test strategies_color_roundtrip
```

Expected: FAIL with "no field `color` on Strategy".

- [ ] **Step 3: Add the field**

```rust
// in src/strategies/mod.rs (or wherever Strategy lives)
#[derive(Debug, Clone, Serialize, Deserialize, /* …existing derives… */)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../frontend/web/src/api/types.gen/"))]
pub struct Strategy {
    // …existing fields…

    /// Optional per-strategy display color (hex, e.g. "#D4A547").
    /// Charts dashboards fall back to the strategyRotation palette
    /// by stable index when this is None.
    #[serde(default)]
    pub color: Option<String>,
}
```

- [ ] **Step 4: Wire the store**

```rust
// in src/strategies/store.rs (or wherever insert/load live)
// insert side:
sqlx::query(r#"
    INSERT INTO strategies (…existing columns…, color)
    VALUES (…existing binds…, ?)
"#)
.bind(/* existing binds */)
.bind(s.color.as_ref())   // sqlx maps Option<&String> → NULL when None
.execute(&self.pool).await?;

// load side: add `color` to the SELECT column list and to the row mapper
sqlx::query_as::<_, StrategyRow>(r#"
    SELECT id, name, …, color FROM strategies WHERE id = ?
"#)
```

Where `StrategyRow` is the existing FromRow shim (or `Strategy` directly if `#[derive(FromRow)]` is used). Add the field there too.

- [ ] **Step 5: Run the test to verify it passes**

```bash
cargo test -p <owning-crate> --test strategies_color_roundtrip
```

Expected: PASS for all three tests.

- [ ] **Step 6: Regenerate ts-rs exports (if not automatic)**

```bash
cargo test -p <owning-crate> --features ts-export ts_export
```

Or whatever the workspace convention is — search for an existing `ts_export` test fixture. Verify `frontend/web/src/api/types.gen/Strategy.ts` now includes `color?: string | null`.

- [ ] **Step 7: Commit**

```bash
git add crates/<owning-crate>/src/strategies/mod.rs crates/<owning-crate>/src/strategies/store.rs crates/<owning-crate>/tests/strategies_color_roundtrip.rs frontend/web/src/api/types.gen/Strategy.ts
git commit -m "feat(strategies): add color field with NULL-as-fallback semantics"
```

### Task 3 — Define new payload types in frontend

**Files:**
- Modify: `frontend/web/src/components/chart/v2/types.ts`

The handoff-driven payload types are defined now (B0) so B1–B4 consumers + the F-CHART-LIQHEAT followup share one source of truth.

- [ ] **Step 1: Append the new types**

```typescript
// at the bottom of frontend/web/src/components/chart/v2/types.ts

// Multi-strategy equity bundle — used by B1 + B2 + B4.
export type MultiStrategyEquityBundle = {
  kind: "multi_strategy_equity";
  generatedAt: number;            // unix seconds
  granularity: string;
  time: number[];
  strategies: Array<{
    id: string;
    name: string;
    short: string;
    color: string;                 // hex; resolved server-side (Strategy.color or rotation fallback)
    kind: string;                  // "Trend" | "Momentum" | "Reversion" | "Vol" | "Bench"
    dashed?: boolean;
    equity: number[];
    drawdown: number[];
    monthly: Array<{ year: number; month: number; value: number }>;
    metrics: {
      return: number; sharpe: number; mdd: number; win: number; pf: number;
    };
  }>;
  lead?: string;                   // defaults to strategies[0].id when omitted
};

// Annotated chart payload — used by B3.
export type Annotation = {
  idx: number;
  side: "top" | "bottom";
  type: "PATTERN" | "FLOW" | "RISK" | "REVERSION" | "STRUCTURE";
  title: string;
  body: string;
  conf: number;                    // 0..1
  action: "WATCH" | "LONG" | "SHORT" | "CAUTION";
  danger?: boolean;
  ts?: number;                     // unix seconds, for insight log timestamp
};

export type AnnotatedChartPayload = {
  kind: "annotated";
  source: "run" | "live";
  run_id?: string;                 // present when source = "run"
  symbol?: string;                 // present when source = "live"
  asset: string;
  granularity: string;
  candles: CandleColumns;
  ema?: LineSeries;
  annotations: Annotation[];       // may be [] if producer not wired (live source)
};

// Reserved for F-CHART-LIQHEAT (Chart 04). No producer/consumer in this wave.
export type LiquidationLevel = {
  price: number;
  heat: number;
  notional: number;
  side: "long" | "short";
};

export type LiquidationHeatmapPayload = {
  kind: "liquidation_heatmap";
  asset: string;
  granularity: string;
  candles: CandleColumns;
  ema?: LineSeries;
  levels: LiquidationLevel[];
  cascade: {
    longExposure: number;
    shortExposure: number;
    nearestWall: number;
    cascadeRisk: number;
  };
};

// Extend the AnyChartV2Payload union.
export type AnyChartV2Payload =
  | RunChartV2Payload
  | CompareChartV2Payload
  | ScenarioChartV2Payload
  | StrategyChartV2Payload
  | LiveChartV2Payload
  | WizardPreviewV2Payload
  | AnnotatedChartPayload;
  // NB: MultiStrategyEquityBundle and LiquidationHeatmapPayload are not
  // part of AnyChartV2Payload — they have their own endpoints and consumers.

export type DashboardFixtureKey =
  | "multi-strategy-equity"
  | "annotations"
  | "monthly-returns";
```

- [ ] **Step 2: Typecheck**

```bash
cd frontend/web && npm run typecheck
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/components/chart/v2/types.ts
git commit -m "feat(chart-v2): add Track B + F-CHART-LIQHEAT payload types"
```

### Task 4 — Extend `Chart2ThemeDefinition` with the design-token superset

**Files:**
- Modify: `frontend/web/src/components/chart/v2/theme.ts` (or wherever `Chart2ThemeDefinition` is declared — grep to confirm)

Reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/source/charts/chart-theme.css`. Mirror token names verbatim so the design package is portable.

- [ ] **Step 1: Locate the existing `Chart2ThemeDefinition`**

```bash
grep -rn "Chart2ThemeDefinition" frontend/web/src
```

Find the type and each theme that implements it (dark, folio-dark, light, black per A-M0).

- [ ] **Step 2: Extend the type**

```typescript
// Append to whichever file holds `Chart2ThemeDefinition`.
export type Chart2WarmPalette = {
  gold: string;
  amber: string;
  bronze: string;
  ember: string;
  copper: string;
  plum: string;
  teal: string;
  info: string;
  warn: string;
  danger: string;
};

export type Chart2StrategyRotationEntry = {
  id: string;
  name: string;
  short: string;
  color: string;
  kind: "Trend" | "Momentum" | "Reversion" | "Vol" | "Bench";
  dashed?: boolean;
};

export type Chart2HeatRamp = {
  scorching: { color: string; alpha: number };
  hot: { color: string; alpha: number };
  warm: { color: string; alpha: number };
  cool: { color: string; alpha: number };
  cold: { color: string; alpha: number };
};

export type Chart2Typography = {
  fontSerif: string;
  fontSans: string;
  fontMono: string;
};

export type Chart2Radius = {
  card: string;
  sm: string;
};

export type Chart2ThemeDefinition = {
  // …existing groups (surface, candle, overlay, marker, position, panes,
  // compare, motion, density)…

  // Track B additions (2026-05-23):
  warm: Chart2WarmPalette;
  strategyRotation: Chart2StrategyRotationEntry[];   // length 8, ordered
  heatRamp: Chart2HeatRamp;                          // also used by F-CHART-LIQHEAT
  typography: Chart2Typography;
  radius: Chart2Radius;
};
```

- [ ] **Step 3: Add the dark-theme values (canonical, mirror handoff)**

```typescript
// dark theme block
warm: {
  gold:   "#D4A547",
  amber:  "#E5B86A",
  bronze: "#A87A3C",
  ember:  "#C16A3A",
  copper: "#8C4A2E",
  plum:   "#8E6789",
  teal:   "#5E8A8C",
  info:   "#6F8FB8",
  warn:   "#DB9230",
  danger: "#C8443A",
},
strategyRotation: [
  { id: "fib", name: "Fibonacci Golden Cross", short: "Fib · GC",     color: "#D4A547", kind: "Trend" },
  { id: "ema", name: "EMA Pullback",           short: "EMA · 50/200", color: "#E8DCB0", kind: "Trend" },
  { id: "brk", name: "Breakout Retest",        short: "BRK · 4h",     color: "#E07A3A", kind: "Momentum" },
  { id: "msw", name: "Momentum Swing",         short: "MSW · 1d",     color: "#B98AB4", kind: "Momentum" },
  { id: "mvr", name: "Mean Reversion AI",      short: "MVR · 15m",    color: "#6BAFA8", kind: "Reversion" },
  { id: "vsc", name: "Volatility Scalper",     short: "VSC · 5m",     color: "#D67B5C", kind: "Vol" },
  { id: "lqh", name: "Liquidation Hunter",     short: "LQH · 1h",     color: "#8C6024", kind: "Vol" },
  { id: "btc", name: "BTC Buy & Hold",         short: "BTC · HOLD",   color: "#6B6553", kind: "Bench", dashed: true },
],
heatRamp: {
  scorching: { color: "#FF6B5C", alpha: 0.48 },
  hot:       { color: "#E04A3A", alpha: 0.42 },
  warm:      { color: "#A93428", alpha: 0.36 },
  cool:      { color: "#6A2A22", alpha: 0.30 },
  cold:      { color: "#3A1E1A", alpha: 0.22 },
},
typography: {
  fontSerif: '"Cormorant Garamond", serif',
  fontSans:  '"Inter", sans-serif',
  fontMono:  '"JetBrains Mono", monospace',
},
radius: {
  card: "6px",
  sm:   "4px",
},
```

- [ ] **Step 4: Add derived values for folio-dark / light / black themes**

For each non-dark theme, copy the entire dark block as the default and override only where it would actively crash or read unreadably. Light theme typically wants `warm` unchanged (the gold/ember palette reads on light too); the `strategyRotation` colors are stable across themes; `heatRamp` and `typography` are also stable. Skip per-theme tweaks until a B-milestone surfaces a real readability issue.

- [ ] **Step 5: Add a snapshot test**

```typescript
// frontend/web/src/components/chart/v2/theme.test.ts (append, do not replace)
import { describe, it, expect } from "vitest";
import { chart2ThemeDark } from "./theme";

describe("Chart2ThemeDefinition Track B extensions", () => {
  it("has 10 warm-palette tokens", () => {
    expect(Object.keys(chart2ThemeDark.warm)).toHaveLength(10);
  });
  it("has 8 strategy rotation entries", () => {
    expect(chart2ThemeDark.strategyRotation).toHaveLength(8);
    expect(chart2ThemeDark.strategyRotation.at(-1)?.dashed).toBe(true);
  });
  it("has 5 heat-ramp stops in descending heat order", () => {
    const keys = Object.keys(chart2ThemeDark.heatRamp);
    expect(keys).toEqual(["scorching", "hot", "warm", "cool", "cold"]);
  });
});
```

- [ ] **Step 6: Run typecheck + tests**

```bash
cd frontend/web && npm run typecheck && npm test -- chart/v2/theme
```

Expected: clean + 3 passing.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/components/chart/v2/theme.ts frontend/web/src/components/chart/v2/theme.test.ts
git commit -m "feat(chart-v2): extend theme with warm palette, strategy rotation, heat ramp, typography"
```

### Task 5 — Wire Google Fonts + add `.caps` utility class

**Files:**
- Modify: `frontend/web/index.html` (or wherever fonts are linked today)
- Modify: `frontend/web/src/styles/globals.css`

- [ ] **Step 1: Audit existing font loading**

```bash
grep -rn "fonts.googleapis\|Cormorant\|JetBrains\|Inter:" frontend/web/index.html frontend/web/src/styles
```

If any of the three families are already loaded, dedupe rather than re-add. Track A may have already brought in Inter.

- [ ] **Step 2: Ensure a single `<link>` loads all three families**

```html
<!-- frontend/web/index.html (in <head>) -->
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Cormorant+Garamond:ital,wght@0,500;1,500&family=Inter:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
```

- [ ] **Step 3: Add `.caps` utility class to globals.css**

```css
/* frontend/web/src/styles/globals.css — append */
.caps {
  font-family: "Inter", sans-serif;
  font-size: 10.5px;
  letter-spacing: 0.10em;
  text-transform: uppercase;
  color: var(--text-3);
  font-weight: 500;
}
```

- [ ] **Step 4: Verify a build**

```bash
cd frontend/web && npm run build
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/index.html frontend/web/src/styles/globals.css
git commit -m "feat(charts): load Cormorant + Inter + JetBrains Mono; add .caps utility"
```

### Task 6 — Generate the three new fixtures

**Files:**
- Modify: `scripts/gen-chart-v2-fixtures.ts`
- Create (generated): `frontend/web/src/components/chart/v2/__fixtures__/multi-strategy-equity.json`
- Create (generated): `frontend/web/src/components/chart/v2/__fixtures__/annotations.json`
- Create (generated): `frontend/web/src/components/chart/v2/__fixtures__/monthly-returns.json`

Port the handoff's deterministic generators from
`docs/design/trading-charts/XVN.zip` → `design_handoff_charts/source/charts/chart-data.js`
(mulberry32 PRNG; `makeEquity`, `makeTime`, `makeDrawdownSeries`,
`makeMonthlyMatrix`, `makeCandles`).

- [ ] **Step 1: Port mulberry32 + the four generators into the script**

```typescript
// scripts/gen-chart-v2-fixtures.ts — append (do not duplicate if a PRNG already exists)
function mulberry32(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6D2B79F5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const ROTATION = /* paste from theme.ts: strategyRotation */;

function makeEquity(target: number, points: number, seed: number): number[] {
  const rng = mulberry32(seed);
  const out: number[] = [];
  let v = 0;
  for (let i = 0; i < points; i++) {
    const shock = (rng() - 0.5) * 2 * 0.012;
    v += 0.0006 + shock;
    v += (target / 100 * (i / points) - v) * 0.012;
    out.push(v * 100);
  }
  out[0] = 0;
  out[points - 1] = target;
  return out;
}

// drawdown(eq), makeMonthlyMatrix(strategies, months, seed) — direct ports.
```

- [ ] **Step 2: Write the three fixture files**

```typescript
// in main() of the script
writeFixture("multi-strategy-equity.json", buildMultiStrategyBundle());
writeFixture("annotations.json", buildAnnotations());
writeFixture("monthly-returns.json", buildMonthlyReturns());
```

`buildAnnotations()` returns exactly the five handoff annotations
(Bull Flag, Volume Divergence, Liquidation Wall *(danger:true)*,
RSI Reset, Break of Structure) — copy the literal from
`chart-ai-annotation.jsx` `ANNOTATIONS`.

- [ ] **Step 3: Run the generator and verify idempotency**

```bash
cd frontend/web && npm run gen:chart-v2-fixtures
git diff frontend/web/src/components/chart/v2/__fixtures__/
npm run gen:chart-v2-fixtures      # second run
git diff frontend/web/src/components/chart/v2/__fixtures__/    # MUST be empty
```

Expected: first run creates the three files; second run produces no diff.

- [ ] **Step 4: Commit**

```bash
git add scripts/gen-chart-v2-fixtures.ts frontend/web/src/components/chart/v2/__fixtures__/multi-strategy-equity.json frontend/web/src/components/chart/v2/__fixtures__/annotations.json frontend/web/src/components/chart/v2/__fixtures__/monthly-returns.json
git commit -m "fixture(chart-v2): add multi-strategy-equity, annotations, monthly-returns"
```

### Task 7 — `Charts` sidebar entry (cookie-gated)

**Files:**
- Modify: `frontend/web/src/components/shell/Sidebar.tsx`
- Modify: `frontend/web/src/components/shell/Sidebar.test.tsx`
- Modify (only if `chart-pie` missing): `frontend/web/src/components/primitives/Icon.tsx`

- [ ] **Step 1: Write the failing tests**

```typescript
// frontend/web/src/components/shell/Sidebar.test.tsx — append
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { Sidebar } from "./Sidebar";

describe("Sidebar / Charts entry", () => {
  beforeEach(() => { document.cookie = ""; });

  it("hides Charts entry when xvn.chartv2 cookie is absent", () => {
    render(<MemoryRouter><Sidebar /></MemoryRouter>);
    expect(screen.queryByText("Charts")).toBeNull();
  });

  it("shows Charts entry between Scenarios and Eval when cookie is set", () => {
    document.cookie = "xvn.chartv2=1";
    render(<MemoryRouter><Sidebar /></MemoryRouter>);
    const items = screen.getAllByRole("link").map(a => a.textContent?.trim());
    const scenariosIdx = items.indexOf("Scenarios");
    const chartsIdx    = items.indexOf("Charts");
    const evalIdx      = items.indexOf("Eval");
    expect(chartsIdx).toBeGreaterThan(scenariosIdx);
    expect(chartsIdx).toBeLessThan(evalIdx);
  });
});
```

- [ ] **Step 2: Run to verify failure**

```bash
cd frontend/web && npm test -- shell/Sidebar
```

Expected: FAIL (Charts entry missing).

- [ ] **Step 3: Add the entry behind a cookie check**

```typescript
// frontend/web/src/components/shell/Sidebar.tsx — modify
function hasChartV2Cookie(): boolean {
  return typeof document !== "undefined" &&
    document.cookie.split(";").some(c => c.trim().startsWith("xvn.chartv2=1"));
}

const PRIMARY: Item[] = [
  { to: "/", label: "Dashboard", icon: "home" },
  { to: "/strategies", label: "Strategies", icon: "chart" },
  { to: "/agents", label: "Agents", icon: "user" },
  { to: "/scenarios", label: "Scenarios", icon: "list" },
  // Charts entry inserted at runtime when the cookie is set (B-rollout will
  // make this unconditional).
  { to: "/eval-runs", label: "Eval", icon: "bars" },
  { to: "/docs", label: "Docs", icon: "book" },
  { to: "/settings", label: "Settings", icon: "sliders" },
];

const CHARTS_ITEM: Item = { to: "/charts", label: "Charts", icon: "chart-pie" };

export function Sidebar({ className = "" }: { className?: string }) {
  const items = useMemo(() => {
    if (!hasChartV2Cookie()) return PRIMARY;
    const out = [...PRIMARY];
    const evalIdx = out.findIndex(i => i.to === "/eval-runs");
    out.splice(evalIdx, 0, CHARTS_ITEM);
    return out;
  }, []);

  // …existing JSX, but iterate over `items` instead of `PRIMARY`.
}
```

If `chart-pie` is not in `IconName`, add it:

```typescript
// frontend/web/src/components/primitives/Icon.tsx
export type IconName = /* …existing… */ | "chart-pie";
// add the SVG glyph mapping for chart-pie (any pie/donut path works; use lucide-react's "PieChart" SVG if available)
```

- [ ] **Step 4: Run tests to verify pass**

```bash
npm test -- shell/Sidebar
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/shell/Sidebar.tsx frontend/web/src/components/shell/Sidebar.test.tsx frontend/web/src/components/primitives/Icon.tsx
git commit -m "feat(sidebar): add Charts entry between Scenarios and Eval, cookie-gated"
```

### Task 8 — `/charts` route scaffolding + four placeholder shells

**Files:**
- Modify: `frontend/web/src/routes.tsx`
- Create: `frontend/web/src/routes/charts/ChartsLayout.tsx`
- Create: `frontend/web/src/routes/charts/ChartsOverview.tsx`
- Create: `frontend/web/src/routes/charts/ChartsCompare.tsx`
- Create: `frontend/web/src/routes/charts/ChartsAnnotated.tsx`
- Create: `frontend/web/src/routes/charts/ChartsHero.tsx`
- Create: `frontend/web/src/routes/charts/ChartsLayout.test.tsx`

- [ ] **Step 1: Write the failing route test**

```typescript
// frontend/web/src/routes/charts/ChartsLayout.test.tsx
import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { ChartsLayout } from "./ChartsLayout";
import { ChartsOverview } from "./ChartsOverview";

describe("/charts", () => {
  it("renders the Overview tab at /charts/overview with placeholder copy", () => {
    render(
      <MemoryRouter initialEntries={["/charts/overview"]}>
        <Routes>
          <Route path="/charts" element={<ChartsLayout />}>
            <Route path="overview" element={<ChartsOverview />} />
          </Route>
        </Routes>
      </MemoryRouter>
    );
    expect(screen.getByText("B1: Overview")).toBeInTheDocument();
    // subnav present
    expect(screen.getByRole("link", { name: /Overview/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Compare/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Annotated/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Hero/ })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify failure**

```bash
cd frontend/web && npm test -- routes/charts
```

Expected: FAIL (modules don't exist).

- [ ] **Step 3: Create the four placeholder shells**

```typescript
// frontend/web/src/routes/charts/ChartsOverview.tsx
import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";

export function ChartsOverview() {
  return <EmptyState title="B1: Overview" body="Dark Minimal Strategy Dashboard lands in milestone B1." />;
}
```

Repeat for `ChartsCompare` (`B2: Compare`), `ChartsAnnotated`
(`B3: Annotated`), `ChartsHero` (`B4: Hero`). Each is a 5-line file.

- [ ] **Step 4: Create the layout with subnav + outlet + index redirect**

```typescript
// frontend/web/src/routes/charts/ChartsLayout.tsx
import { NavLink, Outlet, Navigate, useLocation } from "react-router-dom";

const TABS = [
  { to: "overview",  label: "Overview" },
  { to: "compare",   label: "Compare" },
  { to: "annotated", label: "Annotated" },
  { to: "hero",      label: "Hero" },
];

export function ChartsLayout() {
  const { pathname } = useLocation();
  if (pathname === "/charts" || pathname === "/charts/") {
    return <Navigate to="/charts/overview" replace />;
  }
  return (
    <div className="flex flex-col h-full">
      <nav className="flex items-center gap-4 px-6 py-3 border-b border-border-soft">
        {TABS.map(t => (
          <NavLink
            key={t.to}
            to={t.to}
            className={({ isActive }) =>
              isActive
                ? "text-text border-b-2 border-gold pb-1"
                : "text-text-2 hover:text-text pb-1"
            }
          >
            {t.label}
          </NavLink>
        ))}
      </nav>
      <main className="flex-1 min-h-0 overflow-auto"><Outlet /></main>
    </div>
  );
}
```

- [ ] **Step 5: Mount the route**

```typescript
// frontend/web/src/routes.tsx — add to the existing router config
import { lazy } from "react";
const ChartsLayout    = lazy(() => import("./routes/charts/ChartsLayout").then(m => ({ default: m.ChartsLayout })));
const ChartsOverview  = lazy(() => import("./routes/charts/ChartsOverview").then(m => ({ default: m.ChartsOverview })));
const ChartsCompare   = lazy(() => import("./routes/charts/ChartsCompare").then(m => ({ default: m.ChartsCompare })));
const ChartsAnnotated = lazy(() => import("./routes/charts/ChartsAnnotated").then(m => ({ default: m.ChartsAnnotated })));
const ChartsHero      = lazy(() => import("./routes/charts/ChartsHero").then(m => ({ default: m.ChartsHero })));

// inside the routes array, sibling to other top-level shells:
{
  path: "/charts",
  element: <ChartsLayout />,
  children: [
    { path: "overview",  element: <ChartsOverview /> },
    { path: "compare",   element: <ChartsCompare /> },
    { path: "annotated", element: <ChartsAnnotated /> },
    { path: "hero",      element: <ChartsHero /> },
  ],
},
```

- [ ] **Step 6: Run the test to verify pass + manual smoke**

```bash
npm test -- routes/charts
npm run dev   # open / and visit /charts; verify subnav + placeholder
```

Expected: tests PASS; `/charts` redirects to `/charts/overview`; each tab shows its placeholder.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/routes.tsx frontend/web/src/routes/charts/
git commit -m "feat(routes): /charts layout with overview/compare/annotated/hero placeholders"
```

### Task 9 — Backend endpoint stub: `GET /api/v2/charts/dashboards/overview`

**Files:**
- Create: `crates/xvision-engine/src/api/charts_dashboards.rs`
- Modify: `crates/xvision-engine/src/api/mod.rs`
- Modify: `crates/xvision-dashboard/src/routes.rs`
- Create: `crates/xvision-engine/tests/charts_dashboards_overview.rs`

B0 ships a fixture-backed stub so B1 can wire UI without waiting for the real builder. B1 replaces the body with the real builder (pairs each Strategy with its latest backtest run's equity series, resolves `color` per §11.5 with rotation fallback).

- [ ] **Step 1: Write the failing handler test**

```rust
// crates/xvision-engine/tests/charts_dashboards_overview.rs
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn overview_returns_multi_strategy_equity_shape() {
    let app = xvision_engine::api::test_app().await;
    let resp = app
        .oneshot(Request::builder().uri("/api/v2/charts/dashboards/overview").body(Body::empty()).unwrap())
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["kind"], "multi_strategy_equity");
    assert!(body["time"].as_array().unwrap().len() >= 240);
    assert_eq!(body["strategies"].as_array().unwrap().len(), 5);
    for s in body["strategies"].as_array().unwrap() {
        assert!(s["color"].as_str().unwrap().starts_with('#'));
        assert!(s["equity"].as_array().unwrap().len() >= 240);
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-charts-b0"
cargo test -p xvision-engine --test charts_dashboards_overview
```

Expected: FAIL (route not mounted).

- [ ] **Step 3: Implement the stub handler**

```rust
// crates/xvision-engine/src/api/charts_dashboards.rs
use axum::{Json, Router, routing::get};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiStrategyEquityBundle {
    pub kind: &'static str,
    pub generated_at: u64,
    pub granularity: String,
    pub time: Vec<f64>,
    pub strategies: Vec<StrategyBundle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct StrategyBundle { /* fields per spec §6.1 */ }

pub fn router() -> Router {
    Router::new().route("/api/v2/charts/dashboards/overview", get(overview))
}

async fn overview() -> Json<MultiStrategyEquityBundle> {
    // B0 stub: return a deterministic 5-strategy fixture matching
    // multi-strategy-equity.json. B1 replaces this with the real
    // builder that pairs Strategy records with their latest backtest
    // run equity series and resolves color via Strategy.color → rotation.
    Json(stub_bundle())
}

fn stub_bundle() -> MultiStrategyEquityBundle {
    // Generate 240 daily points using mulberry32 seeded the same way as
    // the frontend fixture so a UI test against /api/… matches the
    // fixture rendering pixel-for-pixel.
    todo!("port mulberry32 + makeEquity from scripts/gen-chart-v2-fixtures.ts; see strategies::rotation::ROTATION for the 5-strategy palette")
}
```

> `todo!()` is the only acceptable placeholder in this step — fill it in within the same task by porting the generator from `scripts/gen-chart-v2-fixtures.ts`. The TS PRNG (mulberry32) maps directly to Rust `u32` arithmetic; verify outputs match for seed=1.

- [ ] **Step 4: Wire the router**

```rust
// crates/xvision-engine/src/api/mod.rs
pub mod charts_dashboards;
// in build_router(): .merge(charts_dashboards::router())
```

```rust
// crates/xvision-dashboard/src/routes.rs — confirm the engine's router is mounted under the existing API surface; no separate registration needed if the dashboard nests engine's full Router.
```

- [ ] **Step 5: Run the test to verify pass + parity**

```bash
cargo test -p xvision-engine --test charts_dashboards_overview
```

Plus a one-shot parity check: compare the stub's first 5 values for one strategy against the same indices in the frontend fixture file. They must match bit-for-bit (modulo JSON number formatting).

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/api/charts_dashboards.rs crates/xvision-engine/src/api/mod.rs crates/xvision-engine/tests/charts_dashboards_overview.rs
git commit -m "feat(api): /api/v2/charts/dashboards/overview stub returning fixture-matched bundle"
```

### Task 10 — `/chart-lab/dashboards` review tab

**Files:**
- Create: `frontend/web/src/routes/chart-lab/ChartLabDashboards.tsx`
- Modify: `frontend/web/src/routes/chart-lab/index.tsx`

- [ ] **Step 1: Add the tab definition + page**

```typescript
// frontend/web/src/routes/chart-lab/ChartLabDashboards.tsx
import { Link } from "react-router-dom";

const DASHBOARDS = [
  { slug: "overview",  label: "Overview (B1 — Dark Minimal)" },
  { slug: "compare",   label: "Compare (B2 — Comparison AB)" },
  { slug: "annotated", label: "Annotated (B3 — AI Annotation)" },
  { slug: "hero",      label: "Hero (B4 — Gradient Warm)" },
];

export function ChartLabDashboards() {
  return (
    <div className="p-6 space-y-3">
      <h2 className="text-lg font-serif">Dashboards</h2>
      <p className="text-text-2 text-sm">
        Track B canvases rendered full-bleed against their fixtures. Each one
        is the visual regression target for its B-milestone.
      </p>
      <ul className="space-y-2">
        {DASHBOARDS.map(d => (
          <li key={d.slug}>
            <Link to={`/chart-lab/dashboards/${d.slug}`} className="text-gold hover:underline">{d.label}</Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
```

- [ ] **Step 2: Add the tab to the chart-lab index**

```typescript
// frontend/web/src/routes/chart-lab/index.tsx — modify the TABS array
const TABS = [
  { to: "/chart-lab",            label: "Overview",   end: true },
  { to: "/chart-lab/primitives", label: "Primitives" },
  { to: "/chart-lab/surfaces",   label: "Surfaces" },
  { to: "/chart-lab/dashboards", label: "Dashboards" },   // NEW
  { to: "/chart-lab/tokens",     label: "Tokens" },
];
```

Mount the child route in whatever pattern the existing tabs use. Each `/chart-lab/dashboards/:slug` page is a 2-line wrapper that imports its B-milestone surface as a placeholder for now (`<EmptyState text="B1 not yet implemented" />`) and gets replaced as each B-milestone lands.

- [ ] **Step 3: Verify the route**

```bash
cd frontend/web && npm run dev   # visit /chart-lab; verify Dashboards tab; click each slug.
```

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/routes/chart-lab/
git commit -m "feat(chart-lab): add Dashboards tab linking to B1-B4 surface targets"
```

### Task 11 — Final verification gate

- [ ] **Step 1: Run the full Track-B-relevant test surface**

```bash
cd /Users/edkennedy/Code/xvision/.claude/worktrees/charts-section-b0
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-charts-b0"

# Backend
cargo test -p <owning-crate> --test strategies_color_roundtrip
cargo test -p xvision-engine --test charts_dashboards_overview
cargo test -p xvision-engine                # wider regression sweep
cargo clippy -p xvision-engine --tests -- -D warnings

# Frontend
cd frontend/web
npm run typecheck
npm test -- "chart|shell/Sidebar|routes/charts"
npm run build
```

All green.

- [ ] **Step 2: Smoke-check the running app**

```bash
# from worktree root
cd frontend/web && npm run dev
# in another shell, start the engine: xvn dashboard --dev
```

Verify:
- Without `xvn.chartv2=1` cookie: `Charts` entry not in sidebar; `/charts` redirects or shows the route (still mounted — gating is on the sidebar entry only).
- With `xvn.chartv2=1` cookie: `Charts` appears between `Scenarios` and `Eval`; clicking it lands on `/charts/overview` with the B1 placeholder; subnav switches between the four placeholders.
- `/chart-lab/dashboards` tab exists and lists the four B-milestones.

- [ ] **Step 3: Write the status note + open the PR**

Update `team/status/charts-section-b0.md` to `phase: pr-open` per the briefing template. Open the PR with body summarising tasks 1–10 and pointing at the spec.

```bash
gh pr create --title "feat(charts): B0 foundation — sidebar + routes + Strategy.color + tokens" --body "$(cat <<'EOF'
## Summary
- Adds `Charts` sidebar entry (cookie-gated) between Scenarios and Eval
- Mounts `/charts/{overview,compare,annotated,hero}` with placeholder shells
- Migration 034: `strategies.color TEXT` nullable; render layer falls back to strategyRotation palette by stable index
- Extends `Chart2ThemeDefinition` with warm palette + 8-strategy rotation + 5-step heat ramp + typography stack
- Loads Cormorant + Inter + JetBrains Mono; adds `.caps` utility
- Three new fixtures + extended `gen-chart-v2-fixtures.ts`
- New endpoint `/api/v2/charts/dashboards/overview` (fixture-backed stub; real builder in B1)
- New `/chart-lab/dashboards` review tab

## Spec
docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md — Track B / B0.

## Test plan
- [x] cargo test -p xvision-engine
- [x] npm run typecheck && npm test && npm run build
- [x] Manual: cookie on/off → sidebar entry toggles
- [x] Manual: /chart-lab/dashboards opens and lists the four B-milestones

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-review notes

- All eleven tasks have file paths, code blocks, commands, and commit lines — no `TODO`/`TBD` placeholders.
- The single `todo!()` in Task 9 Step 3 is intentional and gets filled inline within the same task, not deferred.
- Migration registry update is in Task 1 (reserved up-front per the conductor rule); the SQL file then references that row.
- Type names match across tasks: `MultiStrategyEquityBundle`, `AnnotatedChartPayload`, `Annotation`, `LiquidationHeatmapPayload`, `Chart2ThemeDefinition`, `Chart2WarmPalette`, `Chart2StrategyRotationEntry`, `Chart2HeatRamp`, `Chart2Typography`, `Chart2Radius`.
- Sidebar test asserts placement (between Scenarios and Eval) — the §11.1 locked decision is enforced by the test, not just documented.
- B0 explicitly does **not** ship Track B's chart bodies — those are B1–B4. Placeholders make the route topology testable now.
- B0 ships fixture-backed `/api/v2/charts/dashboards/overview` so B1 can start UI work the moment B0 merges; the real builder is one of B1's first tasks.
