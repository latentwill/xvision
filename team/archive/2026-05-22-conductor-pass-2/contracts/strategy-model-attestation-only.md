---
track: strategy-model-attestation-only
lane: leaf
wave: eval-honesty-tail-2026-05-22
worktree: .worktrees/strategy-model-attestation-only
branch: task/strategy-model-attestation-only
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies/manifest.rs
  - crates/xvision-engine/src/strategies/slot.rs
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/src/strategies/store.rs
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/api/strategy.rs
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/tests/strategy_attested_with.rs
  - frontend/web/src/components/strategy/StrategyForm.tsx
  - frontend/web/src/api/types.gen/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agent/llm.rs
  - crates/xvision-engine/src/agent/execute.rs
interfaces_used:
  - xvision_engine::strategies::manifest::PublicManifest::required_models (rename + demote)
  - xvision_engine::strategies::slot::SlotConfig::model_requirement (demote)
  - Validation/launch paths that read these fields (remove gate)
parallel_safe: false
parallel_conflicts:
  - strategy-slot-prompt-resolution
verification:
  - cargo test -p xvision-engine --test strategy_attested_with
  - cargo test -p xvision-engine
  - pnpm -C frontend/web typecheck
acceptance:
  - `required_models` / `model_requirement` fields renamed to `attested_with` (or kept as deprecated aliases for one release if needed by serialized data; pre-launch breaking-change rules apply)
  - No eval-launch-time gate based on `attested_with` — model selection comes from the agent binding, full stop
  - `attested_with` surfaces in the strategy detail UI as informational "this strategy was last published with model X" — never blocks a different binding
  - Documentation updated to reflect the demoted semantics
  - The `model_requirement` deserialize/serialize path drops the legacy validation gate; existing manifests still parse
---

# Scope

Demote `Strategy.required_models` and `SlotConfig.model_requirement`
from authoring/launch gates to informational attestation. Today they
constrain which model can be bound; the intake removes that gate
because gating model choice contradicts the strategies-folder
direction ("user owns the model choice; strategy describes intent").

Rename to `attested_with` (or equivalent) to signal "this strategy
was last published / tested with model X" without blocking different
bindings.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
row "Demote `required_models` / `model_requirement` to informational
`attested_with`; no eval-time substitution gate."

# Out of scope

- Provider attestation per call (already shipped — `eval-provider-attestation` #450)
- Provider preflight (already shipped — `eval-provider-preflight` #452)
- New marketplace fields (defer to V2C)

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/strategy-model-attestation-only -b task/strategy-model-attestation-only origin/main
```

# Notes

Touches `crates/xvision-engine/src/strategies/manifest.rs` (line 14:
`required_models: Vec<String>`) and `slot.rs` (line 8:
`model_requirement: String`). Coordinate with
`strategy-slot-prompt-resolution` — both edit `slot.rs` and
`manifest.rs`. Sequential preferred; smaller diff lands first.
