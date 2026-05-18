---
track: v2a-in-app-docs
lane: leaf
wave: v2a
worktree: .worktrees/v2a-in-app-docs
branch: task/v2a-in-app-docs
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/docs/**
  - frontend/web/src/features/docs/**
  - frontend/web/src/api/docs.ts
  - crates/xvision-dashboard/src/routes/docs/**       # static doc serving
forbidden_paths:
  - crates/xvision-engine/**
  - docs/**                                            # source docs untouched
  - frontend/web/src/features/eval-runs/**
  - frontend/web/src/features/chat-rail/**
interfaces_used:
  - GET /api/docs/index
  - GET /api/docs/page/:slug
parallel_safe: true
parallel_conflicts: []
verification:
  - corepack pnpm --dir frontend/web test -- docs
  - corepack pnpm --dir frontend/web typecheck
  - cargo test -p xvision-dashboard routes::docs
acceptance:
  - `/docs` route renders an index built from packaged Markdown.
  - At least: Quickstart, Strategies, Scenarios, Eval Runs, CLI Reference.
  - Search across docs index works (client-side fuzzy match acceptable).
  - Docs ship with the deployed image — no external network fetch at runtime.
---

# Scope

V2A item 2 from the action plan: surface existing in-repo documentation
inside the dashboard for first-time users. Source pages are subset of
`README.md`, `MANUAL.md`, and select `docs/`. The dashboard serves them via
a packaged Markdown index, not by reading `docs/` at runtime in the deployed
image.

# Out of scope

- Editing source docs.
- Translation / i18n.
- Versioned docs (deploy image carries one version).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/v2a-in-app-docs -b task/v2a-in-app-docs origin/main
```

# Notes

- Use the existing static-asset cache headers established by the
  `runtime-render-optimization` checkpoint (no-cache for index.html,
  immutable for hashed assets).
