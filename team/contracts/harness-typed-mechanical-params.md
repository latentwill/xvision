---
track: harness-typed-mechanical-params
lane: integration
wave: harness-observability-audit
worktree: .worktrees/harness-typed-mechanical-params
branch: task/harness-typed-mechanical-params
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies/mod.rs
  - crates/xvision-engine/src/strategies/store.rs
  - crates/xvision-engine/src/strategies/risk.rs
  - crates/xvision-engine/src/strategies/templates.rs
  - crates/xvision-engine/src/strategies/mechanical_params.rs   # NEW
  - crates/xvision-engine/tests/strategy_mechanical_params.rs   # NEW
  - crates/xvision-core/src/trading.rs
  - crates/xvision-core/src/risk.rs
  - crates/xvision-core/src/config.rs
  - team/contracts/harness-typed-mechanical-params.md
  - team/status/harness-typed-mechanical-params.md
  - team/board.md
forbidden_paths:
  - crates/xvision-engine/migrations/**            # F-3 territory, no migration needed here
  - crates/xvision-engine/src/eval/executor/**     # F-5 owns
  - crates/xvision-observability/**                # observability is settled
  - frontend/web/**                                # F-7 territory
interfaces_used:
  - xvision_engine::strategies::Strategy           # mechanical_params field replaced
  - xvision_engine::strategies::StrategyStore::save   # single pre-persist validator seam
  - xvision_core::trading::InternBriefing
  - xvision_core::trading::TraderDecision
  - xvision_core::risk::RiskDecision
  - xvision_core::config::RiskConfig / RiskCaps / Limits / Stops
parallel_safe: true
parallel_conflicts:
  - trace-dock-simple-advanced-toggle   # F-7 — different files entirely (frontend); listed for clarity
verification:
  - cargo build -p xvision-engine -p xvision-core
  - cargo test -p xvision-engine
  - cargo test -p xvision-core
  - cargo test -p xvision-engine --test strategy_mechanical_params
acceptance:
  - `Strategy.mechanical_params: serde_json::Value` at `crates/xvision-engine/src/strategies/mod.rs:59` is replaced by a typed enum `MechanicalParams` keyed on `manifest.template`. One variant per canonical template lives in a new `crates/xvision-engine/src/strategies/mechanical_params.rs`; each variant carries its own `#[serde(deny_unknown_fields)]` typed struct (one struct per template — `ema_cross`, `donchian_breakout`, `rsi_meanreversion`, plus whatever else `templates.rs` ships). A `Custom(serde_json::Value)` fallback is preserved for templates the operator hasn't migrated yet, so the JSON wire format is backwards-compatible with existing strategy bundles on disk.
  - The discriminator is `manifest.template` (the string id already on `Strategy`). Serde uses `#[serde(tag = "_template_kind", content = "params")]` or an equivalent externally-tagged scheme that keeps the JSON shape readable when persisted to `strategy_bundles`. Pre-existing bundles that don't carry the tag fall through to `Custom(Value)` — never panic, never crash a load.
  - `#[serde(deny_unknown_fields)]` is added to **`InternBriefing` (`xvision-core/src/trading.rs`), `TraderDecision` (same file), `RiskDecision` (`xvision-core/src/risk.rs`), `RiskConfig` / `Limits` / `Stops` / `RiskCaps` (`xvision-core/src/config.rs`)**. Each addition is paired with a test that asserts an unknown field on the input fails deserialization — F-6 makes silent prompt-typo bugs (e.g. `tp_pct` vs `take_profit_pct`) loud at the boundary instead of silently dropping.
  - Cross-field invariants land via `garde`'s custom validators where the existing field-level discipline is already in place. The audit calls out **TP > SL for long positions** as the canonical example — implement that one on the relevant template's risk-side mechanical params; document the validator path in the contract notes for future invariants to follow. Don't backfill exhaustive invariants — one per known footgun is enough to pin the pattern.
  - `StrategyStore::save` (`crates/xvision-engine/src/strategies/store.rs`) gets a single pre-persist validate seam. It calls `MechanicalParams::validate()` (a method that delegates to the variant's own `garde` `Validate` impl) before writing the bundle. Errors surface as a typed `StrategyStoreError::InvalidMechanicalParams(garde::Report)` — the dashboard / CLI catches it and renders the operator-readable diagnostic. **No other call site** validates mechanical_params; the single seam is the contract's load-bearing simplification.
  - Existing call sites that read `Strategy.mechanical_params` (the indicator-period inference at `mod.rs:84`, the wizard scaffolding at `mod.rs:148`, the test fixtures at `mod.rs:238/261/290/309`) keep compiling. The simplest path: keep a `pub fn mechanical_params_json(&self) -> serde_json::Value` accessor on `MechanicalParams` that serializes the variant back to the existing JSON shape. Existing readers call the accessor; new readers can match on the variant for typed access.
  - New test file `crates/xvision-engine/tests/strategy_mechanical_params.rs` covers:
    - Round-trip serialize/deserialize for each typed variant + the `Custom(Value)` fallback.
    - Unknown-field rejection: deserialize a payload with a typo on a known template fails (proves `deny_unknown_fields` is active).
    - Backwards-compat: a JSON blob in the pre-F-6 untagged shape lands as `Custom(Value)`.
    - `StrategyStore::save` rejects an invalid `MechanicalParams` payload with `InvalidMechanicalParams`; a valid one round-trips through save+load.
    - Cross-field invariant (TP > SL example) fires on the right template and passes on a valid input.
  - No new schema migration. No change to the `strategy_bundles` SQLite column type. No change to API wire format beyond the addition of `_template_kind` on per-template payloads (which the dashboard ignores until F-6's frontend follow-up wires a typed editor — out of scope here).
  - No backwards-compatibility shim for the deprecated `setup_id` / `StrategyBundle` names — those are pre-launch breaking changes that already shipped.
  - `cargo test -p xvision-engine` and `cargo test -p xvision-core` both green. No regression in the existing strategy fixture round-trip tests.
---

# Scope

Implements F-6 from the 2026-05-18 harness observability audit
(`team/intake/2026-05-18-harness-observability-audit.md`).

The audit flagged
`Strategy.mechanical_params: serde_json::Value`
(`crates/xvision-engine/src/strategies/mod.rs:59`) as an **untyped
escape hatch**: template-specific parameters skip all validation,
typos in operator prompts (e.g. `tp_pct` instead of
`take_profit_pct`) silently drop, and there's no place to express
cross-field invariants (e.g. TP > SL for long positions).

`InternBriefing` and `RiskConfig` also lack
`#[serde(deny_unknown_fields)]`, so an unknown field on either type
silently deserializes — the operator never learns their input was
ignored. The audit notes field-level `garde` discipline is already
strong (~50% landed); the gap is the **unified pre-persist pass**.

F-6 closes both gaps:

1. Type `mechanical_params` per template via a new `MechanicalParams`
   enum keyed on `manifest.template`. One variant per canonical
   template, each with its own typed struct + `deny_unknown_fields`.
   A `Custom(Value)` fallback keeps existing on-disk bundles loading
   without crash.
2. Add `deny_unknown_fields` to `InternBriefing`, `TraderDecision`,
   `RiskDecision`, `RiskConfig`/`Limits`/`Stops`, `RiskCaps`. Each
   addition is paired with a test that proves unknown-field
   rejection.
3. Land cross-field invariants via `garde` custom validators. The
   audit's canonical example — TP > SL for long positions — gets the
   first invariant; the pattern is documented for follow-ups.
4. Wire a single pre-persist validate seam in `StrategyStore::save`.
   Errors surface as `StrategyStoreError::InvalidMechanicalParams(garde::Report)`.

This is the "make typos loud, make invariants checkable" track. F-6
does not change the dashboard UI (a typed wizard editor is a
follow-up); it does not change the SQLite schema (mechanical_params
is already a JSON-typed column). The wire format gains one tag field
on per-template payloads but pre-F-6 untagged blobs still load via
`Custom(Value)`.

Reference: 2026-05-18 harness audit intake, finding F-6.

# Out of scope

- A typed wizard editor on the dashboard. F-6 ships the typed model;
  the editor that consumes it is a separate frontend track.
- An exhaustive cross-field invariant catalogue. F-6 implements the
  TP > SL example as a pattern. Each new invariant lands in its own
  PR with the operator's rationale.
- A migration to rewrite existing on-disk bundles into the typed
  shape. They keep loading as `Custom(Value)` and get re-saved into
  the typed shape on the next operator-driven edit; no batch
  rewrite.
- Replacing `garde` with a different validator. The intake explicitly
  rules this out — field-level discipline already works.
- Changes to `xvision-observability` or any span emission. F-6's
  validator runs at persistence, not at every span emit. The
  `tool.validate_input` / `tool.validate_output` spans F-4 added
  remain no-op brackets; F-6 doesn't fill those bodies either (that's
  a separate engine-level question about which tool inputs to revalidate
  at every call vs. trust the persisted strategy contract).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-typed-mechanical-params status
git -C .worktrees/harness-typed-mechanical-params log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/harness-typed-mechanical-params
#   - base is up to date with origin/main
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-typed-mechanical-params \
  -b task/harness-typed-mechanical-params origin/main
```

# Notes

Append checkpoints / PR links below.
