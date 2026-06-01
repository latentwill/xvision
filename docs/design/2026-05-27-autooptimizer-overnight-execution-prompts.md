# Overnight execution prompts for the autooptimizer spine

> Date: 2026-05-27
> For: operator (you) running an autonomous coding agent overnight
> Companion: `docs/superpowers/plans/2026-05-27-autooptimizer-master-implementation-spine.md`

## Honest framing first

You cannot execute the entire autooptimizer spine in one overnight
session. The spine is 4–6 weeks single-engineer or 2–3 weeks with a
team per Part 7 of the spine doc itself. An autonomous coding agent
is not faster than a human at writing the cryptographic substrate
of AR-1, integrating against the eval engine, or making the dozens
of design judgments the dashboard needs.

What an autonomous agent IS good at overnight:

- Mechanical work with clear acceptance gates (renames, sweeps,
  test snapshot updates)
- Following an already-written plan that decomposes into TDD tasks
  with red→green→commit per step
- Producing draft specs from existing context for operator review
  the next morning
- Running multiple parallel streams in worktrees so coordinated
  rollouts land while you sleep

What an autonomous agent is BAD at overnight:

- Cross-cutting design decisions (e.g., "should the autoreason
  tournament use 3 or 7 judges?")
- Anything involving the deploy guardrails in CLAUDE.md (one stray
  cargo invocation on a 4GB box and the night is wasted)
- Verifying its own output on operator-only judgment calls
  (typography, copy voice, demo readiness)
- Recovering from unexpected blocks (the agent will either spin or
  fabricate — write the prompt so it halts and documents instead)

Below are three calibrated overnight subsets, ranked by
risk-adjusted value, with copy-paste-ready prompts for each. Plus a
multi-agent parallel variant.

---

## Subset A — Terminology rollout (recommended for first overnight)

**Why this one.** Lowest risk, highest visible-progress yield. Five
tracks already specced as self-contained handoffs. Three of them
(Tracks 1, 3, 4) have hard acceptance gates and no design judgment
required. Track 5 is mechanical but blocked by Track 4. Track 2
(frontend) needs design judgment on label wrapping and is best left
for a daylight pass.

**What ships overnight.** A spec amendment, an SSE display-label
registry with tests, a CLI rename with deprecation aliases and an
updated snapshot test. Probably also Track 5 (docs sweep) if Track
4 completes early.

**Estimated wall-clock for one agent**: 4–6 hours.

### Prompt for Subset A

Copy/paste this as your starting message to the agent. Replace
`<your-machine-name>` with something memorable for the HANDOFF
filename.

```text
You are an autonomous coding agent executing the autooptimizer
terminology rollout overnight. You work in the xvision repo at
/Users/edkennedy/Code/xvision.

READ FIRST (in this order, then stop and confirm you understand
before writing any code):
1. /Users/edkennedy/Code/xvision/CLAUDE.md — project conventions and
   HARD RULES on cargo/docker
2. /Users/edkennedy/Code/xvision/docs/superpowers/plans/2026-05-27-autooptimizer-master-implementation-spine.md — the master plan
3. /Users/edkennedy/Code/xvision/docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md — the locked vocabulary (the contract)
4. /Users/edkennedy/Code/xvision/team/intake/2026-05-27-autooptimizer-terminology-rollout.md — the wave structure with track dependencies

If the superpowers:executing-plans skill is available, USE IT for
each track below. If not, follow this loop per task: read the
handoff section → write failing test → implement → run test → commit
→ move on. Do not skip the test step.

WORK PLAN — execute these three tracks IN ORDER. Each track must
hit its acceptance gate before the next one starts. Do not parallelize.

================================================================
TRACK 1 — Spec amendment (doc-only, ~1 hour)
================================================================
Handoff: /Users/edkennedy/Code/xvision/docs/design/2026-05-27-autooptimizer-spec-amendment-handoff.md

Allowed paths (only file you may modify):
- docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md

Acceptance gate: every term in the handoff's parenthetical table has
its first-mention annotation added exactly once; §0.1 exists with
the cross-reference paragraph and the cryptographic-primitives
sentence; the spec still reads cleanly end-to-end.

Verification: read the modified spec end-to-end before committing.
If any parenthetical reads as intrusive editorial markup, rephrase.

Commit message format: "docs(autooptimizer): add §0.1 operator
vocabulary + first-mention annotations"

================================================================
TRACK 3 — SSE display-label registry (Rust + JS, ~1–2 hours)
================================================================
Handoff: /Users/edkennedy/Code/xvision/docs/design/2026-05-27-autooptimizer-sse-registry-handoff.md

Allowed paths:
- crates/xvision-dashboard/src/sse.rs
- crates/xvision-dashboard/static/js/bus.js
- crates/xvision-dashboard/tests/sse_smoke.rs

Forbidden:
- crates/xvision-engine/src/autooptimizer/progress.rs (wire schema stays)
- Any orchestrator emission code

Acceptance gate: display_label() helper exists in sse.rs covering
all 13 AutoOptimizerEvent variants; SSE payload includes display_label
field; bus.js has the fallback map; sse_smoke test asserts the label
is in the payload.

Verification commands:
- `cargo test -p xvision-dashboard --test sse_smoke` passes
- `cargo build -p xvision-dashboard` passes
- Manually grep the variants: count of arms in display_label() match
  must equal count of AutoOptimizerEvent variants in
  crates/xvision-engine/src/autooptimizer/progress.rs (or wherever the
  enum lives)

Commit messages: one for the Rust helper, one for the bus.js fallback,
one for the test update.

================================================================
TRACK 4 — CLI rename with deprecation aliases (Rust, ~2–3 hours)
================================================================
Handoff: /Users/edkennedy/Code/xvision/docs/design/2026-05-27-autooptimizer-cli-rename-handoff.md

NOTE the amendment: `xvn memory promote` renames to `xvn memory
distill`, NOT to `xvn memory activate`. The single-Pattern
`xvn memory activate <id>` form stays unchanged. See the handoff's
"Memory verb structure (per lock amendment 2026-05-27)" section.

Allowed paths:
- crates/xvision-cli/src/commands/autooptimizer.rs
- crates/xvision-cli/src/commands/memory.rs
- crates/xvision-cli/src/commands/flywheel.rs (help text only)
- crates/xvision-cli/src/lib.rs (only if top-level help text references renamed verbs)
- crates/xvision-cli/tests/cli_surface_snapshot.json
- crates/xvision-cli/tests/autooptimizer_cli.rs

Forbidden:
- crates/xvision-engine/** (engine API stays developer-surface)
- Any migration
- Any frontend code

Acceptance gate: every rename in the handoff's verb-rename and
flag-rename tables is implemented; every old verb/flag is preserved
as a hidden clap alias with a stderr deprecation note; the CLI
surface snapshot test is updated and passes; cargo test -p
xvision-cli passes.

Verification commands:
- `cargo test -p xvision-cli` passes
- Smoke: `cargo run -p xvision-cli -- autooptimizer promote --help`
  should print the deprecation note and forward to activate's help
- Smoke: `cargo run -p xvision-cli -- memory promote --help` should
  print the deprecation note and forward to distill's help
- Banned-words grep: run `cargo run -p xvision-cli -- autooptimizer
  gate --help 2>&1 | grep -iE 'epsilon|holdout'` — should return
  nothing

Commit messages: one per logical group (verbs, flags, output
strings, snapshot test).

================================================================
HARD RULES (from CLAUDE.md — NON-NEGOTIABLE)
================================================================
- NEVER run cargo on a deploy/remote host. You are on the local
  development workstation — assume cargo is safe here, but if you
  somehow find yourself elsewhere (Coolify, GHCR, an Oracle ARM box),
  STOP IMMEDIATELY.
- NEVER build Docker images during this work. Track 4's verification
  uses cargo run, not docker compose.
- Always set CARGO_TARGET_DIR if you create a temporary worktree:
  `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"`
- F+L+T leakage protection is non-negotiable. If any change you make
  touches memory recall, STOP and add a leakage regression test
  before proceeding.
- Per-task evidence: code diff + passing test + CLI transcript for
  CLI changes. Don't skip the transcript.

================================================================
STOP CONDITIONS (halt and document, do not improvise)
================================================================
If any of the following happen, STOP. Write a HANDOFF-<your-machine-name>.md
file at the repo root summarizing: which track you were on, which
task within the track, what you completed, what failed, the error
output verbatim, what you'd try next.

Stop conditions:
- Any acceptance gate fails after a reasonable retry
- Any test fails that wasn't failing before your changes
- Any change required by the handoff falls outside your allowed paths
- An ambiguity in the handoff that needs operator judgment
- You're about to invent vocabulary not in the terminology lock
- More than 90 minutes elapsed on a single task without progress
- More than 6 hours total elapsed on the rollout

DO NOT improvise around blocks. The operator picks up in the morning;
clean halt is far more valuable than partial progress hidden in a
broken state.

================================================================
SUCCESS CRITERIA
================================================================
By morning:
- Track 1 committed and the spec reads cleanly
- Track 3 committed with passing tests
- Track 4 committed with passing tests and working deprecation aliases
- A single SUMMARY-<your-machine-name>.md file at the repo root listing
  what you did, the commit SHAs, and any followups you noticed for the
  operator

If you have time after Track 4 lands, attempt Track 5 (skills + docs
sweep) per /Users/edkennedy/Code/xvision/docs/design/2026-05-27-autooptimizer-skills-docs-sweep-handoff.md.
Track 5 has the same allowed/forbidden paths as documented in the
handoff. Same stop conditions.

Begin.
```

---

## Subset B — Draft the two missing specs (lower-risk parallel option)

**Why this one.** The spine identifies two unwritten specs as gaps:
the autoreason tournament integration (Phase 4) and skill discipline
adoption (Phase 6). Both are spec-shaped work — the bottleneck is
written drafts the operator can review, not lines of code. Drafting
both overnight gives you two operator-review-ready specs by morning.

**Why it's parallelizable with Subset A.** Different repo paths, no
overlap, no compilation. You can run Subset A in one agent session
and Subset B in another against the same checkout.

### Prompt for Subset B

```text
You are an autonomous spec-drafting agent. You work in the xvision
repo at /Users/edkennedy/Code/xvision but you write only spec docs,
no code.

READ FIRST:
1. /Users/edkennedy/Code/xvision/docs/superpowers/plans/2026-05-27-autooptimizer-master-implementation-spine.md
   — Parts 6 (gaps), 3 (Phase 4 and Phase 6 framing), 4 (registry)
2. /Users/edkennedy/Code/xvision/docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md
   — the karpathy spec (for Phase 4 context — the tournament wraps
   the mutator this spec defines)
3. /Users/edkennedy/Code/xvision/docs/superpowers/plans/2026-05-09-autooptimizer-1-mutator-lineage-gate-seal.md
   — the AR-1 plan (for the Mutator API shape Phase 4 will integrate with)
4. /Users/edkennedy/Code/xvision/.claude/skills/xvision/autooptimizer-ops/SKILL.md
   and /Users/edkennedy/Code/xvision/.claude/skills/xvision/flywheel-ops/SKILL.md
   (for Phase 6 context — these are the skills the discipline applies to)
5. The Murat skill-pack release notes are public knowledge; you may
   reference the pack's methodology (router benchmark, body
   hardening, negative controls with expected_primary_skill: "none",
   strict skill health validators) without citing source files.

WORK PLAN — produce two draft specs. Both are operator-review
drafts, not final. Mark each "STATUS: draft for operator review".

================================================================
SPEC 1 — Autoreason tournament integration design
================================================================
Path: /Users/edkennedy/Code/xvision/docs/superpowers/specs/2026-05-28-autoreason-tournament-integration-design.md

Required sections:
1. Purpose and scope (why now; one paragraph)
2. Architecture: how the tournament wraps the existing AR-1 Mutator
   API. Three candidates per round (incumbent / adversarial revision /
   synthesis). Blind Borda judge panel. Numeric gate runs AFTER the
   tournament picks a winner.
3. Locked decisions (table) — including: token budget per round (3×
   single-shot Haiku is the starting point), judge panel size
   (autoreason evidence shows 3 minimum, 7 converges 3× faster), the
   "do nothing" first-class option semantic, what happens to
   "incumbent wins" runs (recorded as null-change accepted, not
   dropped).
4. Failure modes addressed (the three autoreason findings: prompt
   bias, scope creep, lack of restraint) mapped to xvision failure
   modes from the karpathy spec §11.
5. Cost analysis: 3× LLM tokens; default off (opt-in via config);
   when on, recommended for which lineages.
6. Open questions for operator decision (judge panel size: 3 or 7;
   tournament-on-every-cycle vs tournament-on-stuck-lineages-only;
   whether the judge is the same Sonnet that AR-2's Finding judge
   uses or a separate fresh agent).
7. Integration plan reference: this spec is followed by an
   implementation plan to be written after operator review.

Tone: matches the karpathy spec — factual, decision-table-heavy,
honest about tradeoffs. Around 800–1500 words.

Operator-facing vocabulary throughout per
/Users/edkennedy/Code/xvision/docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md
— Mutator stays in developer surface, "Experiment writer" in
operator-facing copy. "Tournament" is a new operator concept that
should be added to the lock if this spec lands.

================================================================
SPEC 2 — Skill discipline adoption design
================================================================
Path: /Users/edkennedy/Code/xvision/docs/superpowers/specs/2026-05-28-skill-discipline-adoption-design.md

Required sections:
1. Purpose and scope (what discipline, what it applies to)
2. Current state audit: list every skill under
   /Users/edkennedy/Code/xvision/.claude/skills/xvision/ with a
   one-line description and whether it currently has: ownership
   statement, do-not-activate routing, examples, gotchas.
   (You'll need to glob the directory and read each SKILL.md
   to fill this in.)
3. Locked decisions (table) — including: which validators run in
   CI (strict skill health, router accuracy, activation fixtures),
   the body-hardening template (ownership + do-not-activate +
   examples + gotchas + integration boundaries + references),
   threshold for skill-body health score below which CI fails,
   negative-control corpus (expected_primary_skill: "none" cases)
   to add for xvision domain.
4. The two surfaces this applies to: (a) skill files under
   .claude/skills/xvision/, (b) per-slot prompts inside AgentSlot
   (the trader/intern/regime slots). Each surface has different
   validation but same disciplines.
5. Migration path: how existing skills get upgraded. One pull
   request per skill (small, reviewable) or one big sweep?
6. Open questions for operator decision (CI gate severity: warning
   or fail; baseline accuracy threshold; whether the router benchmark
   needs a real LLM in CI or a deterministic stub).

Tone: technical-discipline doc. Around 800–1500 words.

================================================================
STOP CONDITIONS
================================================================
If any of the following happen, STOP and write
HANDOFF-spec-draft-<your-machine-name>.md at the repo root with the
partial drafts and the question that blocked you:

- You need operator judgment on a tradeoff the spine doesn't preselect
- You'd be inventing technical claims (e.g., "Murat pack reported
  X%") without source — write "[needs citation]" inline and continue;
  do not fabricate numbers
- More than 3 hours total elapsed

DO NOT add example skills to .claude/skills/. DO NOT write
implementation plans yet — the specs come first, then operator
review, then plans.

================================================================
SUCCESS CRITERIA
================================================================
By morning: two draft spec files committed, each marked "STATUS:
draft for operator review", with explicit "Open questions for
operator decision" sections. SUMMARY-spec-draft-<your-machine-name>.md
at repo root listing what you produced and which open questions
need the operator first.

Begin.
```

---

## Subset C — AR-1 Phase A infrastructure (Rust, higher risk)

**Why this one.** Real feature work — the cryptographic substrate
that unblocks Phase 1 → 2 → 3 → 4 progress. Most leverage if it
works, biggest cleanup if it doesn't.

**Why it's higher risk overnight.** Cryptographic code is easy to
get subtly wrong. The AR-1 plan is 3797 lines and the agent will
need to navigate it carefully. The eval engine integration is the
chokepoint — wrong assumptions about the eval engine API and the
whole night's work is rework.

**Recommended only if** you've already had at least one successful
overnight run with Subset A and trust the agent's discipline in
this codebase. If this is the first overnight, prefer Subset A.

### Prompt for Subset C

```text
You are an autonomous coding agent executing AR-1 Phase A
(infrastructure tasks 1–6) overnight. You work in the xvision repo
at /Users/edkennedy/Code/xvision.

READ FIRST:
1. /Users/edkennedy/Code/xvision/CLAUDE.md — hard rules
2. /Users/edkennedy/Code/xvision/docs/superpowers/plans/2026-05-27-autooptimizer-master-implementation-spine.md — Phase 1
3. /Users/edkennedy/Code/xvision/docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md — the design context
4. /Users/edkennedy/Code/xvision/docs/superpowers/plans/2026-05-09-autooptimizer-1-mutator-lineage-gate-seal.md — the AR-1 plan; THIS IS YOUR EXECUTION SCRIPT
   (it's ~3800 lines; read tasks 1 through 6 first; reference the rest only as needed)

The plan is TDD-shaped: write failing test → implement → run test →
commit. Follow it literally. The plan has been hand-checked; do not
deviate from the file structure, the test cases, or the commit
messages it specifies.

If the superpowers:executing-plans skill is available, USE IT and
follow its workflow. If not, the plan itself is detailed enough to
execute step-by-step.

WORK PLAN — execute exactly tasks 1 through 6 of the AR-1 plan, IN
ORDER. Do not start task N+1 until task N's verification passes.

Task 1: Cargo deps + module wiring
Task 2: Content-hash helpers (BLAKE3 + canonical JSON)
Task 3: Filesystem blob store
Task 4: Bundle ↔ markdown program-view
Task 5: AutoOptimizerConfig (autooptimizer.toml loader)
Task 6: SessionCommitment + operator key

After Task 6: STOP. Do not proceed to Task 7 (which begins the
mutator LLM integration — that needs operator presence for the
LLM-provider setup).

PREREQUISITE CHECK (run BEFORE Task 1):
`cargo test -p xvision-engine eval 2>&1 | grep "test result"`
must return at least one passing test. If it doesn't, STOP — the
eval engine isn't in the state AR-1 expects and the rest of the
work will fail. Document in HANDOFF.md.

HARD RULES (CLAUDE.md):
- NEVER run cargo on a remote/deploy host. Local workstation only.
- NEVER build Docker images during this work.
- One commit per task (per the plan's commit message templates).
- The Rust toolchain on the local checkout must be the same one used
  for the workspace; do not install rustup overrides.

PER-TASK EVIDENCE (don't skip):
- The failing test you wrote (verbatim)
- The cargo test command and its passing output
- The diff of files changed in that task
- The commit SHA

STOP CONDITIONS:
If any of the following happen, STOP. Write HANDOFF-ar1-phase-a.md
at repo root with what you completed, what failed, the error
verbatim, and what you'd try next.

- The prerequisite check fails
- Any test in a task fails after one retry
- The plan's listed file structure conflicts with what exists today
  (e.g., a file already has a conflicting definition)
- You need to invent a struct field, function signature, or test
  case not specified by the plan
- The plan references a downstream component (eval engine, mutator)
  in a way that requires assumptions you can't verify
- More than 90 minutes on a single task
- More than 7 hours total

SUCCESS CRITERIA:
By morning: tasks 1–6 committed in order, all tests passing,
SUMMARY-ar1-phase-a.md at repo root listing the commits and any
plan-deviations (there should be none; flag anything).

DO NOT attempt task 7 even if you finish early. The mutator step
needs operator presence.

Begin.
```

---

## Multi-agent parallel variant (recommended if you have the setup)

If your overnight tooling supports multiple agent sessions in git
worktrees (Claude Code does), you can run Subsets A and B in
parallel since they touch disjoint files:

### Agent 1 (worktree: `terminology-rollout`)
Run the Subset A prompt against a worktree at
`~/code/xvision-worktrees/terminology-rollout` checked out from
main. The CLAUDE.md cache discipline rule applies: set
`CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"` first thing in the
session.

### Agent 2 (worktree: `spec-drafts`)
Run the Subset B prompt against a worktree at
`~/code/xvision-worktrees/spec-drafts`. No compilation needed, so
no CARGO_TARGET_DIR concern.

By morning you'd have:
- 3 (or 4) terminology rollout tracks committed on the
  terminology-rollout worktree
- 2 draft specs committed on the spec-drafts worktree

Both worktrees rebase cleanly onto main since they touch disjoint
paths. Merge in whatever order makes sense.

**Why NOT to run Subset C in parallel with the others.** Subset C
touches `crates/xvision-cli/src/commands/autooptimizer.rs` (extending
it with new verbs in Tasks 1 and 7+). Subset A's Track 4 also
touches that file (renaming verbs). They'd collide. Either run C
sequentially after A is merged, or skip C this night.

---

## What to look at in the morning

For Subset A:
- The three SUMMARY-*.md files at repo root
- `git log --oneline main..HEAD` on the working branch — should
  show 1 commit for Track 1, 3 commits for Track 3, ~5 commits
  for Track 4
- Smoke: `cargo test -p xvision-dashboard --test sse_smoke && cargo test -p xvision-cli`
- Smoke: `cargo run -p xvision-cli -- autooptimizer promote --help`
  should print a deprecation note

For Subset B:
- The two draft spec files
- Each spec's "Open questions for operator decision" section — these
  are the decisions you need to make before the implementation
  plans can be written

For Subset C:
- `git log --oneline` should show 6 commits matching the plan's
  commit message templates
- `cargo test -p xvision-engine` should pass
- HANDOFF.md should NOT exist (its existence means the agent halted)

For any subset:
- If HANDOFF-*.md exists at repo root, read it first. The agent
  halted for a reason; don't manually push past whatever it caught.
- If SUMMARY-*.md exists and HANDOFF-*.md doesn't, the agent
  completed cleanly. Trust the summary but verify the commits.
- If neither exists, the agent likely crashed or was interrupted.
  Check the session logs.

---

## What NOT to attempt overnight

- Phase 2 (AR-2 cycle orchestrator). Too many design judgments
  (parent policy, judge selection, canary parent shape) and too much
  cross-component integration. Wait for daylight.
- Phase 3 (AR-3 dashboard). UI design needs operator judgment on
  every screen.
- Phase 5 Track 2 (frontend rename). Label wrapping and form layout
  judgments need eyes on rendered output.
- Anything touching the eval engine's accuracy work. That's its own
  research-grade investigation.
- Marketplace integration. Has its own spine; not in scope here.
- Production deploys. Deploy guardrails exist for a reason.

---

## A note on the agent itself

These prompts are written for an agent with: ability to read/write
files, run shell commands, run cargo, follow a task-list workflow,
and halt on its own judgment when a stop condition fires. If your
overnight agent doesn't have one of those (no shell, no halt
discipline), the prompts need adjusting:

- No shell: drop Subset C entirely; Subset A's verification steps
  need a human-in-the-loop substitute.
- No halt discipline: add a deadline timer; force the agent to
  produce a summary every hour even if it would rather keep working.
- No file-tools beyond read: probably can't do these subsets
  overnight — agent shape is wrong for the work.

The xvision repo already has the `superpowers:executing-plans` and
`superpowers:verification-before-completion` skills documented in
CLAUDE.md and called out in the AR-1 plan's preamble. If your
overnight agent loads those skills, the prompt above can be much
shorter — the skills enforce most of the discipline. Skills do half
the work; the prompts above are calibrated for an agent that may
not have skills loaded.

---

## Reference

- Master spine: `docs/superpowers/plans/2026-05-27-autooptimizer-master-implementation-spine.md`
- Terminology lock: `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`
- Wave intake: `team/intake/2026-05-27-autooptimizer-terminology-rollout.md`
- All five terminology-rollout handoffs: `docs/design/2026-05-27-autooptimizer-*-handoff.md`
- AR-1 plan: `docs/superpowers/plans/2026-05-09-autooptimizer-1-mutator-lineage-gate-seal.md`
- CLAUDE.md (hard rules): `/CLAUDE.md`
