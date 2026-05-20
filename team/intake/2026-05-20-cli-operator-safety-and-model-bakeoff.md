# Intake — 2026-05-20 — CLI operator safety + bounded model bakeoff workflow

Source: Hermes operator feedback after using the new `xvn` CLI/dashboard surfaces to rerun two profitable short BTC scenarios with `google/gemini-3.5-flash` and accidentally over-launching additional evals. The session exposed a gap between good low-level CLI primitives and a safe, bounded operator workflow for exactly-N model/strategy tests.

One-sentence framing:

> Make `xvn` support scoped, cancellable, token-bounded eval/model-bakeoff workflows without requiring Python glue.

This intake is an addendum to `team/intake/2026-05-19-cli-agent-research-workbench.md`. Some items overlap existing tracks there, but the emphasis here is narrower: prevent token burns, make model reruns first-class, and keep CLI/dashboard/provider behavior consistent.

## Session evidence

What worked:

- `xvn strategy new --prompt ... --provider ... --model ... --json` was useful. Atomic strategy + agent creation is a real improvement over the old multi-step wiring path.
- `xvn eval list --json` and `xvn eval show --json` were useful for state inspection.
- `xvn experiment run --help` shows the right direction: scenario selection, wait, compare, markdown, and decision-budget metadata.
- Dashboard API provider pick list correctly showed `google/gemini-3.5-flash` enabled for `openrouter`.

What still forced Python/HTTP glue:

- Creating multiple prompt files and strategies.
- Launching evals through the dashboard API when CLI eval launch disagreed about provider config.
- Polling terminal states.
- Cancelling token-heavy/running runs.
- Counting actions from decisions.
- Producing a compact comparison summary.

## Findings → intake items

### P0 — Safety / token-burn controls

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 1 | P0 | `cli-eval-cancel` | Add `xvn eval cancel <run_id>`, `xvn eval cancel --running`, `--strategy <id>`, and `--older-than <duration>` so operators can stop live token burns without raw HTTP calls. |
| 2 | P0 | `eval-run-hard-limits` | Add enforceable eval launch limits: `--max-decisions`, `--max-input-tokens`, `--max-output-tokens`, `--max-wall-clock`, and `--cancel-on-token-limit`; propagate to dashboard/API launches too. |
| 3 | P0 | `experiment-run-scope-guardrails` | Add `--max-runs`, sequential execution by default for LLM-backed workflows, and a dry-run/confirmation summary showing total runs, model, scenarios, decision caps, and token ceilings. For automation, require explicit `--yes`. |

Rationale: in the session, Gemini 3.5 Flash produced very high output-token counts, and extra runs had to be cancelled manually via `POST /api/eval/runs/:id/cancel`.

### P1 — Model reruns and bakeoffs as first-class workflows

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 4 | P1 | `cli-strategy-clone-model-override` | Add `xvn strategy clone <strategy_id> --provider <provider> --model <model> --name <name>` to rerun an existing strategy under a new model without manually reconstructing prompts. |
| 5 | P1 | `cli-eval-model-override` | Add `xvn eval run --strategy <id> --scenario <id> --provider <provider> --model <model>` as a temporary per-run model override, producing an explicit derived strategy/agent reference or immutable override receipt. |
| 6 | P1 | `cli-model-bakeoff` | Add `xvn model bakeoff --strategies <ids> --models <ids> --scenario <id> --max-runs <n> --sequential --wait --compare --markdown` for controlled model comparison without Python orchestration. |
| 7 | P1 | `cli-two-run-rerun-workflow` | Support the exact common operator flow: "rerun these two strategy ids on this scenario with this model, wait, compare, stop". This can be a thin mode of `model bakeoff` or `experiment run`. |

Desired shape:

```bash
xvn model bakeoff \
  --strategies 01KS08ECPT4V1JXWSSF1W3SBA7,01KS08KW0Z8S8VXFMWY13X70Z4 \
  --models google/gemini-3.5-flash \
  --provider openrouter \
  --scenario sc_01KS0880VW6854ZQVBXQBVMDHG \
  --max-runs 2 \
  --max-decisions 100 \
  --max-output-tokens 60000 \
  --sequential \
  --wait \
  --compare \
  --markdown
```

### P1 — Provider/config consistency

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 8 | P1 | `provider-resolution-parity` | Ensure `xvn eval run`, dashboard `POST /api/eval/runs`, `xvn provider list`, and `xvn doctor` use/report the same effective provider registry and enabled-model state. |
| 9 | P1 | `provider-doctor-effective` | Add `xvn doctor --providers --json` or `xvn provider list --effective --json` showing config providers, dashboard/runtime providers, enabled models, key present/missing, and whether eval can use each provider/model. |

Observed mismatch: dashboard API could launch OpenRouter/Gemini 3.5 Flash evals, while `xvn eval run` failed with `provider 'openrouter' is not configured`.

### P1 — Machine-readable CLI contract

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 10 | P1 | `cli-json-stdout-contract` | When `--json` is set, stdout must contain JSON only; progress/human text goes to stderr. Apply to `xvn eval run`, `watch`, `batch`, and experiment verbs. |
| 11 | P1 | `cli-report-actions-and-tokens` | Add action distribution, repeated opens, direct flips, decisions, trades, input/output tokens, wall time, and cost estimate to `xvn eval results`, `xvn eval compare`, and `xvn experiment run --compare`. |

Observed issue: `xvn eval run --json` printed human text (`Starting eval run — ...`) before JSON/error output, and action distribution had to be computed with Python.

### P1 — Remote CLI parity / allowlist

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 12 | P1 | `remote-cli-safe-eval-allowlist` | Expand dashboard remote CLI allowlist beyond `bars fetch` to safe read/operation verbs: `eval list`, `eval show`, `eval results`, `eval watch`, `eval compare`, `eval cancel`, `strategy show`, `scenario show`, and bounded `experiment run`/`model bakeoff` variants. |

Observed issue: `scripts/xvn-remote.py exec -- --help` failed with `argv does not match any allowlisted template`, so live eval operation fell back to SSH + `docker exec`.

### P2 — Semantics / wording cleanup

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 13 | P2 | `decision-budget-enforcement-rename` | Current `experiment run --decision-budget` is metadata only. Either add real enforcement or rename to `--decision-budget-label` / `--intended-decisions` and introduce true `--max-decisions`. |
| 14 | P2 | `prompt-action-enum-validation-normalization` | Prompt validator should strip punctuation or validate structured fields, not fail on prose like `short_open.` when the enum token is otherwise valid. Prefer warning when ambiguity is low. |
| 15 | P2 | `cli-docs-agent-recipes` | Add docs/skill recipe for "run exactly two model reruns", "run a small model bakeoff", "cancel all running evals", and "compare action/tokens". Keep Hermes skill references in sync. |

## Acceptance sketch for the first useful slice

A minimal successful slice would let an operator run this without Python:

```bash
xvn model bakeoff \
  --name gemini35-profitable-rerun \
  --strategies <trend-rider-id>,<swing-oracle-id> \
  --provider openrouter \
  --models google/gemini-3.5-flash \
  --scenario <btc-jan13-17-1h-scenario-id> \
  --max-runs 2 \
  --max-decisions 100 \
  --max-output-tokens 60000 \
  --sequential \
  --wait \
  --compare \
  --markdown \
  --yes
```

Expected behavior:

1. Prints a dry-run plan before launch unless `--yes` is supplied.
2. Launches exactly two eval runs, sequentially.
3. Cancels any run that breaches token/wall-clock/decision limits and marks the result clearly as `cancelled_limit` rather than strategy evidence.
4. Produces one comparison report with: return, Sharpe, max drawdown, decisions, trades, action mix, input/output tokens, wall time, and run ids.
5. Writes a persisted experiment/bakeoff result object that can be inspected later.

## Notes for conductor decomposition

Recommended first wave:

1. `cli-eval-cancel`
2. `eval-run-hard-limits`
3. `provider-resolution-parity`
4. `cli-json-stdout-contract`
5. `cli-report-actions-and-tokens`

Then build `cli-model-bakeoff` on top of those primitives.

No migration should be required for the first wave unless persisted token-limit metadata/result status needs new fields. If new cancellation reason/status is added, coordinate with eval trace/status consumers and dashboard filters.
