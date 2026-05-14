# qa9-alpaca-eval-full-run-burndown

Date: 2026-05-14
Branch: qa9-alpaca-eval-full-run-burndown
Claim: team/queue/qa9-alpaca-eval-full-run-burndown__2026-05-14T134735Z__claim.md

## Summary

- Reproduced both reported live failures from the `xvn-app` container state
  and through the dashboard API.
- Added a shared executor trader-output parser for paper and backtest modes.
- Added a missing-`action` diagnostic regression covering both reported run IDs.
- Added eval preflight validation so attached-agent strategies must include a
  `trader` role before launch.
- Stopped treating the final non-trader pipeline output as a trader decision.

## Live Reproduction

`GET /api/health` on `xvn.tail2bb69.ts.net` returned healthy DB and strategy
probes.

`xvn-app` stores the reported runs in `/home/xvision/.xvn/xvn.db`, not
`/data/store.db`.

- `01KRK9Y45K1MKS9FTH4TY4SK47`: `paper`, strategy
  `01KRK9Q7YH6DMK692N03PX7C7K`, scenario `crypto-rangebound-q2-2025`,
  failed at `2026-05-14T13:13:39.251603022+00:00` with
  `run 01KRK9Y45K1MKS9FTH4TY4SK47 decision 0: trader output is invalid JSON: missing field \`action\` at line 1 column 184`.
- `01KRKATKTK331A08TQ2MBN6FYC`: `backtest`, strategy
  `01KRK9Q7YH6DMK692N03PX7C7K`, scenario `crypto-bull-q1-2025`,
  failed at `2026-05-14T13:29:12.787092078+00:00` with
  `run 01KRKATKTK331A08TQ2MBN6FYC decision 0: trader output is invalid JSON: missing field \`action\` at line 1 column 18`.
- A later run, `01KRKAW4M67NS0CFZ4XJZQ5AZ1`, also failed at decision 0 with
  `expected value at line 1 column 1`.

Container logs show the same two reported executor failures.

## Root Cause

The failed strategy has a legacy `trader_slot`, but it also has one attached
agent:

- attached agent role: `seeker`
- attached agent model: `openrouter` / `deepseek/deepseek-v4-flash`
- seeker output schema uses `signal`, not trader `action`

Current eval runtime prefers attached agents whenever any are present. The
pipeline then falls back to the last attached agent output when no role is
named `trader`, so the eval executor parses seeker JSON as trader JSON.
That burns an LLM call and fails with missing `action`.

Live `POST /api/strategy/01KRK9Q7YH6DMK692N03PX7C7K/validate` currently
returns `ok:true`, so deployed validation does not catch this before eval.

## Blockers

- `xvn eval show <run> --json` cannot read either reported run in the live
  container because the CLI exits with
  `open ApiContext: error returned from database: (code: 1) no such column: strategy_bundle_hash`.
- `xvn doctor --json` points CLI state at `/home/xvision/.xvn/xvn.db`, while
  the entrypoint log says it migrates `/data/store.db`. `/data/store.db` only
  has the older core tables, so the live deployment still has the hidden/fallback
  DB split called out by `qa8-cli-runtime-blockers`.
- A full Alpaca eval completion run should wait until the runtime DB/CLI fix,
  `qa9-json-schema-enforcement`, and the strategy-agent guardrail are integrated
  into the deployed image.

## Fix In Branch

- Removed the pipeline fallback that treated the last non-trader attached
  agent as the trader output.
- Added eval preflight validation requiring a real trader output source:
  - legacy strategies with no attached agents must have `trader_slot`
  - attached-agent strategies must include an attached agent with role `trader`
- Added regression coverage for the seeker-only attached-agent case and for
  pipeline output semantics.

## Verification

- PASS: `git diff --check`
- PASS: `rg` audit shows one shared executor `TraderOutput` parser used by both
  paper and backtest executors.
- PASS: local diff includes a regression that non-trader attached-agent
  pipelines do not produce trader decisions.
- PASS: live API inspection reproduced both reported run rows and errors.
- PASS: live `POST /api/strategy/01KRK9Q7YH6DMK692N03PX7C7K/validate` confirms
  the deployed strategy validator currently misses this eval-blocking state.
- PASS: live read-only DB inspection of copied `/home/xvision/.xvn/xvn.db`
  reproduced both reported run rows and errors.
- PASS: `docker logs --tail 120 xvn-app` shows both reported executor failures.
- BLOCKED: `xvn eval show ... --json` exits with the `strategy_bundle_hash`
  schema mismatch above.
- NOT RUN: Rust tests / `cargo test` because this session is on the deploy host
  and `CLAUDE.md` forbids Cargo on deploy hosts.
- NOT RUN: `rustfmt` because `rustfmt` is not installed on this host.
