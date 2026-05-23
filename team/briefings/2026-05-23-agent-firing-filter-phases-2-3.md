---
wave: agent-firing-filter-operator-surface-2026-05-22
date: 2026-05-23
phases: [2, 3]
status: dispatched
related_spec: docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md
related_contracts:
  - team/contracts/agent-firing-filter-cli-verbs.md
  - team/contracts/agent-firing-filter-strategy-composer.md
---

# Dispatch briefing: agent-firing-filter Phases 2 + 3

## Why this is dispatching now

The agent-graph capability cascade (Phases A–E, PRs #527 / #546 / #549 / #550 / #551 / #552) merged 2026-05-22. The runtime can now gate any agent on a Filter capability's `FilterSignal` via `PipelineEdge.condition`. **The engine substrate is sufficient.** What's missing is the operator surface that turns it into a discoverable feature.

Phase 1 (`agent-firing-filter-form-and-docs`) shipped via PR #548 and is closed as of 2026-05-23. The deployed `xvn-app` shows the awareness card on Trader-capable slots; the in-app docs route resolves. Operators see "this exists" but cannot configure it.

Phases 2 (CLI) and 3 (StrategyForm composer) deliver the *configuration* surface.

## Spec decisions to honor

From `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md` — re-read sections **Decisions 1–8** before touching code. Load-bearing points:

1. **Authoring lives at the strategy level, not the agent level.** AgentForm doesn't grow inputs in this wave (Phase 1 only added the awareness card).
2. **No popups.** Inline composer routes or in-card-accordion expands — see `CLAUDE.md` no-popup rule.
3. **`Capability` is a closed enum.** Don't introduce a sixth capability or DSL extensions in either phase.
4. **`xvn strategy validate` warning is soft (exit 0).** Single-trader-every-bar is a legitimate config; the warning teaches, doesn't block.
5. **No new engine schema beyond Phase A's existing fields**, *except* the one new column `agents.scope_strategy_id` Phase 3 introduces for the "Save as reusable agent" toggle.

## Phase 2 — `agent-firing-filter-cli-verbs`

**Worktree:** `.worktrees/agent-firing-filter-cli-verbs` (open, on `task/agent-firing-filter-cli-verbs @ 87bd3df`).
**Status:** claimed.

Adds:
- `xvn agent create --name <n> --capability <trader|filter|critic|intern|router> --provider <p> --model <m> --system-prompt <path-or-string> [--skills <ids>...]`
- `xvn strategy add-filter <strategy_id> --filter-agent <agent_id> --gates <role> --when <predicate-json>`
- `xvn strategy remove-filter <strategy_id> --role <filter_role>`
- Soft warning in `xvn strategy validate` when a Trader/Critic AgentRef has no upstream Filter. Pass-through `--no-filter-warning` flag persisted as `acknowledge_no_filter: true` on the strategy.

**Predicate input:** The `--when` value is a JSON-serialized `EdgePredicate` matching the typed form from Phase A. No DSL parser in CLI — the SPA composer produces the JSON.

**Verification target:**
```bash
cargo test -p xvision-cli --test agent_create
cargo test -p xvision-cli --test strategy_add_filter
cargo test -p xvision-engine --test strategies_validate_warns_no_filter
```

## Phase 3 — `agent-firing-filter-strategy-composer`

**Worktree:** `.worktrees/agent-firing-filter-strategy-composer` (open, on `task/agent-firing-filter-strategy-composer @ 87bd3df`).
**Status:** claimed.

Adds:
- **StrategyForm "When does this fire?" section** per non-Filter AgentRef. Two states:
  - Default (no incoming Filter edge): `Every bar.` + `[Add filter →]` button.
  - Active: `Fires when <filter_role>.<field> <op> <value>` + `[Edit]` / `[Remove]`.
- **Inline Filter composer** (routed view OR in-card accordion — *not* a dialog/sheet/popover):
  1. Pick existing Filter agent from workspace OR author a new one inline (provider, model, system_prompt, skills, temperature).
  2. "Save as reusable agent" toggle defaults ON. Off → sets `scope_strategy_id` to the current strategy ID before save.
  3. Predicate composer: signal name, field, op, value. Free-text fallback for signal name if the Filter agent doesn't declare one (see Risks in contract).
  4. On save: agent created/referenced, `AgentRef` appended with `activates: Capability::Filter`, `PipelineEdge` added with predicate.
- **Schema change**: one migration adding `agents.scope_strategy_id TEXT NULL` + `Agent::scope_strategy_id: Option<String>` (serde-defaulted for back-compat). Agent list endpoints filter scoped agents unless `?scope=all` or `?scope=<strategy_id>` is passed.

**Reserve a migration number via `team/MANIFEST.md` BEFORE the worker writes the migration file** — the May parallel-collision pattern has bitten this wave-class twice.

**Verification target:**
```bash
pnpm --filter web typecheck
pnpm --filter web test -- --run components/strategy
pnpm --filter web e2e -- --grep "strategy-firing-filter"
cargo test -p xvision-engine --test agents_scope_strategy_id
cargo build --workspace
```

**End-to-end happy path** (manual):
1. Open `/strategies/:id/edit` with a single-Trader strategy.
2. Click `Add filter →`, choose `Author new agent`, fill fields, leave toggle ON, compose a predicate, save.
3. Verify the strategy now has 2 AgentRefs + 1 PipelineEdge.
4. Verify the new Filter agent appears in `/agents`.
5. Repeat with toggle OFF → new Filter agent does NOT appear in `/agents`; reopening the strategy still shows it inline.

## Parallelism + ordering

- Phases 2 and 3 are `parallel_safe: false` against each other on the `validate.rs` line range. Phase 3 reads `acknowledge_no_filter` if Phase 2 reshapes it; coordinate via the contract's `parallel_conflicts` list.
- Both depend only on Phase A–C/E (all merged). No external blockers.
- **Recommended sequencing**: Phase 2 lands first (smaller surface, validates JSON-predicate shape end-to-end), Phase 3 lands second (builds the composer that emits that JSON).
- If both workers land within minutes of each other, Phase 3 rebases on Phase 2 (or conductor rebases both onto main and resolves the one validate.rs conflict).

## Out of scope for this wave (do not let scope creep)

- Per-Agent default filter at template level. Spec follow-up, not this wave.
- DSL filter authoring (the `xvision-filters` deterministic substrate) — v1 composer authors LLM Filter agents only.
- Graph-canvas UI for arbitrary DAGs — deferred per the capability-first spec.
- Trader-output-conditioned routing ("if trader says hold, route to critic, re-decide") — v2 graph spec, not v1.
- Cross-agent async / message-bus dispatch — per-decision single-threaded stays.

## Worker session opener

Each phase agent runs the standard `team/briefings/_template.md` ritual:

```bash
cd /Users/edkennedy/Code/xvision/.worktrees/<phase-slug>
git status
git branch --show-current
git log --oneline -3 origin/main..HEAD
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-<phase-slug>"
```

State out loud:

> I am on branch `task/<phase-slug>`.
> I am based on `origin/main` at `87bd3df`.
> My contract is `team/contracts/<phase-slug>.md`.
> I will only edit paths matching the contract's `allowed_paths`.
