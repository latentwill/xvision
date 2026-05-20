# Intake — 2026-05-20 — Update in-repo skills for new xvn functionality

Operator ask 2026-05-20: the in-repo Claude skills under
`.claude/skills/` are out of date with the xvn CLI surface that's
landed across the CLI agent workbench waves (B/C/D), the experiment
orchestrator, the baseline auto-comparison, the scenario regime
labels, the MCP peer expansion, and the deploy-image preflight +
cleanup work. Agents loading these skills are getting a stale picture
of what xvn can do and how to drive it.

This intake belongs on the **immediate board**, not V2. The skills are
the agent-facing docs surface — drift here makes every other wave more
expensive because agents either guess or rediscover the verbs from
`xvn --help`.

## Source

Operator request 2026-05-20, immediately after the deploy-image
preflight + cleanup work landed (#377, 2026-05-20). Same conversation
that filed the AB-compare share-image and strategy-name-in-labels
intake adds.

## Skills in scope

In-repo (`/Users/edkennedy/Code/xvision/.claude/skills/`):

- `xvision-cli/SKILL.md` — USAGE-side orientation: running `xvn`
  verbs, the dashboard, MCP tools, strategy/eval workflows, live
  Tailscale nodes, remote CLI job surface.
- `xvision-cli-qa/SKILL.md` — direct API / CLI QA recipes for
  Strategy / Scenario / Eval CRUD, manifest drift, duplicate records,
  invalid model resolution, raw HTTP evidence collection.
- `xvision-dev/SKILL.md` — CONTRIBUTING-side orientation: building,
  testing, navigating the Rust workspace + Vite SPA, team-coordination
  board, deployment guardrails, locked-in invariants.
- `cycle-migration/SKILL.md` — SQLx migration authoring; touched
  recently (migration 022→023→024 renumbers, scenario regime labels)
  so worth re-verifying terminology + the dual-migration-dir layout
  is current.
- `board-sync/SKILL.md` — if it references CLI verbs for
  status/queue inspection, those should be checked too.

## What changed in xvn that the skills don't yet reflect

New verbs / flags / behaviors that landed since the skills were
last reviewed (recent commits — verify against current `xvn --help`
before writing, this list is the starting hopper, not the spec):

- `xvn experiment …` — full experiment orchestrator verb family
  (intake #8 / commit `78c0d7b`, pick → batch → bind → result_json).
  CLI examples and the "what is an experiment vs a strategy vs an
  eval" mental model probably aren't in any skill yet.
- `xvn strategy create --hypothesis …` — hypothesis flags
  (intake #7 / commit `96d25ce`). Wraps the new `Hypothesis` struct
  on `Strategy` (`c09b120`).
- `xvn scenario classify` + `xvn scenario set-regime` — scenario
  regime labeling with the new regime column in `select` output
  (commit `29fc9ec`).
- `xvn eval compare --markdown` — now includes a Baseline (buy_hold)
  column (commit `64938b3`) backed by baseline auto-comparison in
  `BacktestExecutor` (commit `4cc798c`).
- 6 new MCP tool peers for the CLI workbench verbs
  (commit `fe55dde` — `mcp-surface-parity-for-new-verbs`). The
  `xvision-cli` skill's MCP-tools section needs the new peers added.
- `scripts/deploy-image.sh` preflight + obsolete-image cleanup
  (commit `8fd7d48`, PR #377). The `xvision-dev` skill's deploy
  guardrails section should reflect the preflight `df -h` step and
  the cleanup behavior — and the memory note about xvn host disk
  pressure (`project_xvn_host_disk_pressure.md`) is what motivated
  this; cross-link.
- `AgentSlot.temperature` is now threaded through all call sites
  (commit `ad9b1f7`). Any skill section that documents agent slot
  config should mention temperature as a per-slot field.
- `ShortRunId` / `EvalRunLabels` (frontend, commit `1da17ae`) — not
  CLI but the QA skill's eval-id assertions may need updating if
  they grep for the long ULID.

## Asks (not yet decomposed)

1. **Audit each skill against the current `xvn --help` tree.** For
   every verb / subverb / flag the skill mentions, confirm it still
   exists and the example still runs. Delete or update stale verbs;
   add coverage for the new ones listed above.
2. **xvision-cli skill: add experiment-orchestrator section.** The
   experiment verb family is genuinely new — needs a short
   "experiment vs strategy vs eval" mental-model paragraph plus a
   worked pick → batch → bind → result_json example.
3. **xvision-cli skill: refresh MCP tools list.** Enumerate the 6
   new MCP peers from `fe55dde` so MCP-driving agents don't fall
   back to the CLI shim.
4. **xvision-cli-qa skill: cover new CRUD surfaces.** Hypothesis on
   strategies, regime labels on scenarios, baseline column in
   compare — QA assertions and curl recipes for each.
5. **xvision-dev skill: update deploy-image guardrails.** Document
   the preflight `df -h` step, the obsolete-image cleanup, and
   reaffirm the "build host vs deploy host" split. Cross-link to
   `project_xvn_host_disk_pressure.md` in user memory and to
   `CLAUDE.md` deployment guardrails.
6. **cycle-migration skill: verify migration-number guidance.** The
   021/022/023/024 renumber dance happened twice in the last two
   weeks. The skill should state the current next-available number
   and the dual-dir rule (`xvision-core` + `xvision-engine`) plainly.
7. **Drift prevention.** Either (a) add a CI lint that diffs
   skill-cited verbs against `xvn --help`, or (b) attach a short
   "skills owner: <track>" footer to every skill so future intakes
   know who to ping when a verb changes. Pick one, not both.

## Non-goals / out of scope

- Adding new skills. This is a refresh wave, not an expansion. New
  skills come through a separate intake.
- Touching the third-party skill bundles (`byreal/`, `mantle/`,
  `cline/`). Those are vendored docs and out of scope.
- Re-writing CLAUDE.md or the team-coordination board. Skills should
  *link* to those, not duplicate them.

## Verification (when a track lands)

- For every code example shown in a skill, run it (or its --help
  variant) against a current `xvn` build and confirm exit 0.
- `rg "xvn " .claude/skills/` should match only verbs in the current
  `xvn --help` tree.
- Land the skills refresh as a single PR (or one PR per skill if
  size demands), not a drip of one-flag fixes.

## Related artifacts

- `.claude/skills/xvision-cli/SKILL.md`
- `.claude/skills/xvision-cli-qa/SKILL.md`
- `.claude/skills/xvision-dev/SKILL.md`
- `.claude/skills/cycle-migration/SKILL.md`
- Recent commits: `78c0d7b`, `96d25ce`, `c09b120`, `29fc9ec`,
  `64938b3`, `4cc798c`, `fe55dde`, `8fd7d48`, `ad9b1f7`, `1da17ae`.
- User memory: `project_xvn_host_disk_pressure.md`,
  `project_xvn_xvnej_environments.md`.
