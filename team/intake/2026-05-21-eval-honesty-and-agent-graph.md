# Intake — 2026-05-21 — eval contract honesty + agent-graph composition

Source: xvnej-app QA rerun (last night's image, `xvision:deploy-latest` started
2026-05-21 01:16). Symptoms reported by the QA finder were poor strategy
metrics across `Opening Range Breakout`, `Pullback Reclaim Alpha`, and
`Volatility Mean Reversion`. Root-cause inspection of `/data/xvn.db`,
`/data/agent_runs/blobs/*`, and 24h of dashboard logs found the metrics are
unreliable upstream of any strategy choice.

## Findings (concrete, from the running container)

1. **The LLM was never actually called.** Every one of 217 decisions in run
   `01KS4D0MZBD5VGEQ9ACJDRBFBG` returned the literal string
   `"stub Gemini Flash 3.1 response"` with `action="long_open"` and
   `conviction=0.42`. The `gemini-local` provider (a Serveo tunnel) is
   returning a fixture, not real model output. `provider_diagnostics.tokens_used
   = {input: 2170, output: 2170}` across all 217 decisions confirms the calls
   are not reaching a real Gemini model. The eval pipeline completed with
   `status=completed` and shipped a `sharpe=-7.84` derived entirely from a
   mock response.
2. **Two strategies produced byte-identical metrics** because both attached
   agents hit the same stub. The `Volatility Mean Reversion` earlier +7.07
   sharpe was the same fixture on a different scenario window where BTC
   drifted up.
3. **The pyramid guardrail rewrote 432 of 432 trader actions** in the last
   24h (`original=long_open`, `applied=hold`, `reason="pyramid blocked"`).
   The single per-decision WARN is logged once per blocked action;
   collapsing to one per-run finding would have surfaced this immediately.
4. **No provider/model attestation on the eval export.** The export's
   `provider_diagnostics` block records token counts but does not record the
   actual `(provider, model)` the calls used, so a strategy manifest
   referencing `anthropic.claude-sonnet-4.6` runs through
   `gemini-local / gemini-3.1-flash` with no surfaced warning.
5. **Briefing inefficiency.** The LLM blob at
   `/data/agent_runs/blobs/0052f5a5…` is 40,624 bytes: 200 raw OHLCV bars,
   `"tools": []`, no indicators precomputed even though
   `allowed_tools=["ohlcv","indicator_panel"]` is set on the strategy. A real
   provider would burn ~10k input tokens per call on redundant raw data and
   would still not have ATR / ORB / regime context.
6. **`xvn doctor` and `xvn provider list` via `docker exec` read the
   read-only seed config** (`/config/default.toml`) instead of the writable
   `/data/config/default.toml` the dashboard reads. The container compose
   env hard-sets `XVN_CONFIG_PATH=/config/default.toml`, defeating the
   entrypoint's override. Operator-facing diagnostics lie about which
   providers are configured.

## Design principle for the work below

The platform hosts user/agent-authored strategies. It does not author them.
Surviving items are infra/contract fixes (the system tells the truth about
what happened) and composition primitives (the user wires agents together
how they want). The following are **explicitly out of scope**:

- `Strategy.required_models` / `trader_slot.model_requirement` — the user
  picks the model on their agent. Today these fields exist and are not
  enforced; they should be demoted to a user-declared
  `attested_with: Vec<(provider, model)>` field that is informational only
  and never gates execution.
- Pushing rules from agents into `strategy.trader_slot.prompt` — agents own
  the rules. The slot prompt is at most an override the strategy author can
  set; the source of truth is the agent's `system_prompt`.
- Shipping canonical "Mean Reversion Agent" / "Breakout Agent" templates as
  system defaults — the user composes.
- `mechanical_params.regime_filter` baked into the runner — replaced by the
  agent-graph composition below (a user-authored edge, not a system rule).

## Raw items → tracks

| Raw item | Track | Lane |
|---|---|---|
| Eval run with all-identical justifications must be flagged red, not completed green | `eval-honesty-smell-tests` | foundation |
| Stamp `(provider, model)` actually used per LLM call onto the eval export and run detail UI | `eval-provider-attestation` | foundation |
| Pre-flight `xvn provider check` before eval launch; refuse to start if the agents' bound providers are unreachable | `eval-provider-preflight` | foundation |
| Collapse per-decision `pyramid blocked` WARN to a per-run finding + counter | `eval-guardrail-log-collapse` | leaf |
| Skip the LLM call when the current `portfolio_state` allows zero legal actions; opt-out per-slot | `trader-noop-skip` | leaf |
| Demote `required_models` / `model_requirement` to informational `attested_with`; no eval-time substitution gate | `strategy-model-attestation-only` | leaf |
| Resolve the role of `strategy.trader_slot.prompt`: either remove or make it an explicit author-side override of the bound agent's `system_prompt` | `strategy-slot-prompt-resolution` | leaf |
| Agent-graph composition: formalize `kind` (`trader`/`filter`/`critic`/`intern`) on `AgentRef`, with per-kind I/O contracts; per-Filter `granularity` field on `AgentRef` (decoupled from other agents' cadence); Filter emits user-named signals into downstream agents' briefings; strategy can declare graph edges that short-circuit downstream calls based on Filter output. **Depends on `executor-refactor`** (see `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md`) — `PaperExecutor` is being deleted; Filter pipeline / executor wiring targets the post-refactor unified `Executor`, never `PaperExecutor`. | `agent-graph-composition` | foundation |
| Wire `indicator_panel` tool through to trader slot (currently `"tools": []` in the LLM blob); the agent requests indicators, the system does not stuff them | `indicator-tool-wiring` | leaf |
| Surface and respect `AgentSlot.bar_history_limit` in the agent editor; default-respect on the runner | `bar-history-limit-surface` | leaf |
| Expose `conviction` to the risk layer so user-authored risk configs can scale sizing if they choose; never enforced | `risk-sees-conviction` | leaf |
| Prompt-cache stable prefix (system prompt + tool schemas + scenario header) on supported providers; per-slot `max_tokens` cap default; optional delta-briefing mode | `eval-token-efficiency` | leaf |
| Archive `Test Strategy`, `Template-ish Agent`, `Template Mean Reversion Agent` from the seeded examples | `seed-scaffolding-cleanup` | leaf |
| Drop `XVN_CONFIG_PATH` hard-set from `docker-compose.yml` so `docker exec` inherits the entrypoint's writable override | `container-config-path-papercut` | leaf |

## Out of this intake

- Anything implying a canonical playbook for any strategy type.
- Anything that gates the user's choice of model.
- Anything that stuffs interpretation into the briefing — the agent asks via
  tools.

## Next deploy snapshot

`main` at audit time: `dd195cd5` (Merge pull request #362 from
latentwill/task/model-call-cost-usd-population).
