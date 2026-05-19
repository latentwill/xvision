---
track: agent-cicd-extract-package
lane: integration
wave: agent-cicd-phase-2
worktree: .worktrees/agent-cicd-extract-package
branch: task/agent-cicd-extract-package
base: origin/main
status: deferred
depends_on:
  - agent-cicd-shadow-run            # Phase-1 must be running live in xvision first
  - agent-cicd-review-routing        # Phase-2 review loop (separate contract, not yet cut)
blocks: []
stacking: none
allowed_paths:
  # Activation-time re-confirmation required: at activation, the conductor
  # widens this list to include `tools/agent-conductor/**` (source removal),
  # `agent-conductor.config.ts` (potential import path bump), and `package.json`
  # (consume the published package). Listed narrowly here while deferred so
  # board-lint passes alongside the active daemon-skeleton contract.
  - .github/workflows/agent-conductor-bump.yml
  - docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md  # update post-extraction
forbidden_paths:
  - team/schema/**
  - team/contracts/**
  - team/board.md
  - team/board-v2.md
  - crates/**
  - frontend/web/**
  - migrations/**
interfaces_used:
  - agent-conductor (newly published npm package)
  - team/schema/board.schema.json (now consumed via package)
  - agent-conductor.config.ts (xvision's instance config)
parallel_safe: false
parallel_conflicts:
  - any agent-cicd-* track (config + daemon are load-bearing)
verification:
  - npm view agent-conductor versions --json | jq -e '.[-1] | test("^[0-9]+\\.[0-9]+\\.[0-9]+$")'
  - npx --yes agent-conductor --version
  - npx --yes agent-conductor status --json --config agent-conductor.config.ts | jq -e '.instance.name == "xvision"'
  - bash scripts/board-lint.sh
acceptance:
  - "`tools/agent-conductor/src/**` source has been moved to a new standalone repo (suggested: `github.com/latentwill/agent-conductor`). The xvision repo retains only `agent-conductor.config.ts` (instance config), `team/schema/board.schema.json` (board schema for migrations), and `.github/projects/agent-cicd-board.md` (Project setup doc). The old `tools/agent-conductor/` directory is removed or reduced to a single README pointing at the new repo + install instructions."
  - "Published to npm as `agent-conductor` with semver. Initial version is `1.0.0` (the Phase-1+2 feature set is the v1 baseline). Major bumps reserved for config-schema breaks; the package documents the supported config-version range."
  - "`npx agent-conductor init` scaffolder lands. Run inside an empty repo, it drops: (a) an `agent-conductor.config.ts` stub with required fields (`name`, `repo`, `project`) marked `TODO:` and optional fields commented out with defaults; (b) `team/schema/board.schema.json` (copied from the package); (c) `.github/projects/agent-conductor-board.md` setup doc; (d) `launchd/com.<name>.agent-conductor.plist` (macOS) and `systemd/agent-conductor.service` (linux) templates; (e) `.gitignore` additions for `~/.cache/agent-conductor/`. Idempotent — re-running in a populated repo prints a diff and skips files that already exist."
  - "xvision's `agent-conductor.config.ts` survives the extraction unchanged (or with at most a one-line import path change from `'agent-conductor'`). The status output's `instance.name` is still `xvision`; Hermes routing is unaffected."
  - "xvision's launchd plist is regenerated from the new package's template; old plist is removed. Documented in the daemon-skeleton README's migration note."
  - "Smoke test from a clean second repo: `mkdir test-repo && cd test-repo && git init && npx agent-conductor init && $EDITOR agent-conductor.config.ts && agent-conductor status --json --shadow` returns a valid v1 envelope with `instance.name` from the new config. (Recorded as a screencast or transcript in the new repo's README; not run as a CI check on xvision.)"
  - "Standalone repo CI: typecheck, unit tests, lint including the generalized no-host-repo-references rule (`xvision`, `latentwill`, `kinamix`, any repo-specific term in `core/` source). Publishing pipeline gated on green CI + a manual approval."
  - "Spec section `## Modular packaging` in `docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md` updated post-extraction: marks Stage-1 as complete, links the published package + new repo, replaces 'planned' language with 'shipped'. No new sections."
  - "No regressions in xvision's Phase-1 + Phase-2 acceptance: the same shadow-cohort smoke flow that passed pre-extraction passes post-extraction. Recorded as an entry in `team/archive/agent-cicd-extract-package/`."
---

# Scope

Phase-2 deliverable: extract the in-tree `tools/agent-conductor/` into a
standalone npm package so any repo can adopt the control plane with a
single `npx agent-conductor init`. xvision becomes a consumer rather
than a host.

Implements the "Stage-2" step of the spec's `## Modular packaging`
section. The boundary discipline enforced in
`agent-cicd-daemon-skeleton` (no host-repo references in source) is
what makes this contract a `git mv` + publish rather than a rewrite.

Status is `deferred` until Phase-1 is running live in xvision (i.e.
`agent-cicd-shadow-run` has been signed off and the daemon has been
operating in live mode for at least one full cohort), and the Phase-2
review-routing contract has landed. Extracting before the daemon's
shape is stable is the classic "polished abstraction over guesswork"
trap.

# Out of scope

- Adopting the package in a second repo (kinamix, etc.). That's a
  per-repo contract written against the consuming repo's intake.
- Adding new daemon features (review routing, deploy, healthcheck) —
  those are separate Phase-2/3 contracts.
- Renaming `agent-conductor`. The name was decided in the spec's
  intake decisions.
- Migrating the standalone repo's CI off GitHub Actions.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-cicd-extract-package status
git -C .worktrees/agent-cicd-extract-package log --oneline -3 origin/main..HEAD
# Confirm Phase-1 is live, not just shadowed
test -f team/archive/agent-cicd-phase-1-shadow/report.md
grep -q "flip-to-live" team/archive/agent-cicd-phase-1-shadow/report.md
# Confirm Phase-2 review-routing has merged
git log origin/main --oneline -- tools/agent-conductor/src/transitions/changes_requested.ts 2>/dev/null | head -3
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-cicd-extract-package -b task/agent-cicd-extract-package origin/main
```

# Notes

The "new repo" target — `github.com/latentwill/agent-conductor` is the
suggested name. Confirm with operator before pushing the initial
commit. If the operator chooses a different scope/name (e.g.
`@latentwill/agent-conductor`), the README and config typing import
path follow.

Versioning: start at `1.0.0`, not `0.x`. The Phase-1+2 feature set is
the stable baseline by the time this contract runs; pre-1.0 versioning
signals "expect breaks" which would be wrong.
