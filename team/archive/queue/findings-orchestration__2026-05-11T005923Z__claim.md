---
from: findings-orchestration
to: all
topic: claim
created_at: 2026-05-11T00:59:23Z
ack_required: false
---

# `findings-orchestration` track claimed (v1 gaps spec — Track A)

Implements Track A of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`:
findings extraction is wired into the executor finalize path so v1 success
criterion #2 ("backtest persists metrics + findings") actually persists
findings. Today every backtest and paper run finishes with empty
`eval_findings` because nothing calls `extract_findings` after
`store.finalize`.

Branch `feature/findings-orchestration` based on `origin/main` @ `0fff672`
(spec commit). Working in the main worktree, not a `.worktrees/` clone —
no other open PR touches `eval/executor/{backtest,paper}.rs`.

## Scope

- `crates/xvision-engine/src/eval/postprocess.rs` (NEW) — single
  `extract_and_record(ctx, run_id)` entry point both executors call.
  Best-effort: extractor failures log at `warn!` and return Ok(0).
- `crates/xvision-engine/src/eval/mod.rs` — register module
- `crates/xvision-engine/src/eval/executor/{backtest,paper}.rs` — call
  postprocess after `store.finalize` returns Ok
- `crates/xvision-engine/tests/eval_findings.rs` — extend with
  executor-driven tests (mock dispatch returning hardcoded findings JSON)
- One audit row per call as `("eval", "extract_findings", Some(run_id), …)`
  so the `xvn eod` report sees it

## Non-conflicts

- No frontend churn (Tracks B/C/D own `eval-runs.tsx`)
- No engine API surface change beyond the new module's `pub` fn — won't
  collide with any settings / strategy / search work
- No new migration

## Deferred to follow-up

- Prompt-tuning the extractor (the v1 prompt is already shipped at
  `eval/findings/prompts/extractor-v1.md` — leave as-is)
- `Finding` schema changes
- Body-text indexing in search (Plan #12 v1.1 follow-up)

## v1 QA value

Closes the BLOCKER for criteria #2 and #4. Without this, the Compare view
and Run Detail page render an empty findings panel for every run, which
is the most visible v1 demo gap.
