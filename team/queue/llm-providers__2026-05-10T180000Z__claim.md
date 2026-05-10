---
from: llm-providers
to: all
topic: claim
created_at: 2026-05-10T18:00:00Z
ack_required: false
---

# `llm-providers` track claimed (B.3 Phase 1 — config schema)

Session 3 (formerly docker-image → leverage-items → strategy-2a-templates,
all merged) is taking the LLM providers backend plan, **Phase 1 only**.
Worktree `.worktrees/llm-providers`, branch `feature/llm-providers-phase-1`.
Plan slice:
[`docs/superpowers/plans/2026-05-10-llm-providers-and-per-arm-models-plan.md`](../../docs/superpowers/plans/2026-05-10-llm-providers-and-per-arm-models-plan.md)
**Phase 1 — Tasks 1–4**.

## Scope

Phase 1 is the type-system + config-schema foundation for per-arm LLM
selection. It's pure `xvision-core` work — no CLI, no engine, no API, no MCP
changes.

- T1 `ProviderEntry` + `ProviderKind` types (kebab-case enum: `anthropic`, `openai-compat`, `local-candle`)
- T2 `providers: Vec<ProviderEntry>` field on `RuntimeConfig` with `#[serde(default)]`
- T3 Auto-derive `_default_intern` synthetic row from `[intern]` block; uniqueness validation; reserved underscore prefix
- T4 Update `config/default.toml` with explicit `[[providers]]` rows (anthropic, openai, ollama-local)

## Out of scope (deferred, can ship in parallel)

- **Phase 2** — `SlotRef = { provider, model }` newtype + `ArmKind::Trader` extension
- **Phase 3** — `ProviderRegistry` + `run_ab_compare` wiring
- **Phase 4** — `xvn provider` CLI subcommand (waits until xvision-cli scope is settled across active sessions)
- **Phase 5** — UI design lock + migration note

## Files this track touches

- `crates/xvision-core/src/config.rs` (modify)
- `config/default.toml` (modify)
- `team/MANIFEST.md` (add row)
- `team/queue/llm-providers__*` (new)
- `team/status/llm-providers.md` (new)

Zero overlap with active sessions:
- `eval-engine-3b` (session 1): `crates/xvision-eval/`, `crates/xvision-engine/src/eval/`
- `frontend-2-home-and-health` (session 2): `crates/xvision-dashboard/`, `frontend/web/`

## Why this slice

Phase 1 unblocks:
- B.6 Settings & Onboarding (Settings depends on `[[providers]]` config + `xvn provider` CLI)
- The "backtest one strategy against N LLMs" demo claim — Phase 2/3 wires it through; Phase 1 lays the type foundation.

For v1 QA testing, having explicit `[[providers]]` rows in `config/default.toml`
also gives ops folks a clearer surface to swap models without code changes.
