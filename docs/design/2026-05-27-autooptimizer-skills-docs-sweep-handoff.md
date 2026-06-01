# Skills + docs sweep handoff — autooptimizer rename

> For: technical writer / engineer doing the doc sweep
> Date: 2026-05-27
> Source of truth: `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`

## TL;DR

Sweep two skill files, MANUAL.md, the dashboard wiki, and any
README/marketing copy that references autooptimizer surfaces.
Replace developer-surface terms with operator-surface terms per the
terminology lock. This is largely mechanical search-and-replace, but
the skills are operator-facing reference material and need extra
care — they're the canonical command examples operators copy from.

This handoff ships AFTER the CLI rename lands, because the skills
demonstrate CLI usage. Do not start until the CLI deprecation aliases
are live (so the skill examples actually work on the previous
release) AND the new verbs are landed (so the skill can use them).

## Files in scope

- `.claude/skills/xvision/autooptimizer-ops/SKILL.md` — the canonical
  autooptimizer ops skill; demonstrates the standard distillation
  flow with the CLI
- `.claude/skills/xvision/flywheel-ops/SKILL.md` — the flywheel
  observability skill (read it first to see what's there)
- `MANUAL.md` (project root) — search for autooptimizer / memory /
  flywheel sections
- `crates/xvision-dashboard/wiki/cli-reference.md` — CLI reference
  rendered into the dashboard wiki
- `README.md` (project root) — only if it mentions any of the
  renamed terms in user-facing copy
- `marketing/` — if any marketing copy references autooptimizer
  vocabulary; search `marketing/` for `mutation`, `mutator`,
  `epsilon`, `holdout`, `merkle`, `ghost`, `quarantined`

## Files NOT in scope

- Anything under `docs/superpowers/specs/` (those use
  developer-surface vocabulary on purpose — the spec amendment
  handoff covers the one exception)
- Anything under `docs/superpowers/plans/` (developer-facing)
- Anything under `docs/superpowers/notes/`, `research/`, `evidence/`,
  `reviews/` (developer/historical)
- Code comments inside `crates/` (developer-surface; the code stays
  technical)
- The team coordination files at `team/` (developer process docs)

## The autooptimizer-ops skill rewrite

Current content (`.claude/skills/xvision/autooptimizer-ops/SKILL.md`):

```
---
name: xvision-autooptimizer-ops
description: Operate xvision autooptimizer distillation: stage Patterns from Observation cohorts, apply numeric gates and blind Findings, promote/demote Patterns, and preserve lineage evidence.
---

# xvision autooptimizer ops

Use this skill for offline memory distillation and Pattern promotion work.
AutoOptimizer commands are offline-only; do not run them inside a live trading
decision process.

## Standard Flow

```bash
xvn autooptimizer run \
  --agent <agent_id> \
  --pattern-text "<candidate Pattern>" \
  --embedding-json '[...]' \
  --json

xvn autooptimizer gate <run_id> \
  --metric sharpe \
  --parent-day-score <n> \
  --child-day-score <n> \
  --parent-holdout-score <n> \
  --child-holdout-score <n> \
  --gate-epsilon <n> \
  --finding-text "<blind qualitative finding>" \
  --json

xvn autooptimizer promote <run_id> --json
```

[...]
```

Proposed rewrite:

```
---
name: xvision-autooptimizer-ops
description: Operate the xvision autooptimizer: distill Observations into candidate Patterns, run the gate against today and the untouched test period, record blind findings, activate or retire Patterns, and preserve the lineage evidence for audit.
---

# xvision autooptimizer ops

Use this skill for offline Pattern distillation work. AutoOptimizer
commands are offline-only; do not run them inside a live trading
decision process.

## Standard flow

```bash
xvn autooptimizer run \
  --agent <agent_id> \
  --pattern-text "<candidate Pattern>" \
  --embedding-json '[...]' \
  --json

xvn autooptimizer gate <run_id> \
  --metric sharpe \
  --baseline-today-score <n> \
  --candidate-today-score <n> \
  --baseline-untouched-score <n> \
  --candidate-untouched-score <n> \
  --min-improvement <n> \
  --finding-text "<finding written blind to the numeric scores>" \
  --json

xvn autooptimizer activate <run_id> --json
```

## Invariants

- Never activate from a single Observation cohort.
- The gate's numeric decision (Kept / Dropped) must pass independently
  before activation.
- The finding is qualitative context, not the verdict.
- A Pattern's training cutoff must come from the latest source
  Observation's bar timestamp, not wall clock time.
- Retiring is soft-delete first; hard delete requires explicit operator
  confirmation through the memory janitor path.

## Evidence to capture

- `xvn autooptimizer inspect <run_id> --json`
- Gate input/output JSON, including the blind finding fields.
- Activated Pattern row and contributing Observation ids.
- Look-ahead-protection regression output before declaring an
  activation path change done.
```

Diff summary:
- Description rewritten to use operator vocabulary
- All four gate flags renamed
- `--gate-epsilon` → `--min-improvement`
- `promote` → `activate`
- "demote" → "retire" (in invariant prose)
- "blind qualitative finding" reworded to "finding written blind to
  the numeric scores"
- "Pattern `training_window_end`" → "Pattern's training cutoff"
- "Leakage-regression" → "Look-ahead-protection regression"

## The flywheel-ops skill

(Read the current `.claude/skills/xvision/flywheel-ops/SKILL.md`
first — content unknown to me at audit time.) Apply the same
principles:

- Description: rewrite to remove dev jargon
- CLI examples: use renamed verbs and flags (note: most
  `xvn flywheel` verbs stay; only the output column names change per
  the CLI handoff)
- Invariants/principles: use operator vocabulary

## MANUAL.md sweep

Search MANUAL.md for these terms and replace per the lock:

- `epsilon`, `gate-epsilon` → `minimum improvement`, `min-improvement`
- `holdout` (when describing the time window) → `untouched test period`
- `mutation`, `mutator` → `experiment`, `experiment writer`
- `promote`/`promotion`/`promoted` in autooptimizer context →
  `activate`/`activation`/`activated`
- `xvn memory promote` (Observation-cohort distillation form) →
  `xvn memory distill`; surrounding "promote(d)" prose for this
  operation becomes "distill(ed)"
- `demote`/`demotion`/`demoted` → `retire`/`retiring`/`retired`
- `ghost`, `quarantined` (as status names) → `rejected`, `suspect`
- `merkle`, `BLAKE3`, `Ed25519`, `canonical JSON` — if any of these
  appear in MANUAL.md, that's a defect; remove or replace with the
  operator-facing equivalent (Cycle proof, Strategy fingerprint,
  operator sign-off, etc.)
- `tier` (as a flag/concept name in CLI context) → `kind`
- `promotion_state` → `status`
- `lineage depth` → `generations deep`
- `Mint Child` → `Train new version`
- `demos` (in operator context) → `examples`
- `priors` (in operator context) → `background patterns`

If a term appears inside a code block that's literally the spec
discussing developer-surface concepts, leave it — that block belongs
to the developer-surface narrative. The rename applies to prose and
to operator-action examples.

## Dashboard wiki sweep

Same as MANUAL.md but applied to
`crates/xvision-dashboard/wiki/cli-reference.md` (and any sibling
wiki pages). The CLI reference page must reflect the renamed verbs
and flags exactly as they appear in `xvn --help` output — if the CLI
ships with both old (hidden) and new verbs, the wiki only documents
the new ones.

## README.md / marketing/

Read README.md and search `marketing/` for the term list above. If
any are present:

- README.md: replace inline, preserve surrounding prose flow
- marketing/templates/: replace, then have someone proofread for
  rhythm — marketing copy often loses cadence in mechanical replaces
- marketing/pitch/cold-email.md and marketing/methods.md: these are
  outbound; extra care, get a second pair of eyes

## Acceptance criteria

1. autooptimizer-ops/SKILL.md and flywheel-ops/SKILL.md both updated;
   `cat .claude/skills/xvision/autooptimizer-ops/SKILL.md | grep -iE
   '(epsilon|holdout|mutation|mutator|promote|demote|ghost|quarantined|merkle|blake3|ed25519)'`
   returns nothing.
2. MANUAL.md: same grep on the autooptimizer / memory / flywheel
   sections returns nothing.
3. crates/xvision-dashboard/wiki/cli-reference.md: same grep returns
   nothing.
4. README.md: same grep returns nothing in user-facing copy.
5. Every CLI example in every updated doc actually runs against the
   current main branch (run them; verify exit code 0 or expected
   non-zero with documented reason).
6. Diff is reviewable — no incidental rewording, no scope creep into
   restructuring sections.

## Sequencing dependency

This handoff requires the CLI rename to have shipped first (so the
new verbs and flags exist). Do not start until:

- `xvn autooptimizer activate`, `xvn autooptimizer retire`,
  `xvn memory activate`, `xvn memory retire` (or `distill`) all exist
- `--min-improvement`, `--baseline-today-score`,
  `--candidate-today-score`, `--baseline-untouched-score`,
  `--candidate-untouched-score`, `--kind`, `--status`,
  `--confirm-no-cutoff` all exist
- Deprecation aliases for the old forms are also live (so any older
  doc still works)

## Things to push back on

- If a skill description rewrite changes the activation behavior of
  the skill (the LLM router matches against the description), flag
  it — Murat's skill-pack discipline (see the earlier autooptimizer
  evaluation) says skill descriptions drive routing precision. The
  rewrite above tries to preserve intent but the wording shift might
  affect when the skill fires.
- If marketing/methods.md has carefully crafted copy that the rename
  would break, leave it and open a follow-up ticket — don't damage
  outbound material with a mechanical sweep.
- If you find autooptimizer vocabulary in places this handoff
  didn't anticipate (a blog post, a Twitter thread checked into the
  repo, screenshots in marketing/demos/), flag and decide
  case-by-case.

## Reference

- Terminology lock: `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`
- Frontend handoff (for context on what the operator sees):
  `docs/design/2026-05-27-autooptimizer-frontend-rename-handoff.md`
- CLI handoff (the prerequisite):
  `docs/design/2026-05-27-autooptimizer-cli-rename-handoff.md`
- Project-wide terminology note: `/CLAUDE.md` §Terminology
