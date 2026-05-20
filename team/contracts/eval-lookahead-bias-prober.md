---
track: eval-lookahead-bias-prober
lane: leaf
wave: v2e
worktree: .worktrees/eval-lookahead-bias-prober
branch: task/eval-lookahead-bias-prober
base: origin/main
status: ready
depends_on:
  - eval-trace-surface-foundation
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-eval/src/baselines/**                     # read-only audit; emit lookahead_suspected if a baseline reads forward
  - crates/xvision-eval/src/prober/mod.rs                    # NEW
  - crates/xvision-eval/src/prober/lookahead.rs              # NEW — two-pass replay diff impl
  - crates/xvision-eval/Cargo.toml                           # if a new dep is needed (likely not)
  - crates/xvision-engine/src/eval/findings.rs               # lookahead_suspected kind variant — disjoint region with other tracks
  - crates/xvision-engine/tests/lookahead_prober_*.rs        # NEW
  - frontend/web/src/api/types.gen/**                        # ts-rs regenerated for the new finding kind
forbidden_paths:
  - frontend/web/src/**                                      # no UI work this track
  - crates/xvision-data/**
  - crates/xvision-engine/src/eval/executor/**               # not this track's concern
  - crates/xvision-engine/migrations/**                      # no schema change
interfaces_used:
  - xvision-eval::baselines::Algorithm
  - xvision-engine::eval::findings::Finding
  - xvision-engine::eval::cycle (the foundation's enriched cycle record)
parallel_safe: true
parallel_conflicts:
  - eval-trace-surface-foundation (findings.rs — disjoint regions; foundation owns the schema columns, this track adds the lookahead_suspected variant)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-eval -- -D warnings
  - cargo test -p xvision-eval prober
  - cargo test -p xvision-engine lookahead_prober_
  - pnpm --dir frontend/web typecheck
acceptance:
  - **Baseline side-effect-freedom audit.** Each baseline in `crates/xvision-eval/src/baselines/` (`always_long`, `ma_crossover`, `macd_momentum`, `rsi_mean_reversion`) is reviewed for: (a) indicator state held between decision calls; (b) reads of `bars[t..]` (forward), `bars[..=t]` (current bar inclusive — leakage risk), or `bars[..t]` (correct, past-only); (c) any shared mutable state between baseline instances. Audit findings written to `crates/xvision-eval/src/baselines/AUDIT.md` (NEW). Any baseline that fails side-effect-freedom is fixed in this track or explicitly excluded from the prober's coverage set with a `# Excluded because <reason>` comment.
  - **Two-pass prober.** For a given `(strategy, scenario)`:
    * Pass 1: full backtest, recording each signal-firing bar's decision.
    * Pass 2: for each signal-firing bar `t` in Pass 1, re-run the strategy with `bars[..=t-1]` only, and assert the decision for `t` is identical.
    * Any divergence emits `lookahead_suspected { cycle_id, indicator_name: Option<String>, pass_1_action, pass_2_action }` with `evidence_cycle_ids: [cycle_id]` and `produced_by_check = "prober:lookahead"`.
  - **Indicator-name inference.** If the strategy's trace records which indicators were computed for each decision (per `eval-trace-surface-foundation`'s enriched decision record), the finding includes the suspected indicator name. If the trace doesn't carry that information, leave `indicator_name: None` — better to surface "something read forward at cycle 17" than to fabricate a pointer.
  - **CLI surface.** Add `xvn eval probe-lookahead --run <run_id>` which runs the two-pass prober post-hoc on an already-completed run (re-using its pinned bars hash from `eval-candle-integrity-and-manifest`). Output: pretty-printed findings list. Existing `xvn eval` subcommand structure pattern.
  - **Performance.** Two-pass implies 2× run time. The prober is opt-in (CLI subcommand or scenario flag `probe_lookahead: bool`), not on by default for every eval. Document in `docs/superpowers/specs/2026-05-08-eval-engine-design.md` §11 findings table.
  - **Tests:**
    * Positive case: a synthetic baseline that reads `bars[t+1].close` in its decision at `t` emits `lookahead_suspected` at every signal-firing bar.
    * Negative case: `always_long` (which makes no decision based on bars) does not emit any finding.
    * Negative case: `ma_crossover` (assumed clean after audit) does not emit `lookahead_suspected` on a sample scenario.
    * `bars[..=t]` leakage (reading the current bar): if a baseline's decision at cycle `t` is affected by `bars[t]` (which is information not available at the time of decision — the bar is closing), the prober flags it. Distinguish from `bars[..t]` (past only, correct) in the audit doc.
    * `xvn eval probe-lookahead --run <run_id>` smoke test on a fixture run.

---

# Scope

Research doc §3.5 — freqtrade-style lookahead-bias prober. Catches ~90%
of indicator-based leakage; doesn't catch cross-asset leakage, regime-
label leakage, or anything embedded in the prompt rather than indicator
code.

The track has two halves: (1) audit the existing baselines for side-
effect-freedom (so the prober has a known-clean control group), and
(2) implement the two-pass diff and surface as a CLI subcommand +
opt-in scenario flag.

# Out of scope

- Cross-asset leakage detection (a strategy that uses BTC's future
  price to decide ETH's now). Different shape; not v1.
- Regime-label leakage (a strategy whose regime input is computed using
  future bars). Catch via the baselines audit if it surfaces; otherwise
  defer.
- Prompt-encoded leakage (an LLM strategy whose system prompt contains
  "you will see future bars; do not act on them" or similar). Out of
  scope for an indicator prober; eval-review-agent territory.
- §3.7 bar-vs-tick fidelity guard (a scenario-level policy). Out of
  scope for this track.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-lookahead-bias-prober status
git -C .worktrees/eval-lookahead-bias-prober log --oneline -3 origin/main..HEAD

# Confirm:
#   - rebased on top of eval-trace-surface-foundation's merged commit
#   - findings.rs has evidence_cycle_ids + produced_by_check available
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-lookahead-bias-prober -b task/eval-lookahead-bias-prober origin/main
```

# Notes

The audit doc (`crates/xvision-eval/src/baselines/AUDIT.md`) is the
human-readable record of why each baseline is or isn't covered.
Future baseline additions need to update this doc; consider adding a
CI lint that fails if a new baseline appears without an audit entry.

Prober runs are 2× the cost of normal runs. Don't put `probe_lookahead`
in the marketplace's signed-attestation gate (it's an offline tool,
not a real-time requirement).
