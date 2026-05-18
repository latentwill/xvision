---
track: harness-typed-mechanical-params
worktree: .worktrees/harness-typed-mechanical-params
branch: task/harness-typed-mechanical-params
phase: claimed
last_updated: 2026-05-18T00:00:00Z
owner: claude-f6
---

# What I'm doing right now

Claimed F-6 (harness-typed-mechanical-params) from the 2026-05-18
harness observability audit. Worktree at
`.worktrees/harness-typed-mechanical-params`, branch
`task/harness-typed-mechanical-params`, base `origin/main`.
`CARGO_TARGET_DIR` set to the shared `$HOME/.cargo-target/xvision`
cache so the worktree doesn't grow its own `target/`.

Next step: build the typed `MechanicalParams` enum in
`crates/xvision-engine/src/strategies/mechanical.rs` with per-template
variants and a `Custom(serde_json::Value)` fallback, then wire the
`manifest.template`-driven serde discriminator on `Strategy` so the
existing on-disk JSON shape parses unchanged.

# Blocked on

Nothing. F-6 is parallel-safe with F-2/F-3/F-4/F-5/F-7 — disjoint
files. The F-5 contract carves out
`crates/xvision-engine/src/strategies/**` as F-6's territory.

# Next up

1. Create worktree + push the branch so board-lint passes with status
   `claimed`.
2. Land the typed enum + serde discriminator on `manifest.template`,
   keeping wire-format byte-identical for canonical templates.
3. Add `deny_unknown_fields` to the four trading-side payload types
   listed in the contract.
4. Cross-field garde validators (TP > SL on long `TraderDecision`;
   `RiskStops.stop_loss_min_pct <= stop_loss_max_pct`).
5. Consolidate validate to a single seam in `StrategyStore::save`.
6. Tests per the contract's `acceptance` section.
