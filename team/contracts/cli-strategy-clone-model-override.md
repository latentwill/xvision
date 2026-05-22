---
track: cli-strategy-clone-model-override
lane: leaf
wave: cli-operator-safety-wave-b-2026-05-22
worktree: .worktrees/cli-strategy-clone-model-override
branch: task/cli-strategy-clone-model-override
base: origin/main
status: ready
depends_on: []
blocks:
  - cli-model-bakeoff                                              # bakeoff per-arm "clone with model X" delegates to this verb
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/strategy.rs                    # add the `clone` subcommand
  - crates/xvision-engine/src/api/strategy.rs                      # clone endpoint (if needed; reuse existing create path where possible)
  - crates/xvision-engine/src/strategies/store.rs                  # if the clone needs a store-level helper for new-id minting
  - crates/xvision-engine/src/agents/store.rs                      # if the cloned strategy needs a paired agent clone
  - crates/xvision-cli/tests/strategy_clone_cli.rs                 # NEW
  - crates/xvision-engine/tests/strategy_clone_model.rs            # NEW
forbidden_paths:
  - crates/xvision-cli/src/commands/eval/**                        # eval-launch override is the sibling track cli-eval-model-override
  - crates/xvision-engine/migrations/**                            # no migration — clone reuses existing tables (strategies + agents)
  - frontend/web/**
  - crates/xvision-mcp/**
interfaces_used:
  - xvision_engine::api::strategy::create                          # existing create-strategy path; clone is a thin wrapper
  - xvision_engine::strategies::store::FilesystemStore             # how strategies persist; clone mints a new ULID and writes
  - xvision_engine::agents::store                                  # if the clone needs to create paired Agent record with the new model
parallel_safe: true                                                # single-file CLI surface; engine touches are append-only helpers
parallel_conflicts:
  - cli-eval-model-override                                        # same wave; queue if both in-flight
  - cli-model-bakeoff                                              # depends on this
verification:
  - cargo test -p xvision-cli --test strategy_clone_cli
  - cargo test -p xvision-engine --test strategy_clone_model
  - cargo test -p xvision-cli
acceptance:
  - **CLI flags.** `xvn strategy clone <strategy_id>` is the new subcommand. Required: `--name <new-name>`. Optional and the central feature: `--provider <name> --model <id>` (both or neither). Without `--provider/--model` the clone is a verbatim copy; with them, the cloned strategy's bound agent uses the new provider/model.
  - **Atomic clone.** The verb creates a new `Strategy` with a fresh `agent_id`/`strategy_id` (ULID), copies every field except the id and name, and (if a paired Agent exists) creates a new Agent record bound to the new strategy with the override `(provider, model)`. All-or-nothing: if any of the writes fail the verb leaves no half-cloned state on disk or in the DB.
  - **No mutation of the source.** The source strategy is byte-identical before and after. The verb's output is the new strategy id (JSON: `{ "strategy_id": "...", "agent_id": "...", "source_strategy_id": "..." }`).
  - **Validation respects strategy invariants.** The cloned strategy passes `validate_strategy` (at least one trader-role agent etc.). If the override `(provider, model)` is unreachable per `effective_providers::resolve_provider` (PR #530), the clone refuses with the same structured `reason` discriminant — operators see the same error they would on launch.
  - **Receipt linkage.** The new strategy carries `cloned_from: Option<String>` (the source strategy id) so audit tooling can chain clones back to the original. Implementation choice: extend `Strategy.metadata` JSON (preferred — no migration) or add a column. Document the decision in PR notes.
  - **JSON contract honored.** `xvn strategy clone --json` returns clean JSON-on-stdout per PR #531; human banners go to stderr.
  - **Tests.**
    - `strategy_clone_cli.rs`: invokes `xvn strategy clone <id> --name X --provider Y --model Z` against a seeded strategy; asserts new strategy exists, source unchanged, paired agent uses override provider/model, `cloned_from` set.
    - `strategy_clone_model.rs`: engine-side test that the clone helper refuses on an unreachable provider with the typed reason.

---

# Scope

Track #4 of `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`.

The intake's framing: operators want to rerun "this strategy" under a new model without manually reconstructing prompts, agents, and metadata. A clone primitive solves this cleanly — the new strategy is otherwise identical to the source and can be launched, A/B-compared, or further cloned just like the original.

This contract pairs with `cli-eval-model-override` (the ephemeral version of the same idea, for one-off model spot-checks). Operators pick clone when the new model deserves its own strategy record (durable test → durable result); they pick eval-override when they're spot-checking and don't want library clutter.

The downstream consumer is `cli-model-bakeoff` (intake #6), which uses this verb to generate one cloned strategy per `(model, strategy)` arm in a bakeoff sweep.

# Out of scope

- Per-run override semantics — `cli-eval-model-override`.
- A `xvn strategy archive` / `delete` cleanup verb for the proliferation of clones — separate concern; today's deletion path is unchanged.
- Dashboard UI for clone. Engine + CLI ship here; SPA follows.
- Multi-agent strategies where each agent gets a different override. v2 if needed.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/cli-strategy-clone-model-override -b task/cli-strategy-clone-model-override origin/main
cd .worktrees/cli-strategy-clone-model-override
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-strategy-clone"
```

# Notes

**Reuse `xvn strategy new` plumbing.** The existing `xvn strategy new --prompt X --provider Y --model Z` verb (in `crates/xvision-cli/src/commands/strategy.rs`) already does most of what clone needs. Clone is essentially "new, but pre-fill every field from the source." Build on that path; don't duplicate.

**`cloned_from` is informational only.** It does not gate anything (no "you can't delete the source while clones exist" semantics — that's overkill). It exists so an audit query "what was strategy X derived from" returns a chain.

**Allowlist.** Dashboard remote CLI allowlist's `strategy` subcommand head is already permitted. Verify the existing template (or absence-of-strict-template) covers `strategy clone` with `--name`, `--provider`, `--model` — if STRICT_TEMPLATES contains a `["strategy", "clone"]` entry it needs the new flags added; if not, the SUPPORTED_SUBCOMMANDS path covers it.
