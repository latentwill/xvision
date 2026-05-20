---
track: docs-freshness-staleness-guard
lane: leaf
wave: docs-user-and-agent-wiki-2026-05-20
worktree: .worktrees/docs-freshness-staleness-guard
branch: task/docs-freshness-staleness-guard
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - scripts/docs-freshness-lint.sh                                # NEW — the lint script
  - .github/workflows/docs-freshness.yml                          # NEW — CI invocation
  - .github/workflows/ci.yml                                      # if the existing CI workflow needs the new step added to its job matrix; otherwise leave alone
  - crates/xvision-dashboard/wiki/*.md                            # only to bump `last_reviewed` on the same-PR-as-new-verb path; not for content edits
forbidden_paths:
  - crates/xvision-dashboard/src/**                               # no Rust changes
  - crates/xvision-dashboard/wiki/index.toml                      # manifest schema is locked
  - crates/xvision-cli/src/commands/**                            # do not modify command modules
  - frontend/web/**
  - docs/**
interfaces_used:
  - tomlq / awk / sed                                             # whatever the lint script needs; shell-only
  - GitHub Actions YAML
verification:
  - bash scripts/docs-freshness-lint.sh                           # passes on a clean tree (all pages within 90 days, cli-reference touched alongside new verbs)
  - bash scripts/docs-freshness-lint.sh --check-recent-verbs       # invocation form the workflow uses
acceptance:
  - **New CI lint at `scripts/docs-freshness-lint.sh`.** Two checks:
    1. **Staleness check** — every `[[page]]` entry in `crates/xvision-dashboard/wiki/index.toml` has a `last_reviewed: YYYY-MM-DD` value AND the date is within 90 days of today (or the PR's HEAD commit date, whichever the script chooses — pick one and document). If any page is stale, the script exits non-zero and prints the offending slug + date.
    2. **CLI-verb sync check** — if the PR introduces a new top-level verb under `crates/xvision-cli/src/commands/` (new file under that directory OR a new top-level pub fn in `commands/mod.rs` registered with the CLI parser), then `crates/xvision-dashboard/wiki/cli-reference.md` must also be modified in the same PR. If not, the script exits non-zero with a pointer to the modified verb file(s).
  - **GitHub Actions workflow at `.github/workflows/docs-freshness.yml`.** Runs `bash scripts/docs-freshness-lint.sh --check-recent-verbs` on PRs that touch any of: `crates/xvision-dashboard/wiki/**`, `crates/xvision-dashboard/wiki/index.toml`, `crates/xvision-cli/src/commands/**`, or `crates/xvision-dashboard/src/routes/docs/**`. Skip on docs-only-revert PRs (let the operator dispatch override).
  - **First baseline run is green.** All 13 currently-baked pages have `last_reviewed = 2026-05-20`, so the staleness check passes today. The cli-verb sync check passes because the current `cli-reference.md` is up to date.
  - **Script is portable.** No `gawk`-only features, no GNU-coreutils-only flags. Tested on macOS (BSD utils) and Ubuntu (GNU utils). The CI runner is Ubuntu but operators run the script locally.
  - **Clear error messages.** When a check fails, the output names the file, the slug, and the rule that failed in plain English. No cryptic `awk`/`sed` exit codes.
  - **Bump-on-touch convention documented.** Add a one-line note to `crates/xvision-dashboard/wiki/index.toml`'s comment header: "Update `last_reviewed` whenever you edit the page's content." So contributors know the rule without grepping CI.

---

# Scope

Track #15 of `team/intake/2026-05-20-docs-user-and-agent-wiki.md`.
The wiki manifest at `crates/xvision-dashboard/wiki/index.toml` already
carries `last_reviewed` on every page (shipped 2026-05-20). What's
missing is the CI gate that actually enforces freshness and catches
the verb-sync drift the original intake flagged ("operators following
the in-app docs to drive `xvn` today will hit 'verb not found' within
the first agent loop").

# Out of scope

- Adding new wiki pages.
- Refactoring the wiki manifest schema.
- Adding `last_reviewed` checks to other docs trees (`docs/superpowers/`,
  `docs/runbook/`, etc.). Scope strictly to `crates/xvision-dashboard/wiki/`.
- Auto-bumping `last_reviewed` via a hook. Manual update is the convention.
- Per-section staleness windows. 90 days uniform.
- Renaming `last_reviewed` to something else. The field name is locked in
  the manifest.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/docs-freshness-staleness-guard status
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/docs-freshness-staleness-guard -b task/docs-freshness-staleness-guard origin/main
```

# Notes

Recon (2026-05-21):

- All 13 wiki pages have `last_reviewed = 2026-05-20` per `wiki/index.toml`.
- No existing freshness lint anywhere — `grep -rn last_reviewed scripts/ .github/` returns nothing.
- `crates/xvision-cli/src/commands/` contains the per-verb module files
  the verb-sync check should monitor.

The verb-sync check is intentionally conservative: it triggers on
*new* top-level verbs (new files OR new public fns in mod.rs), not
on changes within existing verb modules. Mid-verb refactors that
don't change the CLI surface don't force a docs update.

For the staleness check, prefer comparing against the PR's HEAD
commit date over `date +%s` — keeps the rule deterministic in CI
even when build agents have clock drift.
