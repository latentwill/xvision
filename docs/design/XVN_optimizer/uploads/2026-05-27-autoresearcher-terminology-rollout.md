# Wave intake — autoresearcher terminology rollout

> Date: 2026-05-27
> Source of truth: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`
> Audit context: `docs/superpowers/notes/2026-05-27-autoresearcher-plain-language-audit.md`
> Process: per `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`

## Wave summary

Roll out the autoresearcher plain-language rename across five
surfaces. The terminology lock is the contract; this wave decomposes
the rollout into five tracks with explicit dependencies so the
conductor can sequence them.

Five handoff docs already exist (`docs/design/2026-05-27-autoresearcher-*-handoff.md`).
Each handoff has its own acceptance criteria, file scope, and
push-back notes — those are the working specs. This intake adds the
cross-track dependencies and the wave-level acceptance gate.

## Tracks

### Track 1: Spec amendment (doc-only, 1–2 hours)

- Handoff: `docs/design/2026-05-27-autoresearcher-spec-amendment-handoff.md`
- Owner: spec maintainer (anyone)
- Scope: one file —
  `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`
- Allowed paths: `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`
- Forbidden paths: any other spec; any plan; any code
- Verification: §0.1 exists, every renamed concept has its first-mention
  parenthetical, no other content moved or renumbered
- Acceptance: spec reads clean end-to-end with parentheticals not feeling intrusive
- Dependencies: none — can start immediately
- Risk: lowest

### Track 2: Frontend label rename (React SPA)

- Handoff: `docs/design/2026-05-27-autoresearcher-frontend-rename-handoff.md`
- Owner: frontend designer/engineer
- Scope: ~80 string changes + 1 new component (`<ShortHash>`) + 1 new
  labels helper module
- Allowed paths: `frontend/web/src/features/memory/**`,
  `frontend/web/src/components/agent/MemoryTab.tsx`,
  `frontend/web/src/routes/agents-flywheel.tsx`,
  `frontend/web/src/components/ShortHash.tsx` (new),
  `frontend/web/src/features/memory/labels.ts` (new)
- Forbidden paths: `frontend/web/src/api/**`,
  `frontend/web/src/api/types.gen/**`, any route path, any Rust code
- Verification: banned-words check on rendered DOM (list in
  handoff §"Acceptance criteria"), screenshot diff against pre-rename
  baseline
- Acceptance: existing Vitest suites pass with updated expectations;
  new ShortHash + labels helper have unit tests
- Dependencies: none — can start immediately (API contract unchanged)
- Risk: low (cosmetic; widest blast radius if a label is wrong, but
  no data risk)

### Track 3: SSE display-label registry (Rust + JS)

- Handoff: `docs/design/2026-05-27-autoresearcher-sse-registry-handoff.md`
- Owner: backend engineer with dashboard familiarity
- Scope: ~50 lines Rust + ~30 lines JS + one new unit test
- Allowed paths: `crates/xvision-dashboard/src/sse.rs`,
  `crates/xvision-dashboard/static/js/bus.js`,
  `crates/xvision-dashboard/tests/sse_smoke.rs`
- Forbidden paths: `crates/xvision-engine/src/autoresearch/progress.rs`
  (wire schema stays), any orchestrator emission code
- Verification: all 13 event variants have a display label, SSE
  smoke test asserts the label is in the payload
- Acceptance: live cycle viewer (when AR-3 ships) reads display
  labels; vanilla-JS dashboard already in static/ renders them via
  bus.js
- Dependencies: none — can start immediately
- Risk: low (additive; doesn't change wire format)

### Track 4: CLI rename with deprecation aliases (Rust)

- Handoff: `docs/design/2026-05-27-autoresearcher-cli-rename-handoff.md`
- Owner: backend engineer
- Scope: 4 verb renames + 8 flag renames + help text rewrites + 11
  human-output string changes, all with deprecation aliases; CLI
  surface snapshot test update
- Allowed paths: `crates/xvision-cli/src/commands/autoresearch.rs`,
  `crates/xvision-cli/src/commands/memory.rs`,
  `crates/xvision-cli/src/commands/flywheel.rs`,
  `crates/xvision-cli/src/lib.rs`,
  `crates/xvision-cli/tests/cli_surface_snapshot.json`,
  `crates/xvision-cli/tests/autoresearch_cli.rs`
- Forbidden paths: `crates/xvision-engine/**` (engine API stays),
  any migration, any frontend code
- Verification: `cargo test -p xvision-cli` passes, the alias path
  works (smoke: `xvn autoresearch promote --help` prints deprecation
  note and forwards to `activate`; `xvn memory promote --help`
  forwards to `distill`), banned-words grep on `--help` output of
  every subcommand returns nothing
- Acceptance: snapshot test diff is reviewable and reflects
  exactly the renames listed in the handoff (no incidental changes)
- Dependencies: none for shipping the rename; but the skills-and-docs
  sweep (track 5) is blocked by this track
- Risk: medium (external consumers — operator scripts, the published
  skill, MANUAL.md examples all run against the CLI; deprecation
  aliases mitigate)

### Track 5: Skills + docs sweep (mechanical)

- Handoff: `docs/design/2026-05-27-autoresearcher-skills-docs-sweep-handoff.md`
- Owner: technical writer or engineer
- Scope: rewrite 2 skill files; mechanical replacement sweep on
  MANUAL.md, dashboard wiki CLI reference, README.md, and
  marketing/
- Allowed paths: `.claude/skills/xvision/autoresearch-ops/SKILL.md`,
  `.claude/skills/xvision/flywheel-ops/SKILL.md`, `MANUAL.md`,
  `crates/xvision-dashboard/wiki/cli-reference.md`, `README.md`,
  `marketing/**` (proofread carefully)
- Forbidden paths: any spec/plan/note/research/evidence/review doc;
  any code; any team coordination doc
- Verification: banned-words grep returns nothing in the swept files;
  every CLI example in every updated doc actually runs
- Acceptance: skill files read cleanly with new vocabulary; MANUAL.md
  is consistent end-to-end; no leaked dev jargon in user-facing copy
- Dependencies: **blocked by Track 4** (CLI rename) — the new verbs
  and flags must exist before skill examples can reference them,
  and deprecation aliases must exist so the documented commands
  still work on the previous release
- Risk: low (mechanical), but skills can affect agent routing if
  description wording changes too much — handoff has a push-back
  note about this

## Wave-level acceptance gate

The wave is complete when:

1. All five tracks have landed on `main`.
2. End-to-end smoke: a fresh operator (someone who hasn't used
   xvision before) can read MANUAL.md, follow the autoresearch ops
   skill's standard flow, look at the dashboard memory page, and
   not encounter any of the banned terms: `epsilon`, `holdout`,
   `mutation`, `mutator`, `ghost`, `quarantined`, `merkle`, `BLAKE3`,
   `Ed25519`, `promote`/`promotion`/`promoted` (in autoresearch
   context), `demote`/`demoted`, `mint`.
3. The terminology lock at
   `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`
   has its "Implementation deliverables" checklist marked complete
   (or moved into a "Done" appendix).
4. CLAUDE.md "Operator-facing names (autoresearcher subsurface)"
   section is unchanged (it's the canonical pointer; no edit needed
   unless a track changes the lock).

## Suggested execution order

Two parallelization paths depending on team size:

**Single-engineer rollout** (sequential, ~3–5 days):
1. Track 1 (spec amendment, 1–2 hr)
2. Track 4 (CLI rename, 1 day)
3. Track 3 (SSE registry, 0.5 day)
4. Track 2 (frontend, 1–2 days)
5. Track 5 (skills + docs sweep, 0.5 day)

**Three-engineer rollout** (parallel, ~2 days):
- Engineer A: Track 1 → Track 4 → Track 5 (sequenced — CLI must land
  before docs sweep)
- Engineer B: Track 2 (frontend, independent)
- Engineer C: Track 3 (SSE registry, independent)

Tracks 1, 2, and 3 have no blockers. Track 5 is blocked by Track 4.
Track 4 has no upstream blockers but should ship with deprecation
aliases live for at least one release before any track 5 work
removes the old verb references from external-facing docs.

## Conflict zones

None of the five tracks touch the same file. Single-writer per file
across the wave; no `team/CONFLICT_ZONES.md` entries needed.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| CLI rename breaks operator scripts | Deprecation aliases for one release; stderr note on use; document migration in the release notes |
| Skill description rewrite changes agent routing accuracy | Track 5 handoff flags this; consider a router benchmark sweep (Murat skill-pack technique) after the rewrite ships to catch regressions |
| Frontend rename hits a layout that doesn't fit longer labels | Track 2 handoff anticipates this — switch to label-on-top form layout when needed; don't abbreviate to fit old column widths |
| Documentation drift between MANUAL.md, the wiki, and the skill | Track 5 acceptance requires all three to pass the same banned-words grep; lock in CI if feasible |
| Marketing copy gets damaged in mechanical replace | Track 5 handoff calls out marketing/ as proofread-required, not search-and-replace |

## Reference

- Terminology lock: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`
- Audit and rationale: `docs/superpowers/notes/2026-05-27-autoresearcher-plain-language-audit.md`
- Five handoff docs: `docs/design/2026-05-27-autoresearcher-*-handoff.md`
- Project-wide terminology note: `/CLAUDE.md` §Terminology → "Operator-facing names (autoresearcher subsurface)"
