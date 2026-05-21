---
track: strategy-ideas-tool-surface
lane: leaf
wave: v2f
worktree: .worktrees/strategy-ideas-tool-surface
branch: task/strategy-ideas-tool-surface
base: origin/main
status: merged
depends_on:
  - strategies-folder-surface
  - strategies-folder-prepopulation
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies_folder/ideas.rs     # NEW module — queries the pre-populated library
  - crates/xvision-engine/src/strategies_folder/mod.rs       # pub mod ideas
  - crates/xvision-engine/tests/strategies_folder_ideas.rs   # NEW test file
  - crates/xvision-dashboard/src/wizard_loop.rs              # register list_strategy_ideas tool on wizard dispatch
forbidden_paths:
  - crates/xvision-engine/src/strategies_folder/reader.rs    # track 1
  - crates/xvision-engine/src/strategies_folder/prepop.rs    # track 2
  - crates/xvision-engine/src/strategies_folder/types.rs     # track 1
  - crates/xvision-dashboard/prompts/wizard.md
  - crates/xvision-engine/src/agents/templates.rs
  - frontend/web/**
  - docs/strategies/**
interfaces_used:
  - crates/xvision-engine/src/strategies_folder::list, read
  - crates/xvision-engine/src/strategies_folder/types::FolderEntry, FileKind
parallel_safe: false                                         # wave-3, serial behind wave-2
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine strategies_folder
  - cargo test -p xvision-dashboard
  - bash scripts/board-lint.sh
acceptance:
  - **New module `strategies_folder/ideas.rs`** — `pub async fn list_ideas(ctx: &ApiContext, filter: IdeaFilter) -> ApiResult<Vec<IdeaSummary>>`. `IdeaFilter { category: Option<String>, indicator: Option<String>, limit: Option<u32> }`. Queries `library/templates/**/*.json` (the pre-populated content) and parses the `schema_version: xvision.strategy_template.v1` JSON shape.
  - **`IdeaSummary` struct** — `{ id: String, category: String, indicators: Vec<String>, name: String, summary: String, source_rel_path: String }`. `summary` is the JSON template's description or the first 280 chars of its rule text. ts-rs export.
  - **Filter semantics** — `category` matches the subfolder name (`ema`, `fibonacci`, `bollinger`, etc.); `indicator` matches an entry in the `indicators` field of the JSON template. Both case-insensitive. Limit default 20, cap 100.
  - **Missing library = empty result** — `list_ideas` on a host without `$XVN_HOME/strategies/library/templates/` returns `Ok(vec![])`. No panic. Wizard prompt should detect empty and suggest the user run `xvn strategies init`.
  - **One new wizard tool** registered in `wizard_loop.rs`: `list_strategy_ideas({ category?: string, indicator?: string, limit?: number }) → idea_summaries[]`. Description: "queries the user's pre-populated strategy idea library; use when the user asks for examples or ideas."
  - **Unit tests** — happy-path filter by category, filter by indicator, empty-library handling, malformed JSON skips that entry and continues (logs a warning), limit clamping.
  - **Wizard integration test** — feed the wizard a prompt like "give me three EMA strategy ideas" against a tempdir with the EMA folder pre-populated; assert the wizard calls `list_strategy_ideas({ category: "ema" })` and names at least three templates in its reply.
  - **No code changes** outside the listed allowed paths.

---

# Scope

Wave-3 V2F closer. Adds the `list_strategy_ideas` wizard tool that
queries the pre-populated `library/templates/**` for matching
strategy ideas by category or indicator. Returns summaries the
wizard can quote back to the user. Depends on tracks 1 + 2 — needs
the reader (1) and the populated content (2) to query.

Spec: `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`.

# Out of scope

- Creating templates from idea JSON (idea → starter strategy is a
  separate flow, deferred until usage shows demand).
- Ranking / scoring ideas.
- Cross-template recommendation ("if you like EMA, try Bollinger").
- Indexing for fast lookup — v1 scans the directory on each call;
  44 files is small enough.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/strategy-ideas-tool-surface status
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/strategy-ideas-tool-surface -b task/strategy-ideas-tool-surface origin/main
```

# Notes

Schema of `library/templates/**/*.json` is documented in
`docs/strategies/README.md` as `xvision.strategy_template.v1`.
Read a few existing templates before defining the parser shape;
inconsistent fields across categories are likely (44 hand-curated
files).

Malformed JSON should not crash the wizard. Log a warning and skip
that entry. If half the templates are malformed, that's a content
bug to fix in `docs/strategies/`, not a runtime failure mode for
this tool.
