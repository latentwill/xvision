---
track: eval-provider-attestation
lane: foundation
wave: eval-honesty-2026-05-21
worktree: .claude/worktrees/agent-a851d36c867f421fe
branch: task/eval-provider-attestation
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/export.rs
  - crates/xvision-cli/src/commands/eval/mod.rs
  - crates/xvision-dashboard/src/routes/eval_runs.rs
  - team/contracts/eval-provider-attestation.md
forbidden_paths:
  - frontend/**
  - crates/xvision-engine/src/eval/review/auto.rs   # score bands — out of scope
  - crates/xvision-engine/src/strategies/slot.rs    # model_requirement semantics unchanged
interfaces_used:
  - eval::export::ProviderDiagnostics
  - eval::export::ProviderModel
  - eval::export::load_providers_used
  - eval::store::RunStore::record_finding
  - eval::store::RunStore::read_findings
  - eval::findings::{Finding, Severity, FINDING_SCHEMA_VERSION}
  - strategies::Strategy::trader_slot
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --check -p xvision-engine -p xvision-cli
  - cargo clippy -p xvision-engine -p xvision-cli -- -D warnings
  - cargo test -p xvision-engine eval::export
acceptance:
  - ProviderDiagnostics gains a `providers_used: Vec<ProviderModel>` field serialized
    as a JSON array; absent from wire when empty (skip_serializing_if).
  - ProviderModel { provider, model, call_count } defined in the same module.
  - build_provider_diagnostics populates providers_used from model_calls rows via
    the eval_run_id → agent_runs → spans → model_calls join path.
  - A provider_mismatch Warning finding is emitted (best-effort, idempotent) when
    trader_slot.model_requirement is non-empty and no providers_used entry matches.
  - Empty model_requirement or empty providers_used → no finding emitted.
  - xvn eval show <run_id> text output renders providers_used (one line per pair).
  - GET /api/eval/runs/:id/export includes providers_used via Serde (no extra code).
  - Unit tests: providers_used populator with two distinct providers; finding emitter
    mismatch/match/empty-requirement/empty-providers cases; idempotency.
  - Integration test: export JSON contains providers_used array for a seeded run.
---

# Scope

Surface the actual `(provider, model)` pairs used during an eval run on the
export and CLI, and emit a `provider_mismatch` warning finding when the strategy
manifest's `trader_slot.model_requirement` names a model that wasn't actually
called.

Implements the `eval-provider-attestation` track from intake
`team/intake/2026-05-21-eval-honesty-and-agent-graph.md` (finding #4: "No
provider/model attestation on the eval export").

Reference run: xvnej-app `01KS4D0MZBD5VGEQ9ACJDRBFBG` — strategy requested
`anthropic.claude-sonnet-4.6`, but all 217 decisions used
`gemini-local/gemini-3.1-flash`. This track makes that mismatch visible.

# Out of scope

- `model_requirement` field semantics or demotion to `attested_with` (separate track).
- Auto-reviewer score bands.
- Provider preflight (separate track).
- Guardrail log collapse (separate track).
- Uniformity smell tests (separate track).
- Any frontend Vue/Svelte/React component changes (follow-up track).

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .claude/worktrees/agent-a851d36c867f421fe status
git -C .claude/worktrees/agent-a851d36c867f421fe log --oneline -3 origin/main..HEAD
```

# Notes

- `load_providers_used` is `pub` so `xvn eval show` can call it without
  going through the heavier `build_export` path.
- The mismatch finding is emitted inside `build_export` (best-effort, idempotent)
  rather than at finalize time because the export is the earliest moment when both
  `providers_used` and `model_requirement` are materialised together.
- Build verification deferred to maintainer's workstation — this host OOMs on
  Rust builds per CLAUDE.md / extndly_dev_resource_limits.md.
