---
from: skill-cli-pp-followups
to: all
topic: claim
created_at: 2026-05-11T03:12:32Z
ack_required: false
---

# `skill-cli-pp-followups` track claimed

Picks up two of the three small Printing-Press follow-ups flagged in
`docs/superpowers/research/2026-05-11-printing-press-review-xvn-cli.md`:

1. **Stdin support for `xvn skill new`** — `--from-file -` reads stdin so
   agents can pipe LLM output into skill registration without a tmpfile
   dance.
2. **`--dry-run` for `xvn skill attach`** — loads the strategy + skill,
   runs the attach in-memory, prints the would-be JSON diff, **skips the
   bundle save**. Standard PP convention for mutating commands.

Both live in `crates/xvision-cli/src/commands/skill.rs` (one cohesive PR).
The third PP recommendation (NOI in READMEs) is a docs-only follow-up —
defer.

Branch `feature/skill-cli-pp-followups` based on `origin/main` @ `b74b657`
(post-#64 typed-exit-codes merge).
Worktree: `.worktrees/skill-cli-pp-followups`.

## Non-conflicts

- All open frontend PRs (#65 / #67 / #68) are in `frontend/web/**`; no overlap.
- PR #62 (Track A — findings orchestration) is in `crates/xvision-engine/src/eval/`; no overlap.
- The H worktree (`feature/strategies-disabled-affordance`) is also frontend; no overlap.
- This work depends on PR #64 (typed exit codes) which already merged.

## Smoke plan

- New stdin path: `cat fixture.md | xvn skill new --from-file -` registers the skill
- New dry-run path: `xvn skill attach <id> --slot trader --skill <name> --dry-run` prints what would change without writing
- 3 new integration tests in `crates/xvision-cli/tests/skill_cli.rs` covering stdin happy path + dry-run no-side-effect + existing tests still pass
- `cargo test --workspace` green

## Why these two now

These are the two cheapest agent-ergonomics wins from the PP review. Each
is a few lines + a test. Closes the small-but-loud part of the PP audit
without the larger typed-exit-codes-style cross-CLI investment.
