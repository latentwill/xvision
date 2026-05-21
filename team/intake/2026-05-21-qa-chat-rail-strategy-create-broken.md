# Intake — 2026-05-21 — QA: chat rail can't see strategies folder, create_strategy hard-blocks on its own placeholder, /memory & /strategies-folder IA collapse, eliminate templates

Operator findings from a single dashboard session, 2026-05-21. The
through-line is the chat-rail "let's make me a strategy" path: the
wizard couldn't see folder content, then failed twice on a save-gate
that rejects the prompt the wizard itself just seeded. Chat persistence
also errored three times in a row in the same session, which is its
own separate failure surface. Two information-architecture changes
(`/strategies-folder` and `/memory` as top-level routes) round out the
batch.

**Operator follow-on (2026-05-21, after initial intake landed):**
the `templates` concept should be eliminated entirely. The strategies
folder is the only library surface. Whatever the operator has in
their strategies folder is the context for authoring — including any
"example template" content, which lives as **seeded folder entries**
(via `strategies_folder::prepop`), not as a parallel `template_registry`.
This reverses the explicit "templates stay where they are" stance of
the merged `wizard-strategy-template-optional` contract (archived
2026-05-18) and supersedes the V2F-wave `agent-pipeline-template-library-expansion`
(#409). The rest of the wave depends on it: without templates the
wizard's `create_strategy` no longer has a placeholder-seeding path,
which dissolves finding #2's deadlock cleanly.

## Source

Operator chat session, 2026-05-21. Verbatim wizard output and tool-call
log copied into the intake brief — the relevant lines are:

- Wizard narrative: "It looks like your strategy folder is empty, and
  I didn't find any specific Fibonacci-based ideas in the library."
- Tool log: `list_strategy_ideas completed`, `list_strategies_folder
  completed`, `list_templates returned 9 templates`. The wizard saw
  templates but reported the folder as empty.
- After three attempts to "just make the strategy," the wizard surfaced:
  > create_strategy stuck — operator review needed.
  > `create_strategy` failed 2× in a row with the same error.
  > internal: save validation failed: slot 'main': system_prompt is
  > the default placeholder or fewer than 200 characters; replace
  > with a real trading prompt before saving
- Follow-on cascade: `create_strategy_agent` returned
  `not found: strategy 'draft-7bcf9274-1237-4d94-912f-8186175f7e6f'`.
- Three back-to-back stream errors: `insert chat_messages row` on
  consecutive operator messages ("yes", "Summarize this week", "can
  you finish the strategy").

## Already in flight / queue notes

- Filter v1 / v1.5 + DSPy adoption: separate intake at
  `team/intake/2026-05-21-dspy-dsrs-optimizer-adoption.md`. Unrelated.
- Canonical template / trader scaffolding: see
  `team/intake/2026-05-20-canonical-template-needs-trader.md`. The
  "blank single-agent" template referenced by the wizard
  (`WIZARD_BLANK_TEMPLATE` at `crates/xvision-dashboard/src/wizard_loop.rs:99`)
  is upstream of finding #2 below — if that intake reshapes the blank
  template, the save-gate fix in #2 should be coordinated with it
  rather than landed in isolation.
- Strategies refactor → agent composition (the
  `Strategy { agents: Vec<AgentRef> }` shape): landed per
  `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md`.
  Findings here ride on top of that shape; nothing reopens it.

## V2 roadmap items (not contracts here)

None. All findings are on existing surfaces.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P2 | Wizard told the operator "your strategy folder is empty, and I didn't find any specific Fibonacci-based ideas in the library" even though `list_strategies_folder` and `list_strategy_ideas` both returned "completed" in the same turn. Either both genuinely returned empty (in which case the wizard should narrate that *and* offer to seed prepop ideas, not hide the empty-state) or the wizard prompt is collapsing non-empty results into a "didn't find anything specific" narrative. Operator was actively asking for a fibonacci+RSI seed — that's a textbook prepop hit (`strategies_folder::prepop`). | `wizard-folder-recall-honesty` |
| 2 | **P0** | `create_strategy` is unusable from the chat rail. The wizard's `create_strategy` tool (`crates/xvision-dashboard/src/wizard_loop.rs:779-800`) scaffolds a draft with `WIZARD_BLANK_TEMPLATE` and then calls `create_default_strategy_agent` to seed the `main` slot. The agent the wizard seeds carries the canonical 129-char placeholder prompt; the save-gate at `crates/xvision-engine/src/agents/validate.rs:157-172` and `:324` rejects it with `slot 'main': system_prompt is the default placeholder or fewer than 200 characters`. The wizard's own scaffolding is what the validator rejects — the path cannot succeed. Operator hit it 3× in a row. | Resolved by `templates-elimination` (the deadlock dissolves when the wizard stops seeding a placeholder-prompt template), with a defensive sub-task on top: don't chain `create_strategy_agent` against a draft id when the parent strategy failed to persist. |
| 3 | P1 | Once finding #2 fires, `create_strategy_agent` is called against the draft id the wizard cached (`self.last_draft_id`) even though the strategy was never persisted. Error surface: `not found: strategy 'draft-7bcf9274-…'`. The wizard should treat a failed `create_strategy` as a hard stop and not chain a follow-on agent write against a phantom id. Fixing #2 hides this 99% of the time, but the chained-write-on-failure is a separate defensive bug. | Folds into `templates-elimination` as a defensive sub-task; not a separate track. |
| 4 | P1 | Three sequential `insert chat_messages row` stream errors on consecutive operator messages ("yes" / "Summarize this week" / "can you finish the strategy"). Insert site: `crates/xvision-engine/src/chat_session/store.rs:87-98`. No obvious operator action between the three attempts — points at schema drift, FK on a missing session row, `(session_id, seq)` unique-constraint collision (seq not advancing), or pool/transaction state corrupted by the earlier failed strategy writes. Needs root-cause not a retry-loop band-aid. | `chat-messages-insert-failing` |
| 5 | P2 | IA: `/strategies-folder` is registered as a sibling top-level route at `frontend/web/src/routes.tsx:79`. Operator wants it folded into `/strategies` as a `List | Folder` view toggle in the page header — URL becomes `/strategies?view=folder` (or keep `/strategies-folder` as a backwards-compat alias that redirects to the toggled state). Folder is a view of strategies, not its own destination. | `strategies-folder-into-view-toggle` |
| 6 | P2 | IA: `/memory` is registered as a top-level route at `frontend/web/src/routes.tsx:95` pointing at `features/memory/MemoryPage.tsx`. Operator wants Memory demoted to `/agents/memory` and surfaced from the Agents page — matching the existing per-agent `components/agent/MemoryTab.tsx`. Global view becomes "memory across all agents," not its own top-level concept. | `memory-into-agents-section` |
| 7 | **P0** | Eliminate `templates` entirely. The strategies folder is the library; any starter / example content lives as seeded folder entries (`strategies_folder::prepop`). Removes `list_templates` tool, the `template_registry` in `crates/xvision-engine/src/authoring.rs:148-175`, the `template: String` field on `CreateStrategyReq` (`crates/xvision-engine/src/api/strategy.rs:40` and `crates/xvision-engine/src/authoring.rs:43`), the `template` field on `StrategyManifest` (`crates/xvision-engine/src/strategies/manifest.rs:10`), and `WIZARD_BLANK_TEMPLATE` from `wizard_loop.rs`. Pipeline-template content from `crates/xvision-engine/src/agents/templates.rs` (the canonical-template work merged 2026-05-19 via #409) migrates into prepop seed entries. Upstream of finding #2: a wizard that has nothing to seed never trips the placeholder validator. | `templates-elimination` |

Seven findings, five tracks (findings #2 and #3 fold into `templates-elimination`).

## Sequencing

`templates-elimination` is the spine of the wave. The hard rules
for it:

- **It is the wave's foundation.** All other tracks treat it as
  `depends_on:` or `parallel_safe: false` against
  `crates/xvision-dashboard/src/wizard_loop.rs`,
  `crates/xvision-engine/src/authoring.rs`,
  `crates/xvision-engine/src/api/strategy.rs`,
  `crates/xvision-engine/src/strategies/manifest.rs`.
- **It absorbs the placeholder-deadlock fix.** Removing the
  placeholder-seeding template path is the cleanest way to resolve
  the validator-vs-wizard contradiction. The save-gate at
  `crates/xvision-engine/src/agents/validate.rs:157-172,324` is
  kept as-is for direct API / MCP callers; the wizard simply stops
  feeding it a placeholder.
- **It absorbs the chained-write defensive fix.** Same touch point
  (`wizard_loop.rs` create path), same review.
- **`wizard-folder-recall-honesty` waits on it.** The "if folder
  is empty, offer prepop" branch is partly redundant once prepop
  is the only seed surface — better to land that wizard prompt
  change after templates-elimination so we change the prompt once.
- **The two IA tracks (`strategies-folder-into-view-toggle`,
  `memory-into-agents-section`) are frontend-only and parallel-safe**
  with everything in this wave. They can be claimed independently.
- **`chat-messages-insert-failing` is engine-only**, touches
  `crates/xvision-engine/src/chat_session/`, parallel-safe with
  everything else. Start with an audit pass that surfaces the
  swallowed SQLx error before scoping the fix.

## Track summaries

### `templates-elimination` (P0, engine + dashboard wizard + library content)

Foundation. The strategies folder becomes the only library surface.
Templates as a parallel registry go away: no `list_templates` tool,
no `template_registry`, no `template: String` field on the
`CreateStrategyReq` API / MCP shape, no `template` field on the
`StrategyManifest`, no `WIZARD_BLANK_TEMPLATE`. The content that
the templates registry currently ships migrates into seeded folder
entries via `strategies_folder::prepop`. This dissolves finding #2
(placeholder deadlock) because the wizard no longer scaffolds a
placeholder prompt.

Why this reverses prior wave decisions, explicitly:

- The merged `wizard-strategy-template-optional` contract
  (archived 2026-05-18) said: "Templates stay where they are; the
  wizard simply stops *requiring* one." That position is now
  retired. Templates leave the engine entirely.
- The merged `agent-pipeline-template-library-expansion` (#409)
  expanded the in-engine template library; its content is the
  raw material for the prepop seed migration in this track, not
  a competing source of truth. After this track lands, that
  content lives only as folder entries.

Code paths to remove or repoint:

- `crates/xvision-engine/src/authoring.rs:43` (`pub template: String`
  on `CreateStrategyReq`) — drop field.
- `crates/xvision-engine/src/authoring.rs:148-175` (`list_templates`,
  `create_strategy` body using `template_registry::get`) — drop
  function, replace body so `create_strategy` builds a blank
  draft directly (single-agent, prompt unset, no
  mechanical_params).
- `crates/xvision-engine/src/agents/templates.rs` (615 lines) —
  the pipeline-stage starter content moves to prepop seed
  entries under `strategies_folder::prepop`. The conventions
  documented here (intern / trader / risk / executor as labels)
  are retained as **documentation** in the prepop module header
  and CLAUDE.md, not as code. File deleted at the end of the
  track.
- `crates/xvision-engine/src/api/strategy.rs:40` (`template: String`)
  and `:270`, `:428` (manifest.template reads) — drop field and
  the downstream tag derivation (line `:428`).
- `crates/xvision-engine/src/strategies/manifest.rs:10`
  (`pub template: String`) — drop field. Migration of any existing
  on-disk strategies that carry the field: the field becomes
  serde-ignored on read (`#[serde(default, skip_serializing)]`
  for one release) so older manifests still load. Drop the
  ignored field entirely in a follow-up release.
- `crates/xvision-dashboard/src/wizard_loop.rs:47-100` and
  `:779-800` — drop `WIZARD_BLANK_TEMPLATE`, drop
  `WizardCreateStrategyInput::template`, drop the `template`-fallback
  branch. The wizard's `create_strategy` tool schema sheds the
  `template` field entirely.
- `crates/xvision-dashboard/src/wizard_loop.rs:1072` and
  `:1090` already expose `list_strategies_folder` /
  `list_strategy_ideas` — they stay; they are the new library
  surface.
- `crates/xvision-dashboard/src/wizard_loop.rs:2298-2350` area —
  the `list_templates` tool definition leaves the wizard
  dispatch and tool-defs.
- `crates/xvision-dashboard/prompts/wizard.md` — the prompt
  paragraph that mentions templates is rewritten to point at
  the folder as the only library surface, with an explicit "if
  the folder is empty, offer to run prepop init" instruction
  (folds finding #1's wizard prompt fix in by construction).

Defensive sub-tasks bundled in (findings #2 and #3):

- `wizard_loop.rs` no longer caches `self.last_draft_id` from a
  `create_strategy` response unless the response carries the
  freshly-persisted strategy id. On `create_strategy` failure,
  the wizard surfaces the engine error and does not chain
  `create_strategy_agent` against a phantom id.
- The save-gate at `crates/xvision-engine/src/agents/validate.rs:157-172,324`
  is **not** weakened. The hard 200-char + no-placeholder rule
  stays load-bearing for direct API / MCP / wallet-plan callers.
  The wizard simply stops feeding it a placeholder.

Content migration (templates registry → prepop seed entries):

- One markdown file per existing template under
  `crates/xvision-engine/src/strategies_folder/prepop/seeds/` (or
  the existing seed directory — worker picks the file location
  consistent with the merged `strategies-folder-prepopulation`
  contract, #419). Each seed includes the original template's
  `display_name`, `plain_summary`, the canonical agent prompts,
  and the mechanical-params shape it used to provide.
- Seeds are operator-readable (markdown front-matter with JSON-or-YAML
  bodies, not raw JSON) so the folder remains useful to a human.
- `xvn strategies init` (the prepop CLI surface that shipped in
  #419) seeds them on first run; an existing folder is never
  silently overwritten.

Acceptance:

- `cargo test -p xvision-engine` passes with the `template` field
  removed from `CreateStrategyReq`, `StrategyManifest`,
  `CreateStrategyOut` (if it carried one), and all downstream uses.
- `cargo test -p xvision-dashboard wizard_loop` passes; new test
  asserts `create_strategy` with no template + no prompt + no
  agent seed returns a `{ id }` whose downstream `set_agent` /
  `update_strategy` flow lands a passing save-gate after the
  operator fills in the prompt.
- New end-to-end wizard test: replay the operator's transcript
  ("Gemini flash lite 3.1 for agent, base it off fibonacci + RSI")
  against a freshly-seeded folder and assert the wizard
  references at least one seeded fibonacci entry rather than
  narrating "your folder is empty."
- Grep confirms zero references to `list_templates`,
  `template_registry`, `WIZARD_BLANK_TEMPLATE`, or
  `manifest.template` outside of (a) the deletion commits, (b)
  the prepop seed content itself, (c) the optional
  `#[serde(default, skip_serializing)]` shim on
  `StrategyManifest::template` (if kept for a single release).
- Existing on-disk strategy manifests with a `template` field
  still load (serde shim).
- The hard save-gate (`validate.rs:324`) still rejects a
  placeholder prompt when a direct API / MCP caller submits one.
- `bash scripts/board-lint.sh` is green.

Verification:

- `cargo test -p xvision-engine`
- `cargo test -p xvision-dashboard`
- `cargo clippy --workspace -- -D warnings`
- `bash scripts/board-lint.sh`
- Manual smoke: `xvn dash`, reproduce the operator's 2026-05-21
  session.

### `chat-messages-insert-failing` (P1, engine chat session)

Three sequential `anyhow::Context("insert chat_messages row")`
failures from `crates/xvision-engine/src/chat_session/store.rs:87-98`
on a single operator session.

Audit pass before writing the fix:

1. Capture the underlying SQLx error — `.context("insert chat_messages
   row")` swallows the original. Add a structured log of the wrapped
   error (`SQLITE_CONSTRAINT_UNIQUE`, `SQLITE_CONSTRAINT_FOREIGNKEY`,
   pool-timeout, etc.) so the next operator hit lands a real signal.
2. Schema check on `chat_messages`: `(session_id, seq)` unique?
   `session_id` FK to a `chat_sessions` row? Did the failed
   `create_strategy` cascade leave the session in a state where the
   seq counter is stuck or the session row was rolled back?
3. Reproduce with `cargo test -p xvision-engine chat_session::store`
   covering: insert with no parent session, insert with duplicate
   `(session_id, seq)`, insert after a sibling-transaction rollback.

Acceptance:

- The originating SQLx error code is captured in the log line, not
  hidden behind `.context()`.
- Tests reproduce the failing condition and the fix removes it.
- Manual repro of the operator's session no longer errors three
  times in a row.

### `wizard-folder-recall-honesty` (P2, dashboard wizard)

Wizard told the operator the folder was empty after the
`list_strategies_folder` and `list_strategy_ideas` tools "completed."
Two scenarios, both fixable in the wizard layer:

1. The folder *was* empty for that operator. Wizard should narrate
   honestly ("your folder has no entries yet") and offer to run
   `strategies_folder::prepop::init` — the wizard already imports
   the prepop module in its test surface
   (`wizard_loop.rs:2581`), so the offer is one tool-call away.
2. The folder had entries / ideas but the wizard's prompt collapsed
   them. Add a wizard regression test that seeds non-empty folder
   results into the mock tool driver and asserts the wizard's
   narrative references the returned entries.

Acceptance:

- Wizard never narrates "empty folder" when the tool returned a
  non-empty list.
- When the folder is genuinely empty and the operator asks for a
  named-pattern strategy (fibonacci+RSI in this case), wizard
  offers prepop init rather than jumping straight to a blank draft.

### `strategies-folder-into-view-toggle` (P2, frontend leaf)

Fold `/strategies-folder` (`frontend/web/src/routes.tsx:79`) into
`/strategies` (`:77` area) as a header-level segmented control:
`List | Folder`. URL convention: `/strategies?view=folder`. Keep
the old path as a 301-style redirect / alias so deep links don't
break.

Touch points:

- `frontend/web/src/routes.tsx:79` — remove or alias the route.
- `frontend/web/src/routes/strategies.tsx` — add the view toggle
  in the page header; mount the existing folder surface in the
  `view=folder` branch.
- `frontend/web/src/routes/strategies-folder.tsx` /
  `strategies-folder.test.tsx` — keep the component, change its
  mount point.

Acceptance:

- `/strategies-folder` continues to work (alias).
- `/strategies?view=folder` shows the folder view.
- Toggle preserves URL state; back/forward navigates between
  views without remounting the page shell.
- Existing tests in `strategies-folder.test.tsx` still pass when
  the component is mounted from inside `strategies.tsx`.

### `memory-into-agents-section` (P2, frontend leaf)

Fold `/memory` (`frontend/web/src/routes.tsx:95`) into the Agents
section. Operator's framing: the global Memory page becomes "memory
across all agents," surfaced from `/agents` either as a left-rail
item under the Agents section or as a tab alongside the agent
list. Per-agent memory continues to live at the existing
`components/agent/MemoryTab.tsx`.

Touch points:

- `frontend/web/src/routes.tsx:95` — remove `/memory` top-level,
  add `/agents/memory` (and consider an alias from `/memory`).
- `frontend/web/src/features/memory/MemoryPage.tsx` and
  `MemorySurface.tsx` — repoint as a nested view, no behavior
  change required.
- Sidebar nav — drop the top-level Memory entry; surface it
  inside the Agents section.

Acceptance:

- `/memory` continues to resolve (alias) so deep links survive.
- The Agents section visibly contains a Memory entry/tab.
- Per-agent `MemoryTab.tsx` is untouched.

## Wave shape recommendation

- **`templates-elimination`** is the wave's foundation. **Split
  2026-05-21 into two contracts** after the worker stopped on a
  scope mismatch (the original contract conflated the strategy
  `template_registry` at `crates/xvision-engine/src/templates/`
  with the distinct `AgentTemplate` agent-picker at
  `crates/xvision-engine/src/agents/templates.rs`, and missed
  that `manifest.template` is the load-bearing discriminator for
  `MechanicalParams::from_value` typed dispatch). The split:
  - **`templates-elimination`** (this contract, descoped to
    wizard-only) — removes `WIZARD_BLANK_TEMPLATE`, the wizard's
    `list_templates` tool, `WizardCreateStrategyInput::template`,
    `authoring::list_templates`; adds a blank-draft creation path
    in `authoring.rs` that the wizard exclusively uses; rewrites
    the wizard prompt to point at the folder only; lands the
    defensive fix for the chained-write. Scope confined to
    `authoring.rs`, `wizard_loop.rs`, and `prompts/wizard.md`.
  - **`strategy-template-registry-removal`** (follow-up; status
    `deferred` until parent merges) — deletes
    `crates/xvision-engine/src/templates/`, removes
    `manifest.template`, refactors `MechanicalParams::from_value`
    dispatch, migrates the 8 strategy starter shapes to operator-
    readable prepop seeds under `docs/strategies/templates/`,
    updates the `xvn strategy create --template` CLI surface and
    the MCP `create_strategy` tool schema.
  - **`agents/templates.rs` (AgentTemplate)** is preserved by both
    contracts — distinct concept (per-agent profile picker for
    `/agents/new`), not part of the "templates" the operator
    asked to eliminate.
- **`chat-messages-insert-failing`** is a parallel P1 track
  scoped to `crates/xvision-engine/src/chat_session/`. Start
  with an audit pass that surfaces the swallowed SQLx error
  before scoping the fix. Parallel-safe with everything else.
- **`wizard-folder-recall-honesty`** depends on
  `templates-elimination` (touches `prompts/wizard.md` and the
  empty-folder narrative). Lands after the foundation.
- **`strategies-folder-into-view-toggle`** and
  **`memory-into-agents-section`** are frontend-only P2 tracks
  scoped to `frontend/web/src/routes.tsx` and the targeted
  route files. Parallel-safe with everything else in the wave.
