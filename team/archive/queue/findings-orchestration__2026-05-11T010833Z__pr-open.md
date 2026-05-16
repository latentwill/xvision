---
from: findings-orchestration
to: all
topic: pr-open
created_at: 2026-05-11T01:08:33Z
ack_required: false
---

# `findings-orchestration` PR open: [#62](https://github.com/latentwill/xvision/pull/62)

Track A of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`
landed as a single PR. Closes the BLOCKER for v1 acceptance criteria #2
(backtest persists metrics + findings) and #4 (Compare renders findings).

## What changed

- New `eval::postprocess::extract_and_record(ctx, run_id, dispatch, model)`
- `api::eval::run_inner` calls it after the run is re-read post-finalize
- Best-effort: extractor failure logs + audits but never fails the run
- Findings persist via `RunStore::record_finding` AND index for ⌘K via
  `api_search::upsert_finding`

## Design choice (worth noting for other tracks)

I deliberately did NOT thread `&ApiContext` into the `Executor` trait. The
spec offered both options; orchestration-layer composition won because it
keeps `Executor::run` focused on "drive a strategy through a scenario" and
avoids ripple. If a future Track needs to add a per-decision audit hook
inside an executor, it'll have to revisit this — but for the post-finalize
slice this composition is clean.

## Tests

- 6 new unit tests in `eval::postprocess::tests`
- Engine: 60/60 unit tests pass, 0 failures
- `cargo build --workspace` clean

## Zero overlap with other v1-gap tracks

- B/C/D own `frontend/web/src/routes/eval-runs.tsx` — no touch
- E owns `routes/authoring.tsx` — no touch
- F adds engine + dashboard surface under `api::settings::danger` — no touch
- G adds tests in `api/{audit,health}.rs` — no touch
- H is a Tailwind tweak in `routes/strategies.tsx` — no touch

Everyone is unblocked.

## Live smoke

Defer to operator (needs `ANTHROPIC_API_KEY`):

```sh
xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval show <run_id>   # findings section should now be non-empty
```
