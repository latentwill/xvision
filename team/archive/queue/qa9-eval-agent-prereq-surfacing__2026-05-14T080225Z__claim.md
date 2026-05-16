# Claim: qa9-eval-agent-prereq-surfacing

Claimed: 2026-05-14T08:02:25Z

Worktree: `.worktrees/qa9-eval-agent-prereq-surfacing`

Branch: `qa9-eval-agent-prereq-surfacing`

Base: `main`

Scope:

- Surface missing strategy agent/provider prerequisites before users open the eval launcher.
- Keep the Strategy Inspector eval CTA from implying an agentless strategy is runnable.
- Anchor the user back to the Strategy agents section when setup is missing.

Verification target:

- `corepack pnpm --dir frontend/web test -- authoring-risk eval-runs`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

Local note:

- Rust checks are CI/non-deploy only per `CLAUDE.md`.
- Rebased off the original `qa8-eval-provider-preflight` stack onto `main`
  after the QA9 prerequisite branches merged.
