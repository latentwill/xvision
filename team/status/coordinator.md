---
track: coordinator
worktree: /Users/edkennedy/Code/xvision (main)
branch: main
phase: phase-b-three-prs-open
last_updated: 2026-05-10T07:56:17Z
---

# What I'm doing right now

Phase A merged (PRs #4, #5, #6, #7). Phase B is now running with three
parallel PRs open across three CLI sessions:

- **PR #8** — leverage-items Items A–D (docs): hackathon 1-pager, README,
  MANUAL.md scale-tier addendum, incident response runbook. Owner:
  session 3 (external CLI).
- **PR #9** — frontend-foundation Phase B: ts-rs codegen +
  `/strategies` page wired to engine api. Owner: session 2 (external CLI).
- **PR #10** — eval-engine Phase 3.A: migration 002 + Run/Scenario types +
  RunStore. Owner: session 1 (this CLI).

All three PRs are independent (touch different crates / files), so any
merge order works. After they merge, even more Phase B work unblocks
(eval-3b executors, eval-3c metrics + findings, strategy-2a-mcp,
llm-providers, settings-onboarding, chat-rail, command-palette, etc.).

# Blocked on

Operator merge review for PR #8, PR #9, PR #10.

# Next up after merges

Eval Engine Phase 3.B (executors) is the most-impactful next slice — once
PR #10 merges, Phase 3.B can start. It uses `Arc<dyn BrokerSurface>` from
PR #5 to wire PaperExecutor.

Other immediate options unblocked by Phase A merges (none claimed yet):
- **strategy-2a-mcp** (Plan #6) — MCP + tool-call + 7 templates
- **llm-providers** (Plan #7) — `[[providers]]` registry + per-arm SlotRef
- **strategy-2b-skills** (Plan #8) — local OSShip-style skills
- **strategy-2d-dashboard-wizard** (Plan #9) — Wizard + Inspector + Strategies + Eval routes
- **settings-onboarding** (Plan #10) — `/setup` + `/settings/{providers,brokers,daemon,identity,danger}`
- **chat-rail-persistence** (Plan #11) — owns migration 003
- **command-palette** (Plan #12) — owns migration 004 (FTS5)

# Tracks ready for external CLI pickup

Any of the unclaimed B-tier tracks above. The pattern for spawning is:

```
cd /Users/edkennedy/Code/xvision
git fetch origin && git worktree add .worktrees/<track> -b feature/<track> main
cd .worktrees/<track>
claude
# inside Claude:
#   1. Read team/MANIFEST.md
#   2. Write team/briefings/<track>.md (briefing should reference the plan file)
#   3. Post team/queue/<track>__<utc>__claim.md
#   4. Begin work
```
