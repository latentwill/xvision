# Risk decisions & known baseline state

Wave: chat-rail / DSPy / strategy-agents. Branch `feat/chat-rail-dspy-strategy-agents`
based on `feat/cline-runtime-unification` @ `5e7d938`.

This file records intentional deferrals, mock-vs-real evidence honesty, and
pre-existing failures inherited from the base branch so they are not mistaken
for regressions introduced by this wave.

## Base-branch decision

The Cline runtime unification (`feat/cline-runtime-unification`) is **not merged
and not pushed** — verified 2026-05-24: no `cline` branch on `origin`, no open PR.
This wave's Phase 1 unified event taxonomy and Phase 3 ClineSDK/rig-core adapter
build directly on that runtime, so the wave is based on the Cline branch. If the
Cline review changes those commits, this branch rebases onto the updated base.

Risk: building on 43+ in-review, unpushed commits. Mitigation: disjoint file
ownership where possible; the wave's net-new crates (`xvision-dspy`) and new
event projection layer do not rewrite Cline internals.

## Pre-existing baseline failures (NOT caused by this wave)

1. **`scripts/check_agent_docs.sh` exits 1.** The checker greps repo-root
   `README.md` for a reference to `.claude/skills/xvision/SKILL.md` — a stale
   single-skill path from before the skill was split into `xvision-cli`,
   `xvision-dev`, and `xvision-cli-qa`. Flagged as a known baseline failure in
   the Cline handoff note (`docs(cline): build status / forklift handoff`).
   **Plan:** fix the checker to validate the real skill files when this wave
   touches skills (Phases 2.7 / 3.8 / 4.6), so it also enforces the new rail and
   optimizer skill sections rather than masking the regression now.

2. **`scripts/board-lint.sh` exits 1** on
   `team/status/cli-strategy-clone-model-override.md` — phase `ready-for-review`
   is not in the board's allowed phase vocabulary. This status file belongs to a
   different active track (`task/cli-strategy-clone-model-override`), not this
   wave. Not edited here. Surfaced for the owning track.

## Fixed in Phase 0

- **`scripts/guard-no-acpx.sh` false positive** (was exit 1): the allowlist did
  not cover (a) the chat-rail wave **spec** doc's References-section mention of
  `2026-05-24-cline-stage0-acpx-purge.md`, nor (b) `scripts/board-lint.sh`'s own
  invocation of the guard by name. Both are legitimate references to the purge,
  the exact category the allowlist intends to permit (the merge of origin/main's
  plan doc into the Cline branch outran the allowlist). Added both paths to
  `ALLOW`. Guard now passes; this unblocks `board-lint`'s acpx gate.

## Evidence honesty (running)

- Cline live record→sidecar wiring is incomplete on the base (handoff follow-up
  #1): replay/record are tested via seeded stores, not a real end-to-end live
  run. Any Phase 1 SSE evidence captured against a seeded/mock store will be
  labeled as such, per the plan's no-mock-only-proof rule.
