# Intake — 2026-05-20 — canonical strategy template needs a default trader agent

Tiny intake spun off PR #369's test triage. The
`validate_draft_succeeds_for_fresh_template` test in
`crates/xvision-mcp/src/tools.rs` now fails on `main`: a strategy
created from the canonical template ships with no agents, but the
strategies refactor (`2026-05-12-strategies-refactor-agent-composition.md`)
requires at least one agent with a `trader` role for
`validate_strategy` to pass. The chat-rail / wizard surfaces this as
*"Validation failed (1 error)"* on the very first `validate_draft`
call after a fresh template instantiation.

## Source

- PR #369 (leftovers-bundle) verification step — the test fails on
  this branch and is verified to reproduce on `origin/main` with the
  branch stashed.
- `crates/xvision-engine/src/strategies/templates.rs` — canonical
  templates ship `agents: vec![]` plus legacy slot fields that the
  refactor no longer treats as the source of truth.
- `crates/xvision-engine/src/strategies/validate.rs` — emits the
  "strategy must have at least one agent" diagnostic.

## Why it lives here and not as its own track

The clean fix shape is **default capability presets on every starter
template** — exactly the concept the V2 capability-first agent-model
refactor introduces (`team/board-v2.md` → Follow-ups → Capability-
first agent model). A one-off "stuff a trader agent into the
canonical template" patch would be the same role-shaped escape hatch
the refactor is supposed to retire. So this is filed as a V2 spec
input, not a contract.

## Track (gated on capability-first agent-model spec)

| # | Severity | Finding | Track | Status |
|---|---|---|---|---|
| 1 | P2 | Canonical strategy template ships no trader agent → `validate_draft` immediately fails for a fresh template. | `canonical-template-default-trader-agent` | **gated** on the V2 capability-first agent-model spec; resolves as part of that refactor (default-capability-preset on every template). |

## Behaviour today (verbatim from PR #369 test output)

```
test tools::tests::validate_draft_succeeds_for_fresh_template ... FAILED

  expected ok=true, got ok=false
  validation errors: ["strategy must have at least one agent (trader role required)"]
```

## What "fix" looks like once the spec lands

- Every starter template carries a `default_capabilities: Vec<Capability>`
  field (or equivalent).
- Template instantiation auto-attaches a placeholder agent that
  satisfies the template's required capabilities, with the user's
  defaults for provider/model.
- `validate_draft` immediately after `xvn_create_strategy` returns
  `ok=true` for any built-in template.
- Operators editing the template later can rewire / re-author the
  agent without losing the "valid by construction" guarantee.

## Coordination

- Spec lives under the existing V2 capability-first item in
  `team/board-v2.md` (extended 2026-05-20 to fold this in alongside
  the F-11 recorder-wireup carve-out).
- Until the spec lands, the failing test in PR #369 (and any later
  PR that touches `xvision-mcp`) stays a known-fail. Document in the
  PR description; do NOT silently delete or mark `#[ignore]`.

## Out of scope

- Patching the canonical template to ship a trader agent inline.
  That's the role-shaped escape hatch the refactor is supposed to
  end; doing it now means doing it twice.
- Changing `validate_strategy` semantics. The "at least one trader"
  rule from the strategies refactor is correct and stays.
