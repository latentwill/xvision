---
track: eval-honesty-smell-tests
lane: foundation
wave: eval-honesty-2026-05-21
worktree: .worktrees/eval-honesty-smell-tests
branch: task/eval-honesty-smell-tests
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/findings/uniformity.rs
  - crates/xvision-engine/src/eval/findings/mod.rs
  - crates/xvision-engine/src/eval/review/auto.rs
  - crates/xvision-engine/tests/eval_uniformity_smell.rs
  - team/contracts/eval-honesty-smell-tests.md
forbidden_paths:
  - crates/xvision-engine/src/eval/findings/mod.rs  # Finding/Severity enums — consume only
  - frontend/
interfaces_used:
  - RunStore::read_decisions
  - RunStore::record_finding
  - RunStore::read_findings
  - Finding
  - Severity
  - detect_uniformity (new — this track owns it)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine -- eval::findings::uniformity
  - cargo test -p xvision-engine -- eval::review::auto
acceptance:
  - Check 1 fires (severity=critical, kind="uniform_justification") when all n>=10 decisions share identical justification text; check returns early and suppresses checks 2 and 3.
  - Check 2 fires (severity=critical, kind="uniform_decision") when all n>=10 decisions share identical (action, conviction) pair but justifications are varied; check 1 must not have fired.
  - Check 3 fires (severity=warning, kind="near_uniform_justification") when >=90% of n>=20 decisions share the same justification and check 1 did not fire.
  - No finding is emitted for fewer than 10 decisions (identical-check threshold) or fewer than 20 decisions (near-uniform threshold).
  - The auto-reviewer emits verdict=failed (not inconclusive) when a critical uniformity finding is present, because classify_verdict sees the persisted finding before mapping.
  - Audit acceptance — run 01KS4D0MZBD5VGEQ9ACJDRBFBG (217/217 identical justifications "stub Gemini Flash 3.1 response", sharpe=-7.84, shipped as completed/inconclusive): this module would have fired check 1 with severity=critical, title="all 217 decisions returned identical justification text", preventing the inconclusive verdict and surfacing the stub-provider failure.
---

# Scope

Implements the uniformity smell-test module (`crates/xvision-engine/src/eval/findings/uniformity.rs`) and wires it into the run-finalize / auto-review path. The module exposes one pure function `detect_uniformity(decisions: &[DecisionRow]) -> Vec<Finding>` that applies three independent checks against a completed run's `eval_decisions` rows. Any critical finding it emits causes the auto-reviewer's `classify_verdict` to return `failed`, preventing stub-provider or model-collapse runs from shipping as `inconclusive`.

Intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`, track `eval-honesty-smell-tests`.

# Out of scope

- The guardrail log collapse track.
- The provider attestation track.
- The provider preflight check track.
- The `Finding` / `Severity` enums themselves — consumed, not modified.
- Auto-reviewer score band constants — tuned and frozen.
- Frontend — findings render via the existing pipeline.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-honesty-smell-tests status
git -C .worktrees/eval-honesty-smell-tests log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/eval-honesty-smell-tests
#   - base is up to date with origin/main (or rebase planned)
```

# Notes

2026-05-21: Initial implementation. Three checks implemented in `uniformity.rs`; wired into `run_auto_review` via `detect_uniformity` call before `read_findings` / `classify_verdict`. Unit tests cover all threshold boundaries. Integration test seeds 50 identical-justification rows and asserts `verdict=failed` + one `uniform_justification` finding persisted. Build/test deferred to maintainer workstation (extndly-dev cannot run cargo per CLAUDE.md).
