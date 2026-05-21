---
track: cli-operator-safety-p0
lane: integration
wave: cli-operator-safety-2026-05-20
worktree: .worktrees/cli-operator-safety-p0
branch: task/cli-operator-safety-p0
base: origin/main
status: ready
depends_on: []
blocks:
  - cli-model-bakeoff                                              # P1 model-bakeoff cluster depends on hard limits + scope guardrails landing first
stacking: none
allowed_paths:
  # P0 #1 — cli-eval-cancel
  - crates/xvision-cli/src/commands/eval/cancel.rs                 # NEW
  - crates/xvision-cli/src/commands/eval/mod.rs                    # register the new cancel subcommand
  - crates/xvision-cli/tests/eval_cancel_cli.rs                    # NEW — integration test
  # P0 #2 — eval-run-hard-limits
  - crates/xvision-cli/src/commands/eval/run.rs                    # add --max-decisions, --max-input-tokens, --max-output-tokens, --max-wall-clock, --cancel-on-token-limit flags
  - crates/xvision-engine/src/eval/limits.rs                       # NEW — hard limit enforcement on the engine side
  - crates/xvision-engine/src/eval/executor/backtest.rs            # wire limits check
  - crates/xvision-engine/src/eval/executor/paper.rs               # wire limits check
  - crates/xvision-engine/src/api/eval.rs                          # propagate limit fields through launch payload
  - crates/xvision-engine/tests/eval_hard_limits.rs                # NEW
  # P0 #3 — experiment-run-scope-guardrails
  - crates/xvision-cli/src/commands/experiment_run.rs              # --max-runs, --sequential (default for LLM), dry-run summary, --yes
  - crates/xvision-cli/tests/experiment_run_cli.rs                 # NEW or extended — dry-run + --yes coverage
  # Shared
  - crates/xvision-engine/migrations/025_eval_run_cancel_limits.sql        # NEW (if persistent cancel state or limits-breached enum need columns)
  - crates/xvision-engine/migrations/025_eval_run_cancel_limits.down.sql   # NEW
forbidden_paths:
  - crates/xvision-cli/src/commands/eval/batch.rs                  # bakeoff/batch concerns belong to the P1 model-bakeoff track
  - crates/xvision-cli/src/commands/experiment.rs                  # the experiment list/show verbs (no scope changes)
  - crates/xvision-engine/src/api/eval/runs.rs                     # result-shape changes belong to cli-report-actions-and-tokens (P1)
  - frontend/web/**                                                # SPA-side cancel UI is a separate wave
  - crates/xvision-dashboard/**                                    # dashboard launch path must respect new limits (intake §1), but its UI changes are deferred
  - crates/xvision-mcp/**                                          # MCP parity for new verbs is a follow-on (P1 #13 of workbench intake — already shipped, but the new cancel/limits verbs need their own MCP-parity track later)
interfaces_used:
  - RunMode                                                        # crates/xvision-engine/src/eval/run.rs
  - RunStatus                                                      # for adding a `cancelled_limit` terminal status if intake §4 requires
  - LaunchParams                                                   # the engine struct that carries launch-time config
  - EvalLimits                                                     # NEW struct under `crates/xvision-engine/src/eval/limits.rs`
parallel_safe: false                                               # touches eval.rs which other active tracks read
parallel_conflicts:
  - paper-eval-inspector-parity                                    # also reads/writes `api/eval.rs` — coordinate via team/queue/ if both in-flight
  - strategy-require-at-least-one-agent-fixture-migration          # also touches `api/eval.rs`
verification:
  - cargo test -p xvision-cli eval_cancel_cli
  - cargo test -p xvision-cli experiment_run_cli
  - cargo test -p xvision-engine eval_hard_limits
  - cargo test --workspace
acceptance:
  - **P0 #1 — `xvn eval cancel` is a first-class verb.** Subcommand at `crates/xvision-cli/src/commands/eval/cancel.rs` with shape: `xvn eval cancel <run_id>`, `xvn eval cancel --running` (cancel all currently-running), `xvn eval cancel --strategy <id>` (cancel all for one strategy), `xvn eval cancel --older-than <duration>` (cancel runs older than the duration). Each path returns JSON with `cancelled_ids[]` + per-id `outcome` (`cancelled`, `not_running`, `not_found`). Hits the existing `POST /api/eval/runs/:id/cancel` endpoint — no new endpoint needed.
  - **P0 #2 — eval launch enforces hard limits.** `xvn eval run` gains `--max-decisions`, `--max-input-tokens`, `--max-output-tokens`, `--max-wall-clock <duration>`, `--cancel-on-token-limit` flags. Engine-side enforcement in `crates/xvision-engine/src/eval/limits.rs` (new module). When a limit is breached during execution, the run transitions to a terminal status (`cancelled_limit` if intake §4 requires — otherwise `cancelled` with a `cancel_reason: "limit_breached"`); the result row records which limit triggered. Dashboard `POST /api/eval/runs` accepts the same fields and propagates them. **Operator cannot launch without explicit limits if the strategy's prior runs averaged >X tokens — see Notes for the heuristic.**
  - **P0 #3 — experiment-run scope guardrails.** `xvn experiment run` gains `--max-runs <n>` (hard cap on the number of eval runs the experiment launches), default-sequential execution for LLM-backed strategies (parallel only with explicit `--parallel`), and a **dry-run plan summary** before launch unless `--yes` is supplied. Summary lists: total runs to be launched, models, scenarios, decision caps, token ceilings, expected total token bound, expected total cost. With `--yes`, plan still prints but launch proceeds.
  - **Migration (if needed).** If a new `cancelled_limit` terminal status or a `cancel_reason` column is required, the migration is `025_eval_run_cancel_limits.sql` + `.down.sql` per the conductor migration registry. If the existing schema can express the new status without DDL (e.g. a free-text `cancel_reason` column already exists), skip the migration and document the decision in `Notes:`.
  - **Tests.** Each of the three sub-features gets at least one integration test:
    - `eval_cancel_cli`: launch a run, cancel via each of the four `--running` / `--strategy` / `--older-than` / by-id paths; assert correct outcomes and idempotency.
    - `eval_hard_limits`: launch with `--max-output-tokens 100`, mock a slow generator that breaches; assert run lands `cancelled_limit` with `cancel_reason: "max_output_tokens"`.
    - `experiment_run_cli`: assert dry-run prints the plan and refuses launch without `--yes`; assert `--max-runs 1` caps the launches at 1 even with 5 scenarios in the set.
  - **No --json contract drift.** Per workbench intake §10 (P1 `cli-json-stdout-contract`, shipped via wave A discipline): `--json` stdout is JSON-only; human progress goes to stderr. The new cancel/limits flags respect that contract.
  - **No reliance on Python orchestration.** The operator-visible acceptance from the intake's §"Acceptance sketch for the first useful slice": after this PR lands, the example bakeoff invocation works *for the limits + cancel + scope-guardrail subset*. The actual `xvn model bakeoff` verb is a separate P1 track.

---

# Scope

Track #1+#2+#3 of `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`,
bundled into one P0 safety contract. The three sub-features share the
same code surface (eval launch path, run terminal-status handling,
experiment orchestrator) and the same acceptance posture (stop token
burns; require explicit scope). Bundling avoids three sequential
rebases through `crates/xvision-cli/src/commands/eval/` and
`crates/xvision-engine/src/eval/`.

Source: Hermes operator feedback after the Gemini 3.5 Flash session
where rerunning two profitable short BTC scenarios accidentally
over-launched additional evals and burned tokens. The intake's
one-sentence framing: *"Make `xvn` support scoped, cancellable,
token-bounded eval/model-bakeoff workflows without requiring Python
glue."*

# Out of scope

- P1 #4–#11 (model-bakeoff cluster, provider parity, CLI machine
  contract, results enrichment). Sibling contracts.
- P1 #12 (remote CLI allowlist). Separate concern (dashboard remote
  CLI surface).
- P2 #13–#15 (semantics rename, prompt validator normalization, docs
  recipes). Deferred to a follow-on wave.
- Frontend dashboard cancel/limits UI. The API contract must accept
  the new fields, but the SPA-side surface is a separate track.
- New telemetry/observability. Existing `agent_runs` + `model_calls`
  already record token counts; this PR only reads them to enforce
  limits.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/cli-operator-safety-p0 status
git -C .worktrees/cli-operator-safety-p0 log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/cli-operator-safety-p0 -b task/cli-operator-safety-p0 origin/main
```

# Notes

**Heuristic for "operator cannot launch without explicit limits if the
strategy's prior runs averaged >X tokens"** (acceptance bullet 2):
the intake doesn't pin a threshold. Worker should pick a defensible
default (e.g. 200K total tokens per prior run is the threshold; if any
prior run for the strategy crossed it, require explicit `--max-*` flags
or `--no-limits` to launch). Document the chosen threshold in
`crates/xvision-engine/src/eval/limits.rs`'s module-level doc.

**Migration number 025.** Per `team/MANIFEST.md` migration registry,
migrations 023 (trace foundation) and 024 (run-bars manifest) are
claimed by V2E foundation tracks. Worker confirms 025 is free at
sync-before-work; if a later V2E track has claimed 025, escalate to
the conductor for a renumber.

**Coordination with `paper-eval-inspector-parity`** (qa-2026-05-19,
also ready): both contracts may modify `crates/xvision-engine/src/api/eval.rs`.
Disjoint regions are likely (parity diagnoses mode-dispatch around
`:803-1292`; this track adds launch-payload fields around the request
parser). Coordinate via `team/queue/cli-operator-safety-p0.md` if
both are in-flight simultaneously. Land the smaller diff first.

**Decision-budget semantics** (intake P2 #13): the existing
`experiment run --decision-budget` is metadata-only. P2 #13 proposes
renaming to `--intended-decisions` and introducing true
`--max-decisions`. This contract introduces `--max-decisions` on
`xvn eval run` (the launch verb). The rename for `experiment run` is
deferred to P2 — but **do not** silently shadow the metadata semantics.
The dry-run plan summary in #3 should explicitly distinguish
`intended_decisions` (metadata label) from `max_decisions` (hard cap).
