---
title: Full Orderly market coverage — data-driven asset registry (~98 markets)
status: in-progress
beads-epic: xvision-3x0
branch: feat/orderly-full-market-coverage
date: 2026-06-09
plan-version: v2
gate: plan-review v1 returned 3x FAIL; v2 incorporates all blockers
user-approved: false
---

# Full Orderly market coverage (plan v2)

## Goal

Make every active Orderly perp/RWA market (98 in the live snapshot) usable across
strategy/scenario asset selection, symbol mapping, and Orderly execution — without
per-asset Rust edits ever again.

## Root problem

`AssetSymbol` (`crates/xvision-core/src/trading.rs:37`) is a closed 15-variant
`Copy` enum; `FromStr` rejects anything outside the Alpaca crypto whitelist; venue
mappings are hardcoded and *require* an Alpaca symbol; the frontend picker is a
hardcoded array. So `HYPE`, `SPX500`, `SPY_MYTHOS` cannot exist.

## Architecture (v2 — revised after plan-review FAIL)

### Core type: interned `Copy` newtype that PRESERVES legacy variant names

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetSymbol(&'static str);

impl AssetSymbol {
    // Legacy names retained as const associated constants — every existing
    // `AssetSymbol::Btc` call site (~350 across src/ + tests) keeps compiling.
    #[allow(non_upper_case_globals)] pub const Btc: AssetSymbol = AssetSymbol("BTC");
    // … Eth, Ltc, Sol, Avax, Link, Aave, Uni, Dot, Doge, Shib, Matic, Bch, Usdt, Usdc
    pub const fn from_static(s: &'static str) -> Self { Self(s) }
    pub fn as_str(self) -> &'static str { self.0 }
}
```

- **Eq/Hash/Ord are value-based** (`&str` derefs to `str`), so a `const Btc` ("BTC")
  equals an interned "BTC" from a DB row. `const ORDERLY_SUPPORTED` (orderly.rs:72)
  and all `AssetSymbol::Btc` sites compile unchanged → **W7 collapses to near-zero.**
- **Interner** (`Mutex<HashSet<&'static str>>` + `Box::leak`) only for runtime strings
  (Deserialize / FromStr). **Bounded:** intern only validates-format-then-checks-set
  before leaking; an explicit guard rejects junk so LLM/DB-supplied garbage can't leak
  unboundedly (feasibility #6).
- **Serde:** custom impls emit/accept a **bare string scalar** "BTC" (NOT
  `serialize_newtype_struct`/tuple) so `BTreeMap<AssetSymbol,_>` JSON map keys keep
  working (feasibility #5); Deserialize interns. Wire form identical → no migration
  (DB stores asset as TEXT — confirmed `migrations/002_eval.sql:35`).
- **Ord change** (discriminant → lexicographic): audit & fix tests asserting order
  (`asset_set.rs`, multi_asset tests).

### `FromStr` permissive — and the gates that depended on its `Err`

`FromStr` becomes: uppercase, trim, base-before-`/`, validate `^[A-Z0-9_]+$`, intern;
**format errors still error** (empty/bad chars). Three gates currently abuse
`from_str(..).is_ok()` / `unwrap_err()` as a *whitelist* check — these MUST be
re-pointed at the registry (NOT FromStr):
- `crates/xvision-execution/src/broker_surface.rs:43` `is_alpaca_crypto()` — **live
  order-routing gate** (feasibility #2, BLOCKER). Re-implement against the Alpaca
  registry so HYPE/SPX500 don't get routed down the Alpaca crypto path.
- `crates/xvision-engine/src/eval/executor/asset_set.rs:50` `rejects_unparseable_universe_symbol`
  — universe validation should reject *non-whitelisted* symbols via the registry.
- `crates/xvision-execution/src/alpaca.rs:765` XRP `unwrap_err()` test — update to
  registry-based assertion.

### Taxonomy — REMOVE the correlation-cluster rule (user decision 2026-06-09)

`correlation_cluster` rule (cap **2**, config.rs:719) vetoes >2 open positions sharing
`cluster_of()`. User decision: **delete this rule** — correlation belongs to the agent,
not an uncontrollable internal veto. This also resolves the taxonomy concern: with the
rule gone, `cluster` is no longer a risk input, so it becomes a single **coarse
category** field (`crypto`/`stable`/`meme`/`rwa`/`equity`/`index`/`commodity`/
`orderly-broker`) used only for generator-grouped comments + UI picker grouping.
- Delete `crates/xvision-risk/src/rules/correlation_cluster.rs`, its registration in
  `with_default_rules` (lib.rs), its module export, and its tests.
- Remove `max_correlation_cluster` from `RiskConfig` (serde ignores the now-unknown key
  in existing `risk.toml`/configs — no breakage). Drop the key from checked-in `*.toml`.
- `cluster_of()` may remain (harmless) or be repurposed for category lookup.
- Existing 15 assets' `cluster` values become categories (btc→crypto, usdt→stable, …).

### Venue mapping = whitelist data; registry-less fallback for unit tests

Process-global registry in `xvision-core` (`asset_registry.rs`), populated when the
whitelist loads (risk→core dep direction is clean — no cycle, feasibility #7).
`orderly_symbol()/alpaca_pair()/cluster()/category()` read it. Registry-less fallback
(`PERP_{SYM}_USDC`, `{SYM}/USD`) keeps crypto execution/alpaca unit tests green without
loading config; **loaded data is authoritative** (Orderly-only → `alpaca_pair()==None`;
broker-suffixed → exact venue symbol). Loader **asserts no duplicate base key**
(completeness #5).

### XVN base-symbol naming (deterministic; verified zero collisions across 98 symbols)

Strip `PERP_` + `_USDC` → base (`PERP_HYPE_USDC`→`HYPE`, `PERP_1000PEPE_USDC`→`1000PEPE`,
`PERP_H_USDC`→`H`, `PERP_S_USDC`→`S`). Broker-suffixed keep distinct base + EXACT venue
symbol: `PERP_SPY_USDC_mythos`→`SPY_MYTHOS`, `_QQQ_`→`QQQ_MYTHOS`, `_EWY_`→`EWY_MYTHOS`,
`_SNDK_`→`SNDK_MYTHOS`, `_SOXL_`→`SOXL_MYTHOS`, `PERP_NATGAS_USDC_arthur`→`NATGAS_ARTHUR`.
`SPX` and `SPX500` are distinct bases (never dedupe by display name). Plan-review
confirmed **0 base collisions** in the 98-symbol snapshot.

### Legacy/snapshot merge (requirement #7)

Generator MERGES overlapping assets (BTC,ETH,SOL,AVAX,LINK,AAVE,UNI,DOT,DOGE,BCH appear
in both) — preserve legacy `alpaca` + `cluster`, set `orderly` from snapshot, single key.
Legacy-only assets absent from snapshot (MATIC,SHIB,USDT,USDC,LTC) are PRESERVED as-is
with existing mappings (req #7); their Orderly symbols may be inactive on Orderly (e.g.
MATIC→POL rebrand) — documented, not remapped, to preserve existing behavior.

### Data sourcing boundary (DECIDED 2026-06-09: backtest = Alpaca-data only)

Orderly-only assets have no Alpaca bar history; `live_config.rs:190`, `scenario.rs:303`,
`chart.rs:1354` gate on the Alpaca whitelist. This plan makes the 98 markets
**selectable, mappable, and Orderly-executable**, and marks each registry entry with a
`data` source (`alpaca` | `orderly-only`). **Backtest/scenario/chart asset selection
filters to `data == alpaca`** (the picker hides/disables Orderly-only assets in those
contexts with a clear "no backtest data" badge); Orderly/live selection shows all 98.
**Orderly historical-bar integration is OUT of scope** (future effort — user will
design how to bring additional backtest data sources in).

## Work units (v2)

| ID | Scope | Depends |
|----|-------|---------|
| W0 | **Remove `correlation_cluster` rule**: delete rule file + registration + module export + tests; drop `max_correlation_cluster` from `RiskConfig` + checked-in `*.toml`. Verify risk suite green. | — |
| W1 | `AssetSymbol` newtype + legacy const variants + `from_static` + bounded interner + scalar serde + permissive FromStr (format-only) + `asset_registry.rs` (orderly/alpaca/cluster[=category]/data lookups + fallback). xvision-core. TDD. | — |
| W2 | Whitelist structs: `alpaca` optional, `cluster` = coarse category, add `data` source; loader installs registry + asserts no dup base. Re-point universe validation (`asset_set.rs`) at registry. xvision-core config, xvision-risk whitelist, xvision-data asset_whitelist. | W0,W1 |
| W3 | Execution + routing via registry+fallback: `orderly.rs`, `alpaca.rs`, **`broker_surface.rs::is_alpaca_crypto`** re-pointed at registry, **`api/chart.rs`** call sites. Resolution tests (HYPE, SPX500, XAU, NVDA, TSLA, broker-suffixed exact, BTC/ETH/SOL unchanged, Orderly-only alpaca=None). | W1,W2 |
| W4 | `whitelist.toml` 98 markets (merged) + `scripts/gen-orderly-assets` generator + `tests/fixtures/orderly_info_snapshot.json` + completeness test (exact count from fixture, every snapshot symbol present once). | W2 |
| W5 | Backend `GET /api/assets` (symbol, cluster[=category], data, venues, enabled) + ts-rs export. | W2 |
| W6 | Frontend: drop hardcoded `lib/assets.ts ALPACA_ASSETS` + `scenarios-detail.tsx CHART_PREVIEW_ASSET_OPTIONS`; fetch `/api/assets`; rework authoring picker (`authoring.tsx`) into inline searchable/category-grouped pick list (NO popover/modal/right-box); **backtest/scenario contexts filter to `data==alpaca`** with a "no backtest data" badge on Orderly-only. | W5 |
| W7 | Residual fixture/ordering fixups (only sites the const-variant approach can't cover: Ord-order asserts, the 3 reject-based tests). | W1,W2,W3 |
| W8 | (Optional, cuttable) Settings "Refresh markets from Orderly" → writes runtime-volume whitelist, on-demand, no polling. | W4,W5 |

Graph: `W0` independent (do first/parallel); `W1 → W2 → {W3,W4,W5}`; `W5 → W6`; `W7` trails W1–W3.

## Coverage note

`.coverage-thresholds.json` demands 100/100/100/100 via tarpaulin (blocking). main is
almost certainly not at 100% today. Target: thorough unit tests on all new W1–W3 Rust
logic (every fallback/None/category branch); the generator script + frontend are not
Rust-tarpaulin-covered. Will report actual `-p` package coverage; will NOT block the
deliverable on a workspace-wide 100% that main doesn't meet — flag to user if enforced.

## Guardrails

No deploy / no Docker / local only. Worktree `.worktrees/orderly-markets`,
`CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-orderly"`. `AssetSymbol` is NOT in the
2026-05-10 terminology lock — restructuring it needs no rationale doc. Frontend
no-popups / no-right-box / dark-mode rules apply to W6.
