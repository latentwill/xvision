---
from: strategy-2a-mcp
to: all
topic: claim
created_at: 2026-05-10T22:30:00Z
ack_required: false
---

# `strategy-2a-mcp` track claimed (Plan 2a Phase 2A.B — 7 authoring verbs)

Closes the meaty gap in Plan 2a (MCP authoring) by adding seven verbs to
the existing `xvision-mcp` crate. Worktree
`.worktrees/strategy-2a-mcp`, branch `feature/strategy-2a-mcp-authoring`.

## Plan-deviation rationale

Plan 2a §2A.A asked for a fresh engine-side `mcp` module + new `xvn agent
serve --mcp` subcommand. The codebase has evolved past that:

- The `xvision-mcp` crate already exists with rmcp 1.6 + a `xvn-mcp` binary
  (see `crates/xvision-mcp/`). It currently exposes 8 indicator tools.
- The strategy authoring engine APIs (`bundle::store::FilesystemStore`,
  `templates::registry`, `validate_bundle`, `RiskPreset::expand`) are all
  shipped and used by the `xvn strategy` CLI.

So 2A.A's skeleton tasks are effectively done by prior work, and 2A.D
(seven templates) merged via PR #11. The right adaptation: add the seven
authoring verbs onto the existing router so external agents (Claude Code
MCP, Codex MCP, etc.) get one stable server with both surfaces.

## Scope (Phase 2A.B — Tasks 4–9 + the list_templates from T3)

- `xvn_list_templates` — array of `{ name, display_name, plain_summary }`
- `xvn_create_strategy` — `{ template, name, creator? }` → `{ id }`
- `xvn_get_strategy` — `{ id }` → full StrategyBundle JSON
- `xvn_update_slot` — partial update of `regime|intern|trader` slot
  (`prompt`, `model_requirement`, `allowed_tools` — only non-null fields
  mutate)
- `xvn_set_mechanical_param` — `{ id, key, value }`
- `xvn_set_risk_config` — `{ id, preset|explicit }` (mutually exclusive;
  preset ∈ conservative/balanced/aggressive)
- `xvn_validate_draft` — `{ id, ok, errors: [...] }`

`XvisionTools` gains an optional `xvn_home` field (set via
`with_xvn_home(...)` for tests). Production keeps reading `$XVN_HOME`
with `$HOME/.xvn` fallback.

## Files this track touches

- `crates/xvision-mcp/Cargo.toml` (+ xvision-engine dep, ulid, tempfile dev-dep)
- `crates/xvision-mcp/src/tools.rs` (+ ~500 lines: 5 request structs, 7 tool methods, 9 tests)
- `Cargo.lock`

Zero overlap with currently-open PRs:
- PR #27 (Plan #7 Phase 4 finish) — `crates/xvision-cli`
- PR #28 (Plan #5 Phase 3.D `xvn eval compare`) — `crates/xvision-eval` + cli
- PR #29 (Plan #7 Phase 5 design lock) — docs only

## v1 QA value

Closes Plan 2a's central deliverable: external AI agents (Claude Code,
Codex, Hermes) can drive the strategy authoring loop via MCP — list
templates → create draft → tune slots / mechanical params / risk →
validate — without round-tripping through the operator CLI.

## Out of scope (still deferred)

- **Phase 2A.C** — tool-call dispatch in agent loop (engine-side
  `LlmRequest`/`LlmResponse` extensions for tool-use shape; touches
  `crates/xvision-engine/src/agent/`)
- **Phase 2A.E** — README / smoke recipe + final clippy/fmt sweep
- **Plan 2d** — Dashboard + Wizard (the React frontend has superseded
  the original handlebars dashboard plan; only the server-side
  WizardLoop remains, and it depends on Phase 2A.B which lands here)
