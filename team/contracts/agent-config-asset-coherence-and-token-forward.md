---
track: agent-config-asset-coherence-and-token-forward
lane: integration
wave: eval-traces-2026-05-19
worktree: .worktrees/agent-config-asset-coherence-and-token-forward
branch: task/agent-config-asset-coherence-and-token-forward
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agents/**                         # extend the validator added by F-5 (PR #346)
  - crates/xvision-engine/src/agent/llm.rs                      # forward max_tokens + temperature into LlmRequest
  - crates/xvision-engine/src/agent/observability.rs            # if dispatch happens here, thread the fields through
  - crates/xvision-engine/src/eval/executor/paper.rs            # call-site: read slot.max_tokens / temperature before dispatch
  - crates/xvision-engine/src/eval/executor/backtest.rs         # same
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
  - crates/xvision-engine/src/eval/executor/mod.rs   # F-3 owns this
interfaces_used:
  - xvision-engine::agents::validator::* (added by F-5 / PR #346 — extend, don't duplicate)
  - xvision-engine::agent::llm::LlmRequest
parallel_safe: true
parallel_conflicts:
  - agent-prompt-tool-schema-drift-lint (PR #346, F-5 — same validator file; extend the rule list rather than duplicating)
  - eval-provider-error-classify-retry (PR #347, F-2 — modified agent/llm.rs typed errors; the LlmRequest field addition is a separate hunk)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine agents
  - cargo test -p xvision-engine agent::llm
acceptance:
  - **Rule 1 (name ↔ asset coherence)**: `AgentStore::create` / `AgentStore::update` reject a slot whose `agent_name` contains a recognised asset token (`SOL`, `BTC`, `ETH`, `DOGE`, `ADA`, etc. — pick a small static list) that does not appear (case-insensitive substring) in `system_prompt`. Typed error variant added to whatever enum F-5 introduced (`PromptSchemaDriftError`) — pick a natural sub-name like `NameAssetMismatch { name_asset, missing_from_prompt: true }`. The audit's `SOL 4h trend breakout trader agent` (name says SOL, prompt says ETH/USD throughout) is the prototypical case.
  - **Rule 2 (placeholder prompt rejection)**: refuse saves when `system_prompt`'s SHA-256 matches a known-default-placeholder content hash (the audit saw `"You are a trading agent. Decide based on the inputs provided. Output JSON: {action, conviction (0-1), justification (one line)}."` — a 129-char default). Compute the hash once in a const; allow override only via an explicit `--allow-placeholder` flag or an `AgentSlot::allow_placeholder: bool` field set to `true` (don't add the flag unless an existing CLI verb already has it; default behaviour is reject).
  - **Rule 3 (slot field forwarding)**: extend `LlmRequest` to carry `max_tokens: Option<u32>` and `temperature: Option<f64>`. Populate from `agent_slot.max_tokens` and `agent_slot.temperature` at the dispatch site in paper.rs/backtest.rs (the audit found these fields drop to `None` regardless of slot value because the dispatch boundary doesn't read them). Both providers in `xvision-engine/src/agent/llm.rs` (OpenAI-compat + Anthropic, if both exist) must forward them; missing slot value → don't include in the outbound JSON (let provider default apply).
  - Tests:
    * Validator unit tests for both new rules (extending the existing F-5 test module).
    * Round-trip test: an agent slot with `max_tokens=64, temperature=0.2` is saved, loaded, and the next dispatch's outbound JSON includes `"max_tokens": 64, "temperature": 0.2`.
    * Negative test: an agent slot with `max_tokens=None` produces an outbound JSON with no `max_tokens` key (not `null`).
  - **Lint command**: extend the F-5 lint to report the two new rule violations against existing seeded agents (no auto-mutation). The audit's 2 placeholder-prompt agents and the SOL agent show up as 3 findings.
  - No migration needed — `agent_slots.max_tokens` and `temperature` columns already exist; this contract just fixes the silently-dropped-at-dispatch bug.
---

# Scope

Intake F-4 carve of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

Audit findings F-5 was a leaf carved into PR #346 (unregistered-tool refs
+ action-schema-enum drift). This contract adds the **three remaining
rules** from the broader F-4 ask:

1. Name-vs-asset coherence (the `SOL 4h trend breakout trader agent`
   case where name says SOL but prompt+inputs are ETH).
2. Placeholder-prompt rejection (the 129-char default that two seeded
   agents currently ship with).
3. `max_tokens` / `temperature` slot fields are forwarded into the
   outbound provider request (audit found they were silently dropped).

# Out of scope

- Forwarding `top_p` or other less-impactful sampling fields (separate).
- Provider-specific param mapping (assume OpenAI-compat shape).
- Frontend / wizard surfacing of the new error variants — UI auto-picks
  up new error strings via the existing tool_result surfacing path.
- Anything in `executor/mod.rs` (F-3 owns it).

# Coordination

F-5 (PR #346) added the validator module. This contract extends its rule
list. If F-5 hasn't merged when this lands, the worker should be ready to
rebase on top of #346 since they share `crates/xvision-engine/src/agents/validator.rs`
(or whatever F-5 named it).

F-2 (PR #347) added typed errors + retries in `agent/llm.rs`. This
contract adds two fields to `LlmRequest` — separate hunks, should not
text-conflict, but coordinate the rebase if needed.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/agent-config-asset-coherence-and-token-forward status
git -C .worktrees/agent-config-asset-coherence-and-token-forward log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-config-asset-coherence-and-token-forward -b task/agent-config-asset-coherence-and-token-forward origin/main
```

# Notes

Keep the recognised-asset list small and explicit. Don't try to parse
trading pairs out of free-form prompt text — substring match on a static
list of 6–8 tokens is sufficient and false-positive-friendly.
