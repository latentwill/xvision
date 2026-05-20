# V2F — Strategies folder + template refactor

**Date:** 2026-05-21
**Source intake:** `team/intake/2026-05-20-strategies-folder-and-template-refactor.md`
**Phase:** V2F (new — strategy authoring & user knowledge)
**Status:** Decomposed; six tracks authored

## Goal

Give xvision users a place to drop their own notes, docs, strategy
files, and reference material so agents can read it while authoring
new strategies. Pre-seed that folder from the 44 strategy idea
templates already maintained under `docs/strategies/`. In parallel,
expand the agent-pipeline template library and finish the
template-optional refactor started in `wizard-strategy-template-
optional` (#275).

V2F is a small phase — six leaves, mostly independent — that runs
alongside V2E without surface overlap.

## What V2F is **not**

- Not a marketplace listing for strategy idea templates (V2C).
- Not agent-writable (write surface deferred to V3 autoresearcher,
  which is the natural producer of "agent learned this; remember it").
- Not a replacement for `docs/strategies/templates/` as source of
  truth. The python generator and the markdown backlog in
  `strategies/` remain; the strategies folder is a *consumer* this
  phase.
- Not a strategy idea quality scoring or curation system.
- Not eval-result import (deferred until a user manually drops one
  and we see the shape).
- Not V2D cortex memory. The strategies folder is user-curated
  knowledge; cortex is per-agent learned memory. Separate surfaces.

## Decisions (resolves the four open questions in the intake)

### 1. Location: `$XVN_HOME/strategies/` (per-user)

The intake floated `<workspace>/.xvn/strategies/` vs `~/.xvn/strategies/`.
Picking **per-user** under the existing `$XVN_HOME` root because:

- `$XVN_HOME` is already plumbed everywhere via `ApiContext.xvn_home`
  — `agent_runs/`, `config/`, `xvn.db` already live there. Adding a
  sibling `strategies/` is the smallest possible delta.
- The intake's "mixing notes with run artifacts" concern is resolved
  by namespacing: `strategies/` lives next to `agent_runs/`, not
  inside it.
- Per-workspace mode would require a new config knob and a separate
  resolver path. v1 keeps the single `$XVN_HOME` convention.
- If a per-workspace mode is asked for later, it's an additive
  `--workspace` flag on the surface tools; not blocking on v1.

### 2. Pre-population: copy with provenance manifest

The intake floated copy-vs-symlink. Picking **copy + manifest**:

- Copy into `$XVN_HOME/strategies/library/<provenance>/<file>`.
- Drop a `$XVN_HOME/strategies/library/.from-docs.json` manifest
  recording: source path under `docs/strategies/`, SHA-256, copy
  timestamp.
- On `xvn strategies init` re-run, compare manifest hashes; mismatch
  → emit a `strategies_library_drift` finding (user edited the
  copy → preserve it; new source revision → ask before
  overwriting).
- Symlink mode rejected for v1 because the docs tree is a regen
  output of `scripts/generate_strategy_template_files.py` — symlinks
  would make the user's library mutate every time the script runs.
  A future `--symlink` flag is fine; not v1.

### 3. PDF/CSV parsing: minimal text → sidecar `.summary.md`

- `xvn strategies import <path>` accepts `.md`, `.txt`, `.csv`, `.pdf`.
- `.md` / `.txt`: stored verbatim under the appropriate subfolder.
- `.csv`: header row + first 50 rows piped through to a sidecar
  `<name>.summary.md`. Sidecar lives next to the original.
- `.pdf`: text extracted via `pdftotext` (system binary) if available
  on PATH; if missing, emit a `summary_extractor_unavailable` finding
  and skip (the original PDF is still stored, just without a summary).
  Workspace `CLAUDE.md` already says to prefer `pdftotext` over
  in-process parsing.
- No table extraction, no chunking, no embedding. Minimal pass; richer
  parsing is a follow-up if usage shows demand.

### 4. Wizard tool surface: three separate tools

- `list_strategies_folder({ subfolder? }) → entries[]`
- `read_strategies_file({ path }) → { content, kind }`
- `list_strategy_ideas({ category?, indicator? }) → idea_summaries[]`

Separate tools beat one umbrella `strategies({action, args})`:

- The Claude SDK / Anthropic tool surface treats one tool = one
  cognitive unit. Three named tools are easier for the agent to
  compose than one polymorphic dispatcher.
- Easier to permission, lint, and unit-test independently.
- Tool schemas are simpler — no nested discriminated-union args.

## Folder layout

```
$XVN_HOME/strategies/
├── notes/              # user-authored free-form notes
├── docs/               # user-imported reference docs (md, pdf, txt)
├── strategy-files/     # user-authored strategy JSON / TOML
├── evals/              # user-imported eval result snapshots (future)
├── library/            # pre-populated content (read-mostly)
│   ├── .from-docs.json # provenance manifest
│   ├── templates/      # mirror of docs/strategies/templates/
│   │   ├── ema/
│   │   ├── fibonacci/
│   │   ├── bollinger/
│   │   ├── nansen/
│   │   ├── random/
│   │   └── rsi-volume/
│   └── reference/      # freqtrade playlist + source backlog
└── .meta/
    └── index.json      # cached listing (regenerated on demand)
```

Subfolders are created lazily on first write/init. A missing
subfolder is not an error; `list_strategies_folder` returns an empty
list and the wizard prompt notes the gap.

## Track decomposition

Six tracks. Three foundation-style decisions baked in (above) so
each track ships independently once its dependency lands.

| Track | Lane | Depends on | Scope |
|---|---|---|---|
| `strategies-folder-surface` | foundation | — | Read-only `$XVN_HOME/strategies/` reader crate + the two read tools (`list_strategies_folder`, `read_strategies_file`); registers them on the wizard tool dispatch. Unit tests cover every file kind + missing-folder. |
| `agent-pipeline-template-library-expansion` | leaf | — | Add 4–6 new templates to `crates/xvision-engine/src/agents/templates.rs` with one-paragraph blurbs + starter system prompts. Update test count. Independent of the strategies folder. |
| `wizard-prompt-strategy-folder-and-templates` | leaf | — | Refresh `crates/xvision-dashboard/prompts/wizard.md`: describe the strategies folder + new tool names, describe the expanded template library, explicitly state templates and the folder are *references*, not prerequisites. Closes the loop on #275. |
| `strategies-folder-prepopulation` | leaf | surface | `xvn strategies init` CLI verb + pre-population from `docs/strategies/templates/**` and `docs/strategies/freqtrade_strategies_playlist.md` into `$XVN_HOME/strategies/library/`. Manifest at `library/.from-docs.json`. Idempotent re-runs emit `strategies_library_drift` findings for user-modified copies. |
| `strategy-ideas-tool-surface` | leaf | surface + prepopulation | `list_strategy_ideas` wizard tool that queries the pre-populated library by category/indicator filter. Returns one-paragraph summaries from the JSON templates. |
| `strategies-folder-import` | leaf | surface | `xvn strategies import <path>` CLI verb + dashboard drop-zone surface (file picker — native browser primitive, allowed by the no-popups rule). PDF text extraction via `pdftotext` with graceful-missing-binary finding. CSV header + first-50-rows summary. |

### Dependency graph

```
strategies-folder-surface (foundation)
    │
    ├─→ strategies-folder-prepopulation
    │       │
    │       └─→ strategy-ideas-tool-surface
    │
    └─→ strategies-folder-import

agent-pipeline-template-library-expansion  (independent)
wizard-prompt-strategy-folder-and-templates  (independent — small)
```

### Parallel scheduling

- **Wave 1 (parallel):** `strategies-folder-surface`,
  `agent-pipeline-template-library-expansion`,
  `wizard-prompt-strategy-folder-and-templates`. Three different
  files / crates / directories — no conflict.
- **Wave 2 (after surface merges, parallel):**
  `strategies-folder-prepopulation`, `strategies-folder-import`.
- **Wave 3:** `strategy-ideas-tool-surface` (depends on prepop).

## Out of scope

- Marketplace listing for user-published idea templates (V2C).
- Agent-writable strategies folder (V3 autoresearcher).
- Symlink-mode pre-population (post-v1).
- Table extraction from PDFs / column-type inference from CSVs
  (post-v1, if usage demands).
- Eval result import (deferred — need real user shape first).
- V2D cortex memory integration (V2D author cross-links; this phase
  doesn't pre-design the merge).
- Replacement of `docs/strategies/templates/` as source of truth.

## Verification (per-track contract responsibility)

Each track contract enforces:

- Rust unit tests on every new public function.
- Wizard integration test for new tool surface
  (`list_strategy_ideas` round-trip with at least one filter).
- Pre-population idempotency test
  (`xvn strategies init` twice; no duplicates; drift finding when
  the user edited a copy).
- `cargo test -p xvision-engine -p xvision-cli -p xvision-dashboard`
  clean.
- `pnpm --dir frontend/web typecheck && pnpm --dir frontend/web test`
  clean (frontend touches in the import flow only).
- `bash scripts/board-lint.sh` passes before pushing contract edits.

## Related artifacts

- Current intake: `team/intake/2026-05-20-strategies-folder-and-template-refactor.md`
- Builds on: `team/archive/2026-05-18-sweep-2/contracts/wizard-strategy-template-optional.md` (#275)
- Source of truth (pre-pop input): `docs/strategies/templates/**/*.json`
- Wizard prompt: `crates/xvision-dashboard/prompts/wizard.md`
- Agent template registry: `crates/xvision-engine/src/agents/templates.rs`
- Authoring API: `crates/xvision-engine/src/authoring/`
- `$XVN_HOME` plumbing reference: `crates/xvision-engine/src/api/mod.rs::ApiContext`
