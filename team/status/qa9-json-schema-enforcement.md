---
track: qa9-json-schema-enforcement
worktree: .worktrees/qa9-json-schema-enforcement
branch: qa9-json-schema-enforcement
claimed_at: 2026-05-14T00:00:00Z
status: implemented-pending-cargo-ci
---

# qa9-json-schema-enforcement

Claimed to address invalid eval agent JSON responses and strict create-payload
validation.

Primary reported failures:

- Run `01KRK9Y45K1MKS9FTH4TY4SK47`, decision 0: trader output is invalid JSON,
  missing field `action` at line 1 column 184.
- Run `01KRKATKTK331A08TQ2MBN6FYC`, decision 0: trader output is invalid JSON,
  missing field `action` at line 1 column 18.

Working notes:

- `CLAUDE.md` read first. This deploy host must not run `cargo build`,
  `cargo check`, or `cargo test`.
- Initial focus is the runtime agent response contract that blocks evals.

Implementation:

- Added `ResponseSchema` support to `LlmRequest`.
- OpenAI-compatible dispatch now sends provider-native
  `response_format.type = json_schema` when a response schema is present.
- Anthropic dispatch appends the JSON Schema contract to the system prompt,
  since Messages does not expose the same `response_format` knob.
- Legacy strategy `trader_slot` and agent-pipeline final slots now request the
  strict trader-output schema requiring `action`, `conviction`, and
  `justification`.
- Eval backtest and paper executors now reject missing trader fields, unknown
  trader fields, invalid actions, out-of-range conviction, and empty
  justification before emitting decisions or orders.
- Strategy, agent, scenario, eval, slot, and request DTOs now use
  `serde(deny_unknown_fields)` for stricter creation/update payload handling.
- Starter agent templates now spell out the exact eval trader JSON shape and
  explicitly say not to omit `action`.

Verification:

- `git diff --check` passed.
- `rg` audit confirmed every `LlmRequest` initializer sets `response_schema`.
- `rustfmt` could not be run because it is not installed on this host.
- Cargo verification is pending by policy: `CLAUDE.md` forbids running
  `cargo build`, `cargo check`, or `cargo test` on deploy hosts.
