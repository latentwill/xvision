---
track: risk-sees-conviction
lane: leaf
wave: eval-honesty-tail-2026-05-22
worktree: .worktrees/risk-sees-conviction
branch: task/risk-sees-conviction
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-core/src/trading.rs
  - crates/xvision-engine/src/safety/gate.rs
  - crates/xvision-engine/src/api/safety/routes.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/tests/risk_sees_conviction.rs
  - crates/xvision-engine/src/agents/model.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/safety/limits.rs
  - frontend/web/**
interfaces_used:
  - xvision_core::trading::TraderDecision (add `conviction: f32` field, 0.0–1.0, default 0.5)
  - xvision_engine::safety::gate::RiskGate (read `conviction`, never enforce)
  - Trader prompt schema (declare `conviction` as optional 0–1)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine --test risk_sees_conviction
  - cargo test -p xvision-engine
  - cargo test -p xvision-core
acceptance:
  - `TraderDecision.conviction: f32` exists (range 0.0–1.0, default 0.5 for missing field via `#[serde(default)]`)
  - Risk gate has access to the field; default user-authored risk configs ignore it
  - Documentation in the prompt schema (and the schema-drift test fixture) lists `conviction` as optional
  - Existing serialized `TraderDecision` blobs without `conviction` deserialize cleanly (no migration; `serde default` covers it)
  - **Never enforced** — risk gate must not reject a decision based on `conviction` value alone
---

# Scope

Expose `conviction` as a `TraderDecision` field so user-authored
risk configs can scale sizing if they choose. Never enforced by the
default risk gate — it's a piece of data the trader can volunteer
that downstream policies are free to ignore or use.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
row "Expose `conviction` to the risk layer so user-authored risk
configs can scale sizing if they choose; never enforced."

# Out of scope

- Default conviction-based sizing in the risk gate (operator-authored only)
- Conviction-based gating / vetoing (explicitly forbidden by intake)
- Trader prompt-template changes to require conviction (optional always)

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/risk-sees-conviction -b task/risk-sees-conviction origin/main
```

# Notes

`TraderDecision` shape is at `crates/xvision-core/src/trading.rs:196`.
Pattern after the `asset: Option<AssetSymbol>` field (line 219) for
the additive-with-serde-default approach so legacy blobs parse.
Add a `#[garde(range(min = 0.0, max = 1.0))]` clamp.
