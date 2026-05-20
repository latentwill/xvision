# Intake — 2026-05-20 — Strategies folder + template refactor (V2F seed)

> Operator ask 2026-05-20: a user-curated `strategies/` folder that
> agents can read while authoring new strategies, pre-seeded with the
> 44 existing strategy idea templates we already maintain. Companion
> work: expand the agent-pipeline template library and finish the
> template-optional refactor that started in
> `wizard-strategy-template-optional` (#275, merged 2026-05-18).
>
> Proposes a new V2 phase **V2F — strategy authoring & user
> knowledge**. Conductor's call on whether to adopt V2F as a phase
> label or fold these items into V2D-adjacent work.

## Source

1. Operator ask 2026-05-20: "Make an intake item for a strategies
   folder that the user can put their notes, docs, strategy files,
   evals in to help their agents create strategies. We can put our
   collected strategies in there. Part of template refactor which will
   expand the amount of templates available and also make templates
   not required for strategy creation."
2. Already shipped, sets the table: `wizard-strategy-template-optional`
   (PR #275, merged 2026-05-18, archived under
   `team/archive/2026-05-18-sweep-2/contracts/`). Templates are no
   longer required by `create_strategy_draft`; `create_blank_strategy`
   exists; the wizard prompt now describes templates as reference
   examples rather than prerequisites. This intake is the next-step
   work after that relaxation.
3. Existing collected strategies that should pre-seed the new folder:
   - `docs/strategies/README.md` — generator pointer.
   - `docs/strategies/templates/` — 44 JSON strategy idea templates
     across EMA / Fibonacci / Bollinger / Nansen / random / RSI+volume
     categories (`schema_version: xvision.strategy_template.v1`).
   - `docs/strategies/freqtrade_strategies_playlist.md` — annotated
     playlist of strategies to study.
   - The source-of-truth markdown backlog in `strategies/` (root) that
     `scripts/generate_strategy_template_files.py` regenerates from.

## Current state (what already ships)

Two distinct "template" surfaces in the tree today, which the user's
ask conflates:

1. **Agent-pipeline templates** (3 of them) —
   `crates/xvision-engine/src/agents/templates.rs::builtin_templates()`.
   These define *how slots compose* (single-trader, analyst-executor,
   risk-checked-trader). Surfaced at `/agents/new?template=…`.
2. **Strategy idea templates** (44 of them) —
   `docs/strategies/templates/**/*.json`. These describe *what a
   strategy does* (indicator rules, entry/exit conditions). Static
   files; not loaded at runtime today; consumed only by humans reading
   the docs tree or by future agents that learn to read JSON files.

Other surfaces touched by this intake:

- `crates/xvision-engine/src/authoring/` — `create_strategy_from_template`,
  `list_templates`. After the template-optional refactor,
  `create_strategy_from_template(None)` returns a valid empty draft.
- `crates/xvision-dashboard/src/wizard_loop.rs` — the wizard prompt
  + tool schema that's already template-optional.
- `crates/xvision-dashboard/prompts/wizard.md` — the wizard system
  prompt that explains templates as reference examples.
- **Nothing in the tree today is a "user knowledge" surface for
  strategies.** A user with a markdown file of ideas, a CSV of past
  trades, an Excel sheet of indicator backtests, or a PDF of a paper
  about regime detection has no place to drop those for agents to
  read.

## Raw items → tracks

| Raw item | Track | Lane | Notes |
|---|---|---|---|
| Strategies folder schema + read-only surface: per-user `~/.xvn/strategies/` (or `<workspace>/.xvn/strategies/`) tree, with subfolders `notes/`, `docs/`, `strategy-files/`, `evals/`, `library/`; an index file enumerates contents with type + brief summary; agents get a read-only `list_strategies_folder` / `read_strategies_file` tool pair | `strategies-folder-surface` | foundation | New crate `xvision-strategies-folder` or a module under `xvision-engine`. Read-only in v1 — agents can't write back. |
| Pre-population from `docs/strategies/`: copy / symlink the 44 strategy idea templates + the freqtrade playlist + the source markdown backlog into the user's `strategies/library/` subfolder on first init; runtime registry parses them into a queryable shape (per-category index, indicator filter, etc.) | `strategies-folder-prepopulation` | leaf | Depends on `strategies-folder-surface`. Includes an `xvn strategies init` CLI command + a wizard prompt to do this for new users. |
| Expanded agent-pipeline template library: add 4–8 new templates to `crates/xvision-engine/src/agents/templates.rs` (e.g. `momentum-trader-only`, `mean-rev-mean-and-trader`, `multi-asset-router-with-traders`, `regime-aware-trader`, `news-reader-plus-trader`, `dual-execution-paper-and-live-confirmed`). Each template ships with a one-paragraph blurb + a starter system prompt suggestion | `agent-pipeline-template-library-expansion` | leaf | Independent of strategies folder. Surfaces at `/agents/new` template picker. |
| Strategy idea templates lifted into the agent's runtime tool surface: a `list_strategy_ideas(filter)` tool the wizard can call to surface concrete examples from the strategies folder when an operator asks "give me ideas." Driven by the 44 JSON files (post-prepopulation) | `strategy-ideas-tool-surface` | leaf | Depends on `strategies-folder-prepopulation`. Reuses the wizard's existing tool dispatch in `wizard_loop.rs`. |
| Wizard prompt refresh: update `prompts/wizard.md` to (a) describe the strategies folder + when to consult it, (b) describe the expanded template library, (c) explicitly reinforce that templates and the strategies folder are *references for inspiration*, not prerequisites — closing the loop on the `wizard-strategy-template-optional` work | `wizard-prompt-strategy-folder-and-templates` | leaf | Single-file change; coordinate with other in-flight wizard prompt edits. |
| User import flow: `xvn strategies import <path>` and a dashboard drop-zone surface for users to add their own notes/docs/CSV/PDF files into the strategies folder. PDFs and CSVs get a minimal parse pass (extracted text into a sidecar `<name>.summary.md`) so the agent can read summarized content without us shipping a PDF parser into every agent call | `strategies-folder-import` | leaf | Depends on `strategies-folder-surface`. PDF parsing reuses `pdf` skill / `pdftotext` if available; falls back to a "skipped — install pdftotext" finding. |

## Out of this intake

- **Strategy idea templates as a marketplace listing.** A user-published
  strategy idea template (vs the seller-published Strategy NFT in V2C)
  is a different shape: it's a starting point for *making* a strategy,
  not a deployable Strategy artifact. Defer until V2C marketplace work
  determines whether the listing UI should also accept "starter
  templates" as a category.
- **Agent-writable strategies folder.** v1 is read-only — agents read,
  users write. Agent-write requires a permission model + audit trail
  (which strategy did this come from, why did the agent add it). Defer
  to V3 autoresearcher; it's the natural consumer of "agent learned
  this; remember it in the user's strategies folder."
- **Replacing `docs/strategies/templates/` as the source of truth.**
  This intake migrates the *content* into the strategies folder but
  doesn't drop the docs tree. The python generator
  (`scripts/generate_strategy_template_files.py`) and the source
  markdown backlog in `strategies/` remain in place; the strategies
  folder becomes a *consumer* of those files for now. Consolidation is
  a follow-up.
- **Strategy idea quality scoring / curation.** Some of the 44
  templates are stronger than others; ranking them is a separate
  research concern. Out of scope for v1 import + lift.
- **V2D cortex memory integration.** The strategies folder is
  user-curated knowledge; cortex memory is per-agent learned memory.
  They are distinct surfaces. When V2D lands, an agent that consults
  the strategies folder may *remember* what it found via cortex; the
  surfaces don't merge. Note for the V2D contract author.
- **Eval result import into the strategies folder.** Users dropping
  their own past eval results (so an agent can learn from them) is
  appealing but underspecified — is it just JSONL, or a curated
  summary doc? Defer until at least one user actually does this
  manually and we see the shape.

## Verification (when a track lands)

Each track contract's "Verification" section should require:

- **Unit tests** on the strategies folder reader (`list_strategies_folder`,
  `read_strategies_file`): one test per kind of file (markdown, JSON
  template, CSV, PDF summary), one test for "folder missing — return
  empty list", one test for permission-error handling.
- **Wizard integration test** for `list_strategy_ideas` and the new
  agent-pipeline templates: feeding the wizard a prompt like
  "summarize the available strategy ideas" produces a call to
  `list_strategy_ideas` and an answer that names at least three
  templates.
- **Pre-population idempotency:** running `xvn strategies init` twice
  in a row does not duplicate files or break user-edited copies.
  Existing user edits are preserved; a stale-template warning is
  shown via a `strategies_library_drift` finding.
- **`agent-pipeline-template-library-expansion`:** each new template
  loads via `builtin_templates()`, ts-rs exports cleanly, the
  `/agents/new` picker renders all of them, and at least one
  end-to-end strategy creation works using each new template.
- **`wizard-prompt-strategy-folder-and-templates`:** the system prompt
  no longer implies templates are required; explicit mention of the
  strategies folder + when to consult it; existing wizard tests
  continue to pass.
- **Type-check the dashboard:**
  `pnpm --dir frontend/web typecheck && pnpm --dir frontend/web test --run`.
- **Run `bash scripts/board-lint.sh`** before pushing the contract.

## Related artifacts

- `team/archive/2026-05-18-sweep-2/contracts/wizard-strategy-template-optional.md`
  — the relaxation work this intake builds on.
- `crates/xvision-engine/src/agents/templates.rs` — current 3 agent-
  pipeline templates; expanded by track 3.
- `crates/xvision-engine/src/authoring/` — `create_strategy_from_template`
  + `list_templates`; consumed by tracks 1, 4, 5.
- `crates/xvision-dashboard/src/wizard_loop.rs` — wizard tool dispatch;
  consumed by track 4.
- `crates/xvision-dashboard/prompts/wizard.md` — system prompt; primary
  edit site for track 5.
- `docs/strategies/templates/**/*.json` — 44 strategy idea templates;
  pre-population source for track 2.
- `docs/strategies/freqtrade_strategies_playlist.md` — annotated
  playlist; pre-population source for track 2.
- `scripts/generate_strategy_template_files.py` — generator for the
  44 templates from the markdown backlog in `strategies/`.
- `team/board-v2.md` — proposes V2F as a new phase for these tracks;
  conductor decides at decomposition.

## V2F notes — strategy authoring & user knowledge

If the conductor adopts V2F as a new phase label, the V2 board entry
would read approximately:

```
### V2F — strategy authoring & user knowledge

| # | Item | Source |
|---|---|---|
| 26 | Strategies folder (read-only) | This intake, track 1 |
| 27 | Strategies folder pre-population from docs/strategies/ + xvn strategies init | This intake, track 2 |
| 28 | Expanded agent-pipeline template library (4–8 new templates) | This intake, track 3 |
| 29 | Strategy ideas tool surface for the wizard (list_strategy_ideas) | This intake, track 4 |
| 30 | Wizard prompt refresh for strategies folder + expanded templates | This intake, track 5 |
| 31 | User import flow (xvn strategies import + dashboard drop-zone) | This intake, track 6 |
```

V2F is a small phase (six leaves, mostly independent). It can run in
parallel with V2E once V2A leaves merge; the surfaces don't overlap.

Alternative placement: fold this into V2D's intake as an additional
substrate item (V2D memory + V2F strategies folder are both "agent-
facing knowledge surfaces"). Conductor's call.

## Dependency graph (preview, intake-level)

```
strategies-folder-surface (#1, foundation)
    │
    ├─→ strategies-folder-prepopulation (#2) — populates the folder from docs/strategies/
    │       │
    │       └─→ strategy-ideas-tool-surface (#4) — wizard tool reads the folder
    │
    └─→ strategies-folder-import (#6) — user adds their own files

agent-pipeline-template-library-expansion (#3) — independent of folder work
wizard-prompt-strategy-folder-and-templates (#5) — small; can land last,
    references the folder + the expanded templates
```

## Open questions for the conductor

These resolve at decomposition, not in this intake:

1. **Strategies folder location.** `~/.xvn/strategies/` (per-user,
   user-home) vs `<workspace>/.xvn/strategies/` (per-project, in the
   xvision checkout) vs both with a config knob. Default proposal:
   `<workspace>/.xvn/strategies/` because the rest of xvn's per-user
   data already lives under `~/.xvn/runs/<run_id>/` and mixing user
   notes with run artifacts is awkward. Final call needs UX input.
2. **Pre-population: copy vs symlink.** Symlink preserves the
   `docs/strategies/templates/` source of truth (regenerated by the
   python script). Copy lets users edit without modifying tracked
   docs. Recommend copy with a `library/.from-docs.json` manifest
   tracking provenance + drift; users can opt into symlink with a flag.
3. **PDF/CSV parsing depth.** Track 6 proposes minimal "extract text
   into `<name>.summary.md`". An alternative is to ship a richer
   parsing pipeline (table extraction from PDFs, CSV column-type
   inference). v1 should stay minimal; richer parsing is a follow-up
   when usage shows what users actually want from their PDFs.
4. **Wizard tool surface for the strategies folder.** Should the
   wizard get separate tools (`list_strategies_folder`,
   `read_strategies_file`, `list_strategy_ideas`) or one umbrella tool
   (`strategies(action, args)`)? Default: separate tools — clearer
   tool schema, easier for the agent to compose. Decide at #1
   decomposition.

## Next deploy snapshot

`main` at intake time: `c5a3cf1` (carried over from the V2E intake;
deploy-clean). No code changes are part of this intake — every
artifact written today is process/docs only and does not move the
runtime image.
