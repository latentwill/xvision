# status: v2d-agent-memory

> Worker-owned status. Conductor reads but does not edit.

## Current state — 2026-05-21

**Claimed** by V2D wave session 2026-05-21. Branch `task/v2d-agent-memory`
created off `origin/main` (`a37af89`) in worktree
`.worktrees/v2d-agent-memory/`. Merged with origin/main mid-session to
absorb the V2E `eval-trace-surface-foundation` migration-026 collision
(V2D renumbered to 027).

## Phase plan

Per `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`:

- [x] **Phase 0** — plan + intake committed.
- [x] **Phase 1** — `xvision-memory` crate (standalone). 7 unit tests
      pass.
- [ ] **Phase 1.5** — cortex tier split (Resources / Skills); **added
      2026-05-21** after `/grill-me` design pass — see
      `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md`.
- [x] **Phase 2** — engine migration 028 + `AgentSlot.memory_mode` +
      AgentStore roundtrip.
- [x] **Phase 3** — `MemoryRecorder` + `execute_slot` recall/record
      wiring + OpenAI embedder + pipeline + executor end-to-end
      threading.
- [x] **Phase 4** — `MemoryMode` ts-export + AgentForm Memory selector
      + 2 vitest tests.
- [x] **Phase 5** — `MemoryPanel.tsx` in eval-review + 4 vitest tests +
      wired into `ReviewContent.tsx`.
- [ ] **Phase 6** — operator-facing docs
      (`docs/v2d-memory-overview.md`, MANUAL.md link, V2A overview
      subsection). Source: the cortex-tiers design note.
- [ ] **Verification** — `cargo test --workspace`, `pnpm typecheck`,
      `pnpm test --run`, `bash scripts/board-lint.sh`.
- [ ] **PR** — flip from draft to ready-for-review when Phase 1.5 + 6
      land and CI is green.

## Checkpoints

- 2026-05-21 — contract claimed; plan + intake landed; subagent
  dispatch begins for Phase 1.
- 2026-05-21 — Phases 1 → 5 complete and committed. Engine tests at
  707/707 (excluding one pre-existing parallel flake from PR #388,
  `eval_early_stop::second_skip_window_only_triggers_after_counter_resets`).
  Frontend vitest 88 files / 676 tests green. Typecheck clean.
- 2026-05-21 — `/grill-me` pass on the memory design revealed a
  look-ahead leakage problem in the single-tier shape: backtest
  replays could recall prior decisions made on the same cycle. Plan
  extended with **Phase 1.5** (Resources/Skills cortex tier split)
  and **Phase 6** (operator docs). V3 autoresearcher item gains
  sub-entry **11a** on `team/board-v2.md` noting that the
  autoresearcher *is* the distillation pass (needs Skills-tier write
  access via `upsert_skill` / `demote_skill`). Design note at
  `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md`.
  Next: implement Phase 1.5 + 6, then re-verify and flip the PR.
