# 2026-05-22 worktree cleanup — follow-up log

Pre-deploy worktree sweep. All worktrees listed below were removed via
`git worktree remove --force`; uncommitted state captured here so it can
be re-investigated if needed. Branches remain on origin unless noted.

## Removed — `.claude/worktrees/agent-*` (locked agent worktrees)

### agent-aa89f5dbe1084ae22 — `task/strategy-slot-prompt-resolution` @ b1951d4

- Branch tip is `b1951d4` (conductor sweep-3), now in origin/main.
- Dirty: **43 files changed, 333 / -4913** — heavy edits across
  `crates/xvision-engine/src/agent/{execute,llm,mod,pipeline,recovery}.rs`,
  `crates/xvision-engine/src/eval/executor/{backtest,paper,trader_output}.rs`,
  multiple `agent_*` tests, `frontend/web/src/features/agent-runs/TraceDock.tsx`,
  team contracts (deletes the agent-graph-composition spec). Deletes
  `crates/xvision-engine/src/agent/summarize.rs`.
- Looked like an in-flight strategy-slot prompt-resolution attempt with
  wide blast radius. No commit; abandoned.
- Followup: if strategy-slot prompt resolution is still desired, re-scope
  from a fresh branch — the contract is at
  `team/contracts/strategy-slot-prompt-resolution.md` if not already
  archived; check `team/archive/` if missing.

### agent-ac977c02ca4d9e5d1 — `task/harness-recovery-context-overflow` @ c37addc

- **Obsolete.** Feature shipped via PR #513 (merged 2026-05-22 04:51,
  commit `258732b`). Worktree's `origin/pr/513` was `: gone`.
- Dirty: **100 files changed, 444 / -4745** — huge spread, including
  deletes of `agent_recovery_malformed_json.rs` and
  `agent_recovery_schema_missing_field.rs`, `forget_undo.rs`,
  `strategy_attested_with.rs`, plus the capability-first spec deletion.
  Most of this is a stale post-conductor rebase attempt against an old
  baseline.
- Followup: probably nothing; the merged PR is the source of truth.

### agent-ad00ca3db1c1a5bf5 — `task/harness-recovery-schema-missing-field` @ 56d5252

- **Obsolete.** Feature shipped via PR #516 (merged 2026-05-22 04:50,
  commit `8f235f4`). `origin/pr/516` was `: gone`.
- Dirty: **17 files changed, 206 / -959** — `agents/{store,validate}.rs`,
  `trader_noop_skip.rs`, `TraceDock.tsx`, team contract moves,
  capability-first spec deletion.
- Followup: probably nothing; merged PR is canonical.

### agent-afb41c21e9140e7e3 — `task/trace-dock-emitters` @ 82e8c56

- Branch on origin; tip is `feat(trace): fill in tool_calls / events /
  spans / supervisor_notes emitters (F43)`. **No open PR.**
- Dirty: 65-line `frontend/web/package-lock.json` insertion only.
- Followup: F43 trace-dock emitters work is sitting on the branch
  unmerged. Confirm whether it should be PR'd or dropped — branch tip
  `82e8c56` is the candidate. The package-lock dirt was incidental.

## Removed — `.worktrees/` (top-level)

### cli-strategy-clone-model-override — `task/cli-strategy-clone-model-override` @ 065b5b4

- Feature shipped via PR #543 (`xvn strategy clone`) + #544 (clone
  preserves `AgentRef.activates`). Branch tip is a stale conductor
  commit unrelated to the work.
- Dirty: ~50 modified files (mostly fixture migrations across CLI +
  engine tests) **plus 2 untracked tests not on any branch**:
  - `crates/xvision-cli/tests/strategy_clone_cli.rs` (373 lines)
  - `crates/xvision-engine/tests/strategy_clone_model.rs` (371 lines)
- **Salvaged** both untracked tests to
  `team/notes/2026-05-22-cleanup/salvage/...` preserving their paths.
- Followup: the two salvaged tests are real coverage for the
  `xvn strategy clone --provider --model` refusal paths and the
  CLI surface. They reference engine APIs that exist post-#543/#544
  (`api_strategy::clone_strategy_full`, `CloneStrategyReq`,
  `provider_unknown` / `model_disabled` reasons). Worth landing as a
  follow-up PR — they may need small adjustments to match current trait
  shapes after the agent-graph wave. Owner TBD.

### cli-eval-model-override — `task/cli-eval-model-override` @ 30ca6d9

- Feature shipped via PR #538 (`xvn eval --provider/--model` override).
- Worktree clean. Branch tip on origin; check for unmerged follow-ups
  if any operator surface tweaks live past the merged PR.
- Followup: none expected.

### agent-firing-filter — `task/agent-firing-filter` @ 0f0cae1

- Spec + contracts shipped via PR #547; Phase 1 form/docs via PR #548.
- Branch tip `0f0cae1` is `fix(agent-firing-filter): align docs and
  contract wording` — one commit past the merged work, **not on
  origin/main**.
- Followup: check whether the docs/contract wording alignment commit
  needs to land. If yes, cherry-pick `0f0cae1` onto main; if not, drop.

### agent-firing-filter-form-and-docs — `task/agent-firing-filter-form-and-docs` @ 0f995a7

- Same wave as above; tip `0f995a7` is `fix(agent-firing-filter): wire
  firing docs into app docs` — one commit past the merged PR.
- Followup: same — verify whether the in-app docs wire-up landed via
  another path or needs a small PR.

### cli-test-fixture-completion-tail — `main` (no dirty state)

- Worktree was effectively empty (its only dirt was the
  `docs/design/trading-charts/` deletions, which have since been
  resolved in main with commit `3aa4e27`).
- Contract: `team/contracts/cli-test-fixture-completion-tail.md` exists
  on main; check whether the test-fixture completion-tail track is
  still open work or completed.
- Followup: confirm track status; if still open, the worktree can be
  recreated cleanly off main.

## Salvage

- `team/notes/2026-05-22-cleanup/salvage/crates/xvision-cli/tests/strategy_clone_cli.rs`
- `team/notes/2026-05-22-cleanup/salvage/crates/xvision-engine/tests/strategy_clone_model.rs`

## Not touched

- `/private/tmp/xv-test-fix` (branch `fix/agent-recovery-test-failures`,
  clean) and `/private/tmp/xvision-phase-review` (detached HEAD, clean)
  — both at origin/main tip `4e467f7` (pre-rebase). Left alone; may
  belong to an active phase-review session.
- All locked `.claude/worktrees/agent-*` were forced — locks were
  bypassed with `--force` since the work was either obsolete (PRs
  merged) or abandoned with no commit.
