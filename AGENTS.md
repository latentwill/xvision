# AGENTS.md

Guidance for coding agents (codex, claude, 100x, etc.) working in this repo.
The authoritative project guidance is **`CLAUDE.md`** — read it first; the rules
there apply to every agent regardless of tool.

## Worktree isolation (enforced — read this before doing anything)

This clone is shared by multiple concurrent agents. **Do not check out a branch
or commit branch/feature work in the main checkout
(`/Users/edkennedy/Code/xvision`).** Doing so collides with other agents already
working in it (HEAD moves under them, force-push conflicts, tangled commits).

Always work in your own worktree:

```bash
git worktree add .worktrees/<name> -b <branch>
cd .worktrees/<name>
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
```

A `pre-commit` hook (`.githooks/pre-commit`, enabled via `scripts/setup-hooks.sh`)
blocks branch commits in the main checkout. Override only deliberately with
`XVISION_ALLOW_MAIN_COMMIT=1`.

## Disk hygiene

The shared cargo target dir fills the disk if left unchecked. Build through the
guarded wrapper so it self-cleans when low, and never bare `cargo`:

```bash
scripts/cargo build --workspace      # checks/frees disk, then runs cargo
scripts/cargo-disk-guard.sh --check  # report free space
```

See `CLAUDE.md` → "Worktree isolation (enforced)", "Disk hygiene", and
"Team coordination" for the full coordination model (`team/` board, contracts,
conflict zones).

## metaswarm

This project uses [metaswarm](https://github.com/dsifry/metaswarm) for multi-agent orchestration. It provides 18 specialized agents, a 9-phase development workflow, and quality gates that enforce TDD, coverage thresholds, and spec-driven development.

### Workflow

- **Most tasks**: `$start` -- primes context, guides scoping, picks the right level of process
- **Complex features** (multi-file, spec-driven): Describe what you want built with a Definition of Done, then say: `Use the full metaswarm orchestration workflow.`

### Available Skills

Codex discovers skills by their SKILL.md `name` field. Invoke with `$name` syntax.

| Invoke | Purpose |
|---|---|
| `$start` | Begin tracked work on a task |
| `$setup` | Interactive guided setup |
| `$design-review-gate` | Trigger design review gate (5 reviewers) |
| `$pr-shepherd` | Monitor a PR through to merge |
| `$handling-pr-comments` | Handle PR review comments |
| `$brainstorming-extension` | Refine an idea with design review gate |
| `$create-issue` | Create a well-structured GitHub Issue |
| `$plan-review-gate` | Adversarial plan review (3 reviewers) |

### Quality Gates

- **Design Review Gate** -- 5-reviewer design review after design is drafted (`$design-review-gate`)
- **Plan Review Gate** -- 3 adversarial reviewers (Feasibility, Completeness, Scope & Alignment) -- ALL must PASS
- **Coverage Gate** -- `.coverage-thresholds.json` defines thresholds. BLOCKING gate before PR creation

### Testing & Quality

- **TDD is mandatory** -- Write tests first, watch them fail, then implement
- **100% test coverage required** -- Enforced via `.coverage-thresholds.json`
- **Coverage source of truth** -- `.coverage-thresholds.json` defines thresholds. The orchestrator reads it during validation.

### Workflow Enforcement (MANDATORY)

- **After brainstorming** -> MUST run `$design-review-gate` before planning or implementation
- **After any plan is created** -> MUST run `$plan-review-gate` before presenting to user
- **Coverage** -> `.coverage-thresholds.json` is the single source of truth. All skills must check it.
- **Agent discipline** -> NEVER use `--no-verify`, NEVER `git push --force` without approval, NEVER self-certify, ALWAYS follow TDD, STAY within file scope
