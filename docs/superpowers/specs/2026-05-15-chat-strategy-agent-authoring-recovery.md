# Chat Strategy Agent Authoring Recovery

Date: 2026-05-15

## Live Failure

A chat request to create a BTC golden cross 30/90 strategy on 4h candles with
aggressive risk exposed three linked failures:

- The assistant claimed it was setting manifest fields, mechanical params, risk,
  and an agent, but malformed tool input caused `update_manifest` to fail with
  `missing field id`.
- Strategy creation did not create or attach a strategy `AgentRef`, leaving eval
  blocked even after the draft existed.
- Eval preflight surfaced `Pick a provider/model for the strategy agent before
  running eval` because the strategy list had no runtime provider/model pair to
  report.

## Spec

- Chat strategy creation must produce an eval-ready trader agent when the chat
  rail has a selected provider/model.
- The strategy setup loop must expose an explicit tool to create and attach a
  strategy agent, plus a tool to attach an existing agent.
- Strategy tool input should tolerate common model mistakes:
  - `strategy_id` may alias `id` for strategy mutations.
  - `{ "<tool_name>": { ... } }` and `{ "input": { ... } }` wrappers are
    unwrapped before request deserialization.
- The setup prompt must instruct the model not to claim success until the
  relevant `tool_result` succeeds.
- Chat tool log UI should show create/attach-agent progress and created agent
  ids.

## Acceptance

- Creating a strategy from the chat rail with a configured provider/model returns
  the strategy id and an attached trader agent id.
- A nested `update_manifest` call with `strategy_id` persists asset universe and
  cadence instead of failing with `missing field id`.
- `validate_draft` and eval preflight can discover the attached agent's
  provider/model pair from the strategy summary.
- Frontend typecheck and focused ChatRail tests pass.
