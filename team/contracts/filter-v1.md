---
track: filter-v1
lane: foundation
wave: filter-v1
worktree: .worktrees/filter-v1
branch: task/filter-v1
base: origin/main
status: ready
depends_on:
  - executor-refactor
blocks:
  - filter-v1-backtest-evaluator
  - filter-v1-export-and-summary
  - filter-v1-frontend-types-and-panels
  - filter-v1-regression-fixtures
stacking: none
allowed_paths:
  - crates/xvision-filters/Cargo.toml
  - crates/xvision-filters/src/lib.rs
  - crates/xvision-filters/src/types.rs
  - crates/xvision-filters/src/parse.rs
  - crates/xvision-filters/src/validate.rs
  - crates/xvision-filters/src/errors.rs
  - crates/xvision-filters/tests/parse_roundtrip.rs
  - crates/xvision-filters/tests/validate_codes.rs
  - crates/xvision-filters/tests/fixtures/**
  - Cargo.toml
  - docs/superpowers/specs/2026-05-21-filter-v1.md
  - docs/superpowers/plans/2026-05-21-filter-v1.md
forbidden_paths:
  - crates/xvision-engine/**
  - crates/xvision-cli/**
  - crates/xvision-dashboard/**
  - crates/xvision-mcp/**
  - crates/xvision-memory/**
  - crates/xvision-indicators/**
  - frontend/**
  - team/board.md
  - team/board-v2.md
  - team/MANIFEST.md
  - decisions/**
interfaces_used:
  - serde::{Serialize, Deserialize}
  - thiserror::Error
  - ts_rs::TS
  - ulid::Ulid
  - chrono::{DateTime, Utc}
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build --workspace
  - cargo test -p xvision-filters
  - cargo clippy -p xvision-filters -- -D warnings
  - cargo fmt -p xvision-filters --check
  - bash scripts/board-lint.sh
acceptance:
  - crates/xvision-filters crate compiles standalone with no engine dependency
  - Filter TOML form parses to Filter struct; JSON form parses identically; round-trip Filter → TOML → Filter is byte-identical for the spec's example
  - Filter TOML form parses to Filter struct; JSON form parses identically; round-trip Filter → JSON → Filter is byte-identical
  - All 6 indicators (ema_n, sma_n, rsi_n, atr_n, atr_pct_n, close) parse with valid n values; invalid n returns E_FILTER_UNKNOWN_INDICATOR with field path
  - All 8 operators (>, <, >=, <=, ==, crosses_above, crosses_below, between) parse; unknown op returns E_FILTER_UNKNOWN_OPERATOR with field path
  - Numeric operand for `crosses_above` / `crosses_below` (where both sides should be Indicator) returns E_FILTER_OPERAND_TYPE with field path
  - Range operand outside `between` returns E_FILTER_OPERAND_TYPE; non-Range operand with `between` returns E_FILTER_OPERAND_TYPE
  - Range(lo, hi) with lo >= hi returns E_FILTER_RANGE_ORDER with field path
  - RSI threshold outside [0, 100] returns E_FILTER_NUMERIC_BOUNDS; ATR% ≤ 0 returns E_FILTER_NUMERIC_BOUNDS; per-indicator bounds documented in src/validate.rs
  - Future-bar index reference (parsed as +N on IndicatorRef) returns E_FILTER_FUTURE_LEAK; v1 has no such syntax so the test constructs the struct directly and confirms the validator rejects it
  - cooldown_bars < 0 returns E_FILTER_COOLDOWN_NEG (note: u32 prevents this at the type level; test exercises the deserializer rejecting negative integers from JSON/TOML)
  - max_wakeups_per_day < 1 or > 1440 returns E_FILTER_WAKEUP_CAP with field path
  - asset_scope with 0 or > 1 entries returns E_FILTER_ASSET_SCOPE with field path
  - ConditionTree::All(empty) and ConditionTree::Any(empty) return E_FILTER_EMPTY_TREE with field path
  - All 10 error codes from the spec's validation table have a matching test in tests/validate_codes.rs
  - Every error variant exposes a pub fn code(&self) -> &'static str that returns the stable E_FILTER_* string
  - Every error variant exposes a pub fn field_path(&self) -> &str returning a JSON pointer (e.g. "/conditions/all/2/rhs")
  - ts-rs derives compile and produce ts files under target/ts-rs/ (or equivalent workspace target); no schema drift flagged by cargo build
  - cargo test -p xvision-filters passes with zero failures
  - cargo clippy -p xvision-filters -- -D warnings passes with zero warnings
  - bash scripts/board-lint.sh passes
---

# Scope

Stand up the `xvision-filters` crate as a pure, engine-independent
data-model + DSL parser + validator for the Filter v1 entity described
in `docs/superpowers/specs/2026-05-21-filter-v1.md`. This is
Stage 1 of the 5-stage plan at
`docs/superpowers/plans/2026-05-21-filter-v1.md`.

The deliverable is a standalone workspace crate that:

1. Defines the Filter data model (`Filter`, `ConditionTree`,
   `Condition`, `Operand`, `IndicatorRef`, `Operator`, plus the supporting
   enums `ActivationMode`, `ScanCadence`, `WakeInPosition`, `FilterStatus`,
   `AgentContextTemplateId`).
2. Provides `parse_toml(&str) -> Result<Filter>` and
   `parse_json(&str) -> Result<Filter>` returning typed `ParseError` values
   with field-path context.
3. Provides `validate(&Filter) -> Result<(), ValidationError>` that
   implements the 10 rules from the spec. Each rejection carries a stable
   `E_FILTER_*` error code and a JSON-pointer field path so the eventual
   UI (Stage 4) can render targeted help.
4. Exposes ts-rs derives so Stage 4's `frontend/web/src/api/types.gen/`
   regenerates cleanly.

The crate has zero behavior. No engine wiring, no migrations, no
backtest hook, no exports, no frontend changes. Those are Stages 2–5.

## Track dependency

This contract depends on `executor-refactor` only at the **wave-sequencing
level** — Stage 1 is a pure crate add and could in principle land before
the executor refactor. However, Stage 2 of the Filter v1 plan wires
evaluation into the unified `Executor` produced by `executor-refactor`,
so the wave conductor sequences this Stage-1 contract behind
`executor-refactor` to keep the work flowing forward in dependency order.
If `executor-refactor` slips, Stage 1 may still ship to unblock
parallelism; the dependency declaration above is the conductor's signal,
not a hard build-time block.

# Out of scope

Explicit list of things this contract will NOT touch:

- **`crates/xvision-engine/**`** — Stage 2 wires the runtime; this contract
  does not.
- **Any indicator math** — Stage 2 introduces `crates/xvision-filters/src/indicators.rs`
  with the math for the 6 v1 indicators. Stage 1 only needs the *names*
  (`ema_n`, `sma_n`, etc.) typed in the parser/validator; no computation.
- **`xvision-indicators` extraction** — v1.5 chore per the spec's "Out of
  scope" section. Stage 2 keeps the math local to `xvision-filters`.
- **Migrations** — Stage 2 claims the next migration number. Stage 1's
  Filter struct is in-memory only; no persistence yet.
- **Frontend** — Stage 4. ts-rs derives produce the `.ts` files but
  consumers don't exist yet.
- **LLM-backed filters, `SlotRuntime::Llm`, `AgentKind`, `Expr`, edge
  graph** — all v1.5. The spec carves these out explicitly.
- **DSPy / DSRs dependency** — v1.5 per
  `team/intake/2026-05-21-dspy-dsrs-optimizer-adoption.md`. Stage 1 does
  not add `dspy-rs` to the workspace.
- **`vwap`, Bollinger, MACD, multi-symbol scope, CompiledRules
  activation** — all out of v1 per the spec.

If any of the above prove necessary mid-PR, push a contract-update PR
before any code PR.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/filter-v1 status
git -C .worktrees/filter-v1 log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/filter-v1
#   - base is up to date with origin/main (or rebase planned)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/filter-v1 -b task/filter-v1 origin/main
```

# Error-code stability contract

The following codes are part of the public surface of the crate. They are
returned by `validate()` and may be matched on by Stage 4's frontend
error-rendering code. Renaming any of these requires a coordinated PR
across stages and a contract update.

| Code | Rule |
|---|---|
| `E_FILTER_UNKNOWN_INDICATOR` | indicator name not in v1 catalog or n out of range |
| `E_FILTER_UNKNOWN_OPERATOR` | operator not in v1 catalog |
| `E_FILTER_OPERAND_TYPE` | operator/operand type mismatch (Range vs Numeric vs Indicator) |
| `E_FILTER_RANGE_ORDER` | Range(lo, hi) with lo >= hi |
| `E_FILTER_NUMERIC_BOUNDS` | indicator-specific numeric bound violated (RSI 0-100, ATR%>0, etc.) |
| `E_FILTER_FUTURE_LEAK` | reference to a future bar (reserved for v1.5 plugins; v1 rejects at construction) |
| `E_FILTER_COOLDOWN_NEG` | cooldown_bars < 0 (caught at deserialization, not by validator) |
| `E_FILTER_WAKEUP_CAP` | max_wakeups_per_day outside [1, 1440] |
| `E_FILTER_ASSET_SCOPE` | asset_scope length != 1 in v1 |
| `E_FILTER_EMPTY_TREE` | All(empty) or Any(empty) |

# Notes

Free-form. Append checkpoints, surprises, links to PRs. Do not edit
history above the line.

- 2026-05-21 — contract drafted by intake author. Stage 2 contract will be
  drafted after this Stage 1 PR opens, not before (avoid speculative
  contracts on a moving spec).
- 2026-05-21 — renamed from `watcher-v1-shape` to `filter-v1` per the
  operator-decided Watcher→Filter terminology pivot. Filename matches
  spec + plan filenames for the v1 wave.
- v1.5 follow-up (LLM filters + DSPy/DSRs optimizer hook) is tracked
  via `team/intake/2026-05-21-dspy-dsrs-optimizer-adoption.md`. It does
  not block this contract.
