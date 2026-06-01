# Creation-time "no filter / every-bar" warning — design

**Date:** 2026-06-01
**Status:** approved, ready for implementation
**Branch:** `feature/no-filter-creation-warning`

## Problem

When a user (or an agent) creates a strategy that has **no filter** — equivalently,
one whose `activation_mode == ActivationMode::EveryBar` — the strategy will dispatch
the LLM pipeline on **every bar/candle**. That burns tokens with no setup-selection
gate. We want to warn at **creation time**, in both the CLI and the UI, recommending
the user attach a deterministic filter so the strategy only acts on good setups.

"Created with no filter" and "activate every bar" are the **same underlying state**:
`activation_mode == EveryBar` (which requires `filter.is_none()`). The warning treats
them as one condition.

## What already exists (do not rebuild)

- `Strategy.activation_mode: ActivationMode` (`EveryBar` default vs `FilterGated`),
  `Strategy.filter: Option<Filter>`, and `Strategy.acknowledge_no_filter: bool`
  all exist (`crates/xvision-engine/src/strategies/mod.rs:142-163`).
- `validate::no_filter_warnings()` (`crates/xvision-engine/src/strategies/validate.rs:153`)
  already produces a token-cost warning — BUT it keys off **agent-graph topology**
  (a `Trader`/`Critic` agent with no upstream `Filter` edge) and returns **empty**
  for freshly-created strategies whose agent has `activates: None`. That is why
  creation is currently silent.
- `preflight_validate()` already folds `no_filter_warnings()` into its `warnings`.
- The CLI atomic create (`crates/xvision-cli/src/commands/strategy.rs` ~line 957-979)
  computes `preflight.warnings` but only emits them in `--json` mode; the plain path
  prints the bare strategy id with no warning.
- `acknowledge_no_filter` is the existing suppression contract, flipped by
  `xvn strategy create --no-filter-warning` / `xvn strategy edit --no-filter-warning`.

## Decisions (locked)

- **Non-blocking.** Create always succeeds; the warning is informational. Setting
  `acknowledge_no_filter: true` (via `--no-filter-warning`) suppresses it everywhere.
- **Condition = activation state**, not agent topology: warn iff
  `activation_mode == ActivationMode::EveryBar && !acknowledge_no_filter`.
- Fold the every-bar case into the existing validate/eval surfaces too (not only
  creation), so the same condition is reported consistently.
- CLI human warning goes to **stderr** so stdout stays the bare id (keeps
  `id=$(xvn strategy create …)` working).

## Design

### 1. Single source of truth — new helper in `validate.rs`

Add alongside `no_filter_warnings`:

```rust
/// Fires when the strategy will dispatch the LLM pipeline on every bar
/// (no deterministic filter gates it). Independent of agent-graph topology:
/// keys off `activation_mode`, which is what "no filter" / "activate every
/// bar" both mean. Suppressed by `acknowledge_no_filter`.
pub fn every_bar_warning(s: &Strategy) -> Option<String> {
    if s.acknowledge_no_filter {
        return None;
    }
    if s.activation_mode == ActivationMode::EveryBar {
        Some(format!(
            "Strategy '{}' has no filter and will activate on every bar — it runs \
             the LLM pipeline on every candle and burns tokens. Attach a \
             deterministic filter so it only acts on good setups. (Pass \
             --no-filter-warning / set acknowledge_no_filter to silence.)",
            s.manifest.display_name
        ))
    } else {
        None
    }
}
```

- Wording reuses the existing family ("burns tokens", "act on good setups").
- Downstream tooling treats warning strings verbatim — keep this string stable.
- Fold `every_bar_warning` into `preflight_validate`'s `warnings` (in addition to
  the existing `no_filter_warnings`), de-duplicating if both fire on the same
  strategy so the operator does not see two near-identical lines.

### 2. Wire into the three creation seams

| Surface | Seam (verify exact line during analyze) | Change |
|---|---|---|
| **CLI human create** | `xvision-cli/src/commands/strategy.rs` plain (non-`--json`) create path (~line 977) | After persisting, if `every_bar_warning(&strategy).is_some()` print `warning: <msg>` to **stderr**; stdout stays the bare id. The `--json` path already carries `warnings` — ensure it includes the every-bar case. |
| **Agent create (CLI/MCP)** | `authoring::create_strategy` (`xvision-engine/src/authoring.rs:234`), `api::strategy::create_strategy` (`xvision-engine/src/api/strategy.rs:1302`), and the MCP `xvn_create_strategy` / `xvn_strategy_create_atomic` tools | Add/populate a `warnings: Vec<String>` field on the create output so the agent is told to add a filter at creation. A blank draft is EveryBar + no filter by definition, so it should warn. Atomic create already returns `warnings`; make sure it also covers the every-bar case. |
| **UI create** | the SPA create-response renderer (chat tool-row / create form result in `frontend/web/src`) | Surface the returned `warnings` to the user (toast or inline notice) on create. Find the exact render site during analyze. |

### 3. Testing

- Unit: `every_bar_warning` returns `Some` for `EveryBar && !acknowledge_no_filter`,
  `None` when `acknowledge_no_filter` or `activation_mode == FilterGated`.
- CLI: plain `xvn strategy create` (no `--json`) writes the warning to **stderr** and
  the bare id to stdout; `--no-filter-warning` suppresses it.
- Authoring/API: create output `warnings` is populated for a default (every-bar)
  strategy and empty when acknowledged.
- Regression: confirm existing `no_filter_warnings`/`preflight_validate` tests still
  pass and no duplicate warning lines are emitted when both helpers fire.

### 4. Out of scope

- Changing the default `activation_mode` (stays `EveryBar`).
- Any blocking / required-acknowledgment gate (explicitly non-blocking).
- New filter-authoring UX. We only warn and point at the existing
  `--no-filter-warning` / filter-attach paths.

## Handoff

This spec is handed to `100x run` (analyze → implement → test → review). The analyze
stage maps the exact UI render call-site and every create seam; implement applies the
changes; test/review enforce the testing section above.
