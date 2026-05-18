---
track: trader-output-action-case-insensitive
lane: integration
wave: qa-operator-2026-05-18-r3
worktree: .worktrees/trader-output-action-case-insensitive
branch: task/trader-output-action-case-insensitive
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/trader_output.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/executor/mod.rs
  - frontend/web/**
interfaces_used:
  - TraderOutput::action (String)
  - TraderOutput::parse / validate
parallel_safe: false
parallel_conflicts:
  - "alpaca-paper-crypto-submit / qa-trace-broker-spans / qa-decisions-position-pnl: all listed as multi-owner on trader_output.rs in OWNERSHIP.md row 63. Coordinate disjoint regions; this contract only touches the action-vocabulary match (~line 336) and the field-level diagnostic (~line 346)."
verification:
  - cargo test -p xvision-engine -- trader_output
  - cargo clippy -p xvision-engine -- -D warnings
acceptance:
  - Trader action input is **lower-cased** before the strict-vocabulary
    match in `crates/xvision-engine/src/eval/executor/trader_output.rs`
    (around line 336). `"Hold"`, `"HOLD"`, `"hOLd"` all parse to
    `"hold"` and pass; the canonical lowercase form is what flows
    downstream.
  - The field-level diagnostic at line 346 references the normalized
    (lowercased) input the parser actually saw — not the raw agent
    string — so the operator can see what was rejected after
    normalization.
  - The downstream TraderAction enum / vocabulary stays exact. This
    is purely a normalization at the parser boundary, not a relaxation
    of the underlying type system.
  - New tests in `trader_output.rs`:
    - `action_accepts_title_case` (Qwen `"Hold"` repro from operator)
    - `action_accepts_upper_case` (`"LONG_OPEN"`)
    - `action_accepts_mixed_case` (`"Short_Open"`)
    - `unknown_action_after_lowercase_still_fails` (e.g. `"Buy"`
      lowercased to `"buy"` still fails — proves normalization
      doesn't accidentally widen the vocabulary)
  - Existing tests (`missing_action_has_field_level_diagnostic`,
    `invalid_action_has_field_level_diagnostic`,
    and the rest of the file's test module) continue to pass.
---

# Scope

Operator (2026-05-18) Qwer 3.6 eval errored on
`run 01KRWHHBR8FVKM1NVJPQXD4D4B decision 0`:

```
trader_output[invalid_field]: trader output action must be one of
long_open, short_open, flat, hold (got `Hold`)
```

Qwen produced `"action": "Hold"`. The strict match at
`trader_output.rs:336` only accepts the exact lowercase tokens.
Lowercase the agent-supplied string before the match.

One-line behavioural change plus tests. Downstream code already
assumes lowercase canonical form; the fix is purely at the parser
boundary.

# Out of scope

- Widening the canonical action vocabulary (no new actions).
- Loosening other trader-output field validation (conviction range,
  justification min-length, etc.). Separate hardening pass.
- Changing the prompt or system message that tells the agent what
  the legal actions are.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/trader-output-action-case-insensitive status
git -C .worktrees/trader-output-action-case-insensitive log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/trader-output-action-case-insensitive \
  -b task/trader-output-action-case-insensitive origin/main
```

# Notes

The file is multi-owner per `team/OWNERSHIP.md:63` with three other
tracks. This contract only touches the vocabulary match block and
its diagnostic — a 1-line behaviour change + tests. Coordinate via
team/queue/ if any of those tracks open a PR first.

Append checkpoints / PR links below.
