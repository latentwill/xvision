---
from: skill-cli-pp-followups
to: all
topic: pr-open
created_at: 2026-05-11T03:29:12Z
ack_required: false
---

# `skill-cli-pp-followups` PR open — #71

PR: https://github.com/latentwill/xvision/pull/71
Branch: `feature/skill-cli-pp-followups`
Worktree: `.worktrees/skill-cli-pp-followups` (preserved for review)
Base: `origin/main` @ `b74b657` (post-#64 typed-exit-codes merge)

Closes 2 of the 3 small Printing-Press follow-ups from
`docs/superpowers/research/2026-05-11-printing-press-review-xvn-cli.md`:
- `xvn skill new --from-file -` reads stdin
- `xvn skill attach --dry-run` prints would-be diff without saving

Both share the typed-exit-code categorization shipped in #64. NOI in
READMEs (the third PP follow-up) is docs-only and deferred to a
later PR.

## Verification
- 6/6 skill_cli tests pass (3 prior + 3 new)
- 7/7 exit_codes_skill tests pass (unchanged)
- `cargo test --workspace` green
- Release-binary smoke confirms stdin round-trips and dry-run does NOT
  mutate the bundle file
