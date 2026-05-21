---
track: templates-elimination
lane: foundation
wave: qa-chat-rail-2026-05-21
worktree: .worktrees/templates-elimination
branch: task/templates-elimination
base: origin/main
status: ready
depends_on: []
blocks:
  - wizard-folder-recall-honesty
stacking: none
allowed_paths:
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/tests/authoring*.rs
  - crates/xvision-dashboard/src/wizard_loop.rs
  - crates/xvision-dashboard/tests/wizard_loop.rs
  - crates/xvision-dashboard/prompts/wizard.md
forbidden_paths:
  - crates/xvision-engine/src/templates/**
  - crates/xvision-engine/src/strategies/**
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/api/**
  - crates/xvision-engine/src/strategies_folder/**
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/**
  - crates/xvision-observability/**
  - crates/xvision-mcp/**
  - crates/xvision-cli/**
  - frontend/web/**
interfaces_used:
  - authoring::CreateStrategyReq (template field stays for now, accepted as-is)
  - authoring::create_strategy (blank-draft path added; existing template path untouched)
  - api_strategy::create_strategy / create_strategy_agent (existing surface)
  - wizard_loop::run_tool dispatch
  - wizard_loop::agent_tool_defs
parallel_safe: false
parallel_conflicts:
  - "Holds wizard_loop.rs single-writer for the wave. wizard-folder-recall-honesty waits on this contract to merge."
verification:
  - cargo test -p xvision-engine authoring
  - cargo test -p xvision-dashboard
  - cargo clippy -p xvision-engine -p xvision-dashboard -- -D warnings
  - bash scripts/board-lint.sh
acceptance:
  - **Scope is wizard-side only, per the 2026-05-21 conductor descope (see the decision section below).** Engine-side `template_registry`, `manifest.template`, and the typed `MechanicalParams::from_value` dispatch are out of scope for this contract — they move to a follow-up contract (`strategy-template-registry-removal`).
  - **`agents/templates.rs` stays.** That file is `AgentTemplate` for the `/agents/new` agent-picker — a distinct concept from strategy templates. Not touched by this contract.
  - **Wizard scaffolding:** `WIZARD_BLANK_TEMPLATE` (`crates/xvision-dashboard/src/wizard_loop.rs:47-100`) is removed. The `create_strategy` handler at `:779-800` no longer carries a `template` fallback branch. `WizardCreateStrategyInput::template` is removed. The wizard's `create_strategy` tool schema (`:2077` area) no longer declares a `template` field.
  - **Wizard tool surface:** the `list_templates` tool dispatch branch and its entry in `agent_tool_defs` are removed from `wizard_loop.rs`. `list_strategies_folder` and `list_strategy_ideas` are unchanged. `xvn strategy create --template` CLI surface and `/api/agents/templates` HTTP surface are unaffected (out of scope; the wizard simply stops consulting templates).
  - **Wizard's create path stops seeding a placeholder prompt.** The wizard's `create_strategy` handler must produce a draft strategy whose `agents` list is empty (no placeholder agent). The downstream `set_agent` / `create_strategy_agent` flow then attaches the real agent. Implementation options the worker can pick:
    - (a) Add a `authoring::create_blank_strategy(name, creator)` helper that constructs a `Strategy` with `agents: vec![]` directly (using public types from `strategies::`) and saves it via the existing `StrategyStore`. Wizard's `create_strategy` handler calls this helper instead of `authoring::create_strategy`.
    - (b) Special-case the existing `authoring::create_strategy` when called with `template: "custom"` to take the same blank-draft path. The wizard always passes `template: "custom"`.
    - Either option is acceptable; document the choice in the PR description. Both stay within current `allowed_paths` because the `Strategy` struct is public from `strategies::` even though `strategies/**` itself is forbidden territory for edits.
  - **`authoring::list_templates` is removed** (the function in `authoring.rs`). Its callers were `list_templates` in wizard_loop (removed by this contract) and tests under `tests/authoring*.rs` (allowed). Any other caller is out of scope — if grep surfaces one outside allowed_paths, file a queue note and stop.
  - **Defensive fix for chained-write:** the wizard's `create_strategy` handler does not cache `self.last_draft_id` from a failed response. On `create_strategy` failure, the wizard surfaces the engine error verbatim and does not chain `create_strategy_agent` against a phantom id. New test asserts: simulated `create_strategy` failure → no follow-on `create_strategy_agent` call observed; `self.last_draft_id` not set.
  - **Wizard prompt:** `crates/xvision-dashboard/prompts/wizard.md` is rewritten so the library surface narrative points at the strategies folder only — no mention of templates as a separate concept. Include an instruction: "if the folder is empty, offer to seed it with `xvn strategies init` (prepop) before creating a blank draft." This folds finding #1's wizard prompt half into this track.
  - **Hard save-gate untouched:** `crates/xvision-engine/src/agents/validate.rs` is not edited. The 200-char + placeholder-SHA-256 rule remains load-bearing for direct API / MCP / wallet-plan callers that submit a placeholder prompt.
  - **Tests required:**
    - Wizard integration test: `create_strategy` with no template + no prompt returns a `{ id }`; downstream `set_agent` / `update_strategy` fills in the prompt; final save passes the save-gate. Use the operator's 2026-05-21 transcript shape (Gemini Flash Lite 3.1 agent + fibonacci+RSI request) as the test scenario.
    - Engine test under `tests/authoring*.rs` covering the blank-draft path (whichever option the worker picked).
    - Defensive: simulated `create_strategy` failure → no follow-on `create_strategy_agent`; `self.last_draft_id` not set.
    - Hard save-gate regression: direct `api_strategy::create_strategy` with `template: "trend_follower"` still seeds a real strategy that passes the save-gate (no regression on the existing template path).
  - **Grep guard:** `rg --hidden -n 'WIZARD_BLANK_TEMPLATE|WizardCreateStrategyInput.*template' crates/` returns only deletion-adjacent occurrences. `rg --hidden -n 'list_templates' crates/xvision-dashboard/ crates/xvision-engine/src/authoring.rs` returns only deletion-adjacent occurrences. The engine-side `template_registry`, `manifest.template`, `agents/templates.rs`, MCP/CLI surfaces are intentionally untouched and will surface in the follow-up.
  - **No changes outside listed allowed paths.**
---

# Scope

**Descoped 2026-05-21 to wizard-only.** See the "Conductor descope
decision" section below for the full rationale and the worker's
findings that prompted the descope.

This contract removes the wizard's reliance on the strategy
`template_registry`:

- The wizard no longer surfaces a `list_templates` MCP tool.
- The wizard's `create_strategy` tool no longer accepts a `template`
  field and no longer scaffolds with `WIZARD_BLANK_TEMPLATE`.
- `authoring::create_strategy` gains a blank-draft path that
  produces a `Strategy` with `agents: vec![]` (no placeholder
  prompt). The wizard exclusively uses that path; the existing
  template-backed path stays for CLI / API / MCP callers.
- The wizard's prompt is rewritten so the library surface narrative
  points at the strategies folder only.
- Defensive: the wizard no longer chains `create_strategy_agent`
  against a phantom draft id on `create_strategy` failure.

Operator stance from 2026-05-21 ("whatever the user has in its
strategy folder will be the context for the user") is honored
behaviorally — the wizard now consults only the folder, not
templates. The deeper engine refactor (delete `template_registry`,
remove `manifest.template`, refactor typed `MechanicalParams`
dispatch, migrate template content to prepop seeds, update CLI /
MCP / agents-new page) moves to the follow-up contract
**`strategy-template-registry-removal`** (status `deferred` —
opens after this one merges and the wizard fix is verified).

This is the resolution to the P0 placeholder deadlock in
`team/intake/2026-05-21-qa-chat-rail-strategy-create-broken.md`
finding #2: the wizard stops feeding the save-gate a forbidden
prompt. The save-gate stays untouched and load-bearing for direct
API / MCP / wallet-plan callers.

The defensive fix for finding #3 (don't chain
`create_strategy_agent` against a phantom draft id) lands here
because it touches the same `wizard_loop.rs` create handler.

The wizard-prompt half of finding #1 (empty-folder narrative;
offer prepop when truly empty) lands here too because the prompt
is being rewritten anyway. The behavioral half of finding #1
remains in the dependent `wizard-folder-recall-honesty` track.

# Out of scope

- `crates/xvision-engine/src/templates/**` — the strategy
  `template_registry`. Stays intact for CLI / API / MCP callers.
  Moves to the `strategy-template-registry-removal` follow-up.
- `crates/xvision-engine/src/strategies/**` — including
  `manifest.template`, `MechanicalParams::from_value` dispatch,
  store, and validate. Stays intact. Follow-up.
- `crates/xvision-engine/src/agents/templates.rs` — that file is
  `AgentTemplate` for the `/agents/new` agent-picker, a distinct
  concept from strategy templates. Not deleted by this or the
  follow-up contract. The "templates elimination" framing is
  about strategy templates only.
- `crates/xvision-engine/src/api/strategy.rs` — the public
  `CreateStrategyReq` shape carries `template: String`; that field
  stays for now. The wizard internally passes `template: "custom"`
  (or whichever marker the blank-draft path uses). Removing the
  public field requires touching MCP/CLI surfaces and moves to
  the follow-up.
- `crates/xvision-mcp/**` and `crates/xvision-cli/**` — both consume
  `CreateStrategyReq` and `xvn strategy create --template`.
  Untouched.
- The hard save-gate at `crates/xvision-engine/src/agents/validate.rs`.
  Not edited. 200-char + placeholder rule stays.
- The eval engine, observability, broker, wallet plan.
- Frontend UI. The two IA tracks own those.
- `chat_messages` insert failures (`chat-messages-insert-failing`).
- DB migrations.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/templates-elimination status
git -C .worktrees/templates-elimination log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/templates-elimination
#   - base is up to date with origin/main (or rebase planned)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/templates-elimination \
  -b task/templates-elimination origin/main
```

# Notes

**Prior wave decisions, status update:**

- `wizard-strategy-template-optional` (archived 2026-05-18) said
  "Templates stay where they are; the wizard simply stops *requiring*
  one." This contract carries that further on the wizard side:
  the wizard no longer consults templates at all. The engine-side
  "templates stay where they are" remains true for this contract;
  the follow-up `strategy-template-registry-removal` flips it.
- `agent-pipeline-template-library-expansion` (#409, archived
  2026-05-21) expanded the in-engine strategy template library.
  Its content is the raw material for the follow-up contract's
  prepop seed migration. Not touched here.

## 2026-05-21 — conductor descope decision

Initial contract conflated two distinct surfaces and named files
outside the `allowed_paths` it declared. Worker stopped before
writing code and documented the mismatch in the appended checkpoint
at the bottom of this file. Conductor accepted the worker's
recommendation (option C — split into two contracts) and
re-authored this contract to execute the safe wizard-only scope.

**Two distinct "templates" surfaces in xvision** (the original
contract collapsed them):

- **Strategy templates** at `crates/xvision-engine/src/templates/`
  (`template_registry`). Eight strategy starters: Breakout, Custom,
  MeanReversion, Momentum, NewsTrader, RangeTrade, Scalping,
  TrendFollower + a marketplace baseline. Dispatched by
  `manifest.template`. Consumed by `authoring::create_strategy`,
  the `xvn strategy create --template` CLI verb, the MCP
  `create_strategy` tool, and the wizard's `create_strategy`
  handler. **This is the "templates" the operator wants
  eliminated.** Moves to the follow-up contract.
- **Agent templates** at `crates/xvision-engine/src/agents/templates.rs`
  (`AgentTemplate`, 615 lines). Nine agent-composition starters
  (single-trader, analyst-executor, risk-checked-trader, etc.)
  used by the `/agents/new` agent-picker. Distinct concept; useful
  scaffolding for the operator when adding an agent to a strategy.
  **Not eliminated. Stays.**

**Load-bearing dependency on `manifest.template`:**
`MechanicalParams::from_value(template: &str, value)` at
`crates/xvision-engine/src/strategies/mechanical.rs:71` dispatches
typed per-template param validation on the manifest's template
string. Removing the field requires rewriting this dispatch (collapse
to a single shape, or migrate the discriminator). Out of scope for
this contract; in scope for the follow-up.

**This contract's narrow job** is to land the operator-visible
fix (chat-rail can create a strategy without tripping the save-gate)
and leave the engine-side refactor for a dedicated follow-up where
the typed-params dispatch and the MCP/CLI surface can be addressed
deliberately. The follow-up contract `strategy-template-registry-removal`
opens with status `deferred` and becomes `ready` after this contract
merges.

Append checkpoints / PR links below.

## 2026-05-21 — worker STOP, scope-vs-allowed-paths mismatch

Worker started the contract from `.worktrees/templates-elimination` on
`task/templates-elimination` (clean tree, branched from `origin/main`).
After mapping the work, the acceptance criteria require changes to
files outside `allowed_paths`. Per the prompt's explicit rule
("If something tempts you outside, STOP and update the contract via a
separate decision — do not silently exceed scope"), worker halted
without writing code.

**Findings — files that must change to satisfy acceptance, not in
`allowed_paths`:**

1. **`crates/xvision-engine/src/templates/**`** is the literal
   `template_registry` consumed by `authoring::create_strategy`. The
   contract names `template_registry` as a thing to eliminate but does
   not allow editing the directory that contains it. Cannot delete
   `template_registry::get(&req.template)` without also deleting / refactoring
   `templates/{breakout,custom,mean_reversion,momentum,news_trader,range_trade,scalping,trend_follower}.rs`
   and `templates/{mod,registry}.rs`.

2. **`crates/xvision-engine/src/strategies/mechanical.rs:71`** —
   `MechanicalParams::from_value(template: &str, ...)` dispatches typed
   per-template param validation on `Strategy.manifest.template`.
   Removing `manifest.template` requires rewriting this dispatch (e.g.
   collapse everything to `Custom`, or migrate the discriminator).
   File is not in `allowed_paths`.

3. **`crates/xvision-engine/src/strategies/mod.rs:327`** —
   `ma_crossover_template()` construction sets `template: "ma_crossover"`.
   Not in `allowed_paths`.

4. **`crates/xvision-engine/src/strategies/store.rs:301`**,
   **`crates/xvision-engine/src/strategies/templates.rs:117,157,205`**,
   **`crates/xvision-engine/src/strategies/validate.rs:348`** — all
   construct `PublicManifest { template: ... }`. Not in `allowed_paths`.

5. **`crates/xvision-engine/src/agents/templates.rs`** (the 615-line
   file) is `AgentTemplate` for the `/api/agents/templates` picker
   route. Distinct surface from the strategy `template_registry`. The
   contract acceptance line 56 conflates them ("Pipeline-template
   content migration: `agents/templates.rs` (615 lines) is deleted").
   Deleting it cascades into:
   - **`crates/xvision-engine/src/agents/mod.rs:27`** — re-exports
     `builtin_templates`, `AgentTemplate`. Not allowed.
   - **`crates/xvision-engine/src/api/agents.rs:415-416`** — `templates()`
     handler returns `builtin_templates()`. Not allowed.
   - **`crates/xvision-dashboard/src/server.rs:70,163`** —
     `/api/agents/templates` route registration. Not allowed.
   - **`crates/xvision-dashboard/src/routes/agents.rs:12,50,168`** —
     uses `AgentTemplate`. Not allowed.
   - **`crates/xvision-engine/tests/seeded_artifacts.rs:13,15,49,88`** —
     test references `builtin_templates`. Test file not in allowed test
     globs.
   - **Frontend agents-new page** consumes `/api/agents/templates`.
     `frontend/web/src/routes/**`, `features/**`, `components/**` are
     explicitly **forbidden**.

6. **`crates/xvision-mcp/src/tools.rs`** — MCP tools surface declares
   `CreateStrategyReq`-shaped tool input. Schema changes to drop
   `template` ripple here. Not in `allowed_paths`.

7. **`crates/xvision-cli/src/commands/strategy.rs`** and
   **`tests/strategy_cli.rs`** — CLI `xvn strategy create --template ...`
   surface. Not in `allowed_paths`.

8. **Tests outside allowed test globs that reference the removed surface:**
   `tests/seven_templates.rs`, `tests/template_validation.rs`,
   `tests/tokens.rs`, `tests/strategy_update_metadata.rs`,
   `tests/llm_dispatch.rs`, `tests/mechanical_params.rs`,
   `tests/strategy_roundtrip.rs`, `tests/seeded_artifacts.rs`,
   dashboard `tests/http.rs`, `tests/inspector_routes.rs`,
   `tests/strategy_patch_route.rs`. Allowed globs only cover
   `tests/authoring*.rs`, `tests/strategy_api*.rs`, `tests/strategies_folder.rs`.

**Total ripple surface: 23 source files across 5 crates + frontend
(forbidden) + ~11 test files (most outside allowed globs).**

**Additional content-migration note for acceptance line 56:** the
contract says the 615-line `agents/templates.rs` content migrates to
`strategies_folder/prepop` seeds. But (a) that file is `AgentTemplate`
(agent-picker UI seeds), not strategy starters, and (b)
`strategies_folder/prepop.rs` already prepopulates from the
`docs/strategies/templates/**` tree via `include_dir!`. The natural
source for new seed content is to write new markdown under
`docs/strategies/templates/` (currently outside allowed_paths) so the
existing prepop pipeline picks it up; `prepop.rs` itself does not need
data tables added.

**Conductor decision needed — three reasonable paths:**

- **(A) Expand `allowed_paths` to cover the actual surface.** Adds at
  minimum: `crates/xvision-engine/src/templates/**`,
  `crates/xvision-engine/src/strategies/{mechanical,mod,store,templates,validate}.rs`,
  `crates/xvision-engine/src/agents/mod.rs`,
  `crates/xvision-engine/src/api/agents.rs`,
  `crates/xvision-dashboard/src/{server,routes/agents,routes/strategies}.rs`,
  `crates/xvision-mcp/src/tools.rs`,
  `crates/xvision-cli/src/commands/strategy.rs`,
  `crates/xvision-engine/tests/**`,
  `crates/xvision-dashboard/tests/**`,
  `docs/strategies/templates/**`, and the frontend agents-new route
  (currently forbidden). Probably reclassifies this contract as
  non-`parallel_safe` for the entire wave (it already is).

- **(B) Descope: keep `manifest.template` as a free-form string,
  remove only the wizard scaffolding + `list_templates` tool +
  `authoring::list_templates` + the `CreateStrategyReq.template`
  field (defaulting `create_strategy` to a blank `Custom`-style
  draft built in-place without consulting `template_registry`).**
  Leave `template_registry` itself intact (still used by `xvn
  strategy create --template` CLI verb and the `/api/agents/templates`
  picker). The "templates leave the engine entirely" framing in the
  contract Notes becomes "templates stay in the engine but the wizard
  stops consulting them" — which is exactly the reversal of decision
  `wizard-strategy-template-optional` that the Notes say is being
  undone. This is the smallest safe scope inside current
  `allowed_paths`; it lands the placeholder-deadlock fix (acceptance
  line 61 — save-gate untouched) and the chained-write defensive fix
  (line 59) without touching the engine's structural template
  dispatch.

- **(C) Split into two contracts.** Wave 1 (this branch): wizard-only
  descope from option B + the prompt rewrite. Wave 2 (new contract):
  full engine elimination, broader `allowed_paths`, after the wizard
  side is verified in production.

Worker recommendation: **(C)**, with this branch executing (B) only
and a follow-up contract created for the engine half. Rationale: the
wizard scaffolding is the load-bearing bug for the qa-chat-rail wave;
the engine refactor is structurally invasive (typed `MechanicalParams`
dispatch, frontend agents picker, CLI surface) and benefits from
being its own review.

Branch `task/templates-elimination` left clean at `origin/main` HEAD,
no commits, no PR. Awaiting conductor direction.

