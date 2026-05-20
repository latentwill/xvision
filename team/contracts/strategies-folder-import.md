---
track: strategies-folder-import
lane: leaf
wave: v2f
worktree: .worktrees/strategies-folder-import
branch: task/strategies-folder-import
base: origin/main
status: ready
depends_on:
  - strategies-folder-surface
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/strategies.rs              # add import sub-verb alongside init
  - crates/xvision-cli/src/commands/mod.rs
  - crates/xvision-cli/src/main.rs
  - crates/xvision-engine/src/strategies_folder/import.rs      # NEW module
  - crates/xvision-engine/src/strategies_folder/mod.rs         # pub mod import
  - crates/xvision-engine/src/strategies_folder/summary.rs     # NEW — text extractors (pdftotext, csv)
  - crates/xvision-engine/tests/strategies_folder_import.rs    # NEW test file
  - crates/xvision-dashboard/src/routes/strategies_folder.rs   # NEW POST upload route
  - crates/xvision-dashboard/src/routes/mod.rs                 # register route
  - frontend/web/src/routes/strategies-folder.tsx              # NEW route + drop-zone
  - frontend/web/src/api/strategies-folder.ts                  # NEW API client
  - frontend/web/src/router.tsx                                # register route (or wherever route map lives)
  - frontend/web/src/components/shell/Sidebar.tsx              # add nav link if appropriate
forbidden_paths:
  - crates/xvision-engine/src/strategies_folder/reader.rs      # track 1
  - crates/xvision-engine/src/strategies_folder/prepop.rs      # track 2
  - crates/xvision-engine/src/strategies_folder/ideas.rs       # track 4
  - crates/xvision-engine/src/strategies_folder/types.rs       # track 1
  - crates/xvision-dashboard/prompts/wizard.md
  - crates/xvision-engine/src/agents/templates.rs
interfaces_used:
  - crates/xvision-engine/src/strategies_folder::folder_root, list
  - crates/xvision-engine/src/strategies_folder/types::FileKind
  - std::process::Command (for pdftotext)
parallel_safe: false                                           # wave-2 sibling: prepop touches strategies_folder/mod.rs
parallel_conflicts:
  - strategies-folder-prepopulation                            # both touch strategies_folder/mod.rs
verification:
  - cargo test -p xvision-engine strategies_folder
  - cargo test -p xvision-cli strategies
  - cargo test -p xvision-dashboard
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- routes/strategies-folder --run
  - bash scripts/board-lint.sh
acceptance:
  - **New CLI verb `xvn strategies import <path> [--to <subfolder>]`** — copies `<path>` into `$XVN_HOME/strategies/<subfolder>/`. Default subfolder by extension: `.md` / `.txt` → `notes/`, `.pdf` → `docs/`, `.csv` → `docs/`, `.json` → `strategy-files/`. User can override with `--to docs|notes|strategy-files|evals` (allowlist enforced).
  - **Summary sidecar generation** — for `.pdf`, run `pdftotext` on the file and write `<name>.summary.md` next to it. If `pdftotext` is not on PATH, emit a `summary_extractor_unavailable` finding and skip the summary (original still imports). For `.csv`, write a sidecar with the header row + first 50 rows as a markdown table. For `.md` / `.txt` / `.json`, no sidecar.
  - **Idempotency** — re-importing the same file overwrites by default (most-recent-edit wins). `--no-clobber` flag skips if target exists. Document in `--help`.
  - **Dashboard route at `/strategies-folder`** — minimal page rendering: (a) a flat list of items under `$XVN_HOME/strategies/` grouped by subfolder, (b) a file picker via native `<input type="file" multiple>` for upload (native primitive — allowed by the no-popups rule), (c) on file select, POST to a new dashboard route that calls the same import logic.
  - **POST upload route** — `POST /api/strategies-folder/import` accepts `multipart/form-data`. Writes the file to the appropriate subfolder, runs the same summary extraction as the CLI verb. Returns the new entry's `FolderEntry`.
  - **Path safety** — every imported file's target rel_path is validated the same way `strategies-folder-surface`'s reader validates: must canonicalize under `folder_root`. Reject escapes with 400.
  - **Size limit** — 25 MB per file by default (configurable later). Reject larger uploads with 413.
  - **Type allowlist** — `.md`, `.txt`, `.csv`, `.pdf`, `.json` only. Anything else → 400 with explanation. (Future extensions add new types via this allowlist; not v1.)
  - **No popups** — the dashboard surface uses an inline file picker + inline status messages. No modal "import succeeded" dialogs.
  - **Tests** — Rust: happy-path md import, pdf import with `pdftotext` present (skipped with cfg-gate when missing), csv import, path-escape rejection, size-limit rejection, type-allowlist rejection. Frontend: drop-zone renders, file select calls the API, error states render.
  - **No code changes** outside the listed allowed paths.

---

# Scope

Wave-2 V2F track. Lets users add their own notes, docs, strategy
files, and reference material to `$XVN_HOME/strategies/` via two
surfaces: the `xvn strategies import` CLI verb and a dashboard
`/strategies-folder` route with a native file-picker drop-zone.
PDF and CSV imports get text-summary sidecars so the wizard can
quote summarized content.

Spec: `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`.

# Out of scope

- Rich PDF parsing (tables, figures, sections). Minimal `pdftotext`
  pass only.
- CSV column-type inference or schema validation.
- Dragging files onto the page (drag-and-drop is a follow-up; v1
  uses the native file picker for simplicity and safety).
- Versioning / undo on overwrites (post-v1).
- Encrypted-at-rest storage (post-v1; user is local).
- Reading from import-state in the wizard (track 4 covers idea
  surfacing; reading user-imported docs is via the generic
  `read_strategies_file` from track 1).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/strategies-folder-import status
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/strategies-folder-import -b task/strategies-folder-import origin/main
```

# Notes

`pdftotext` is the system binary that workspace `CLAUDE.md` says to
prefer. The CLI uses `std::process::Command` to invoke it. Missing
binary is a soft failure (finding) not a hard error — the import
still completes for the original file.

Drag-and-drop is intentionally deferred to keep the v1 surface
predictable across browsers and keep the no-popups rule clearly
satisfied. The native file picker is the workspace's preferred
primitive.

This track shares `strategies_folder/mod.rs` with track 2 (prepop)
— each only adds a `pub mod ...` line. Coordinate at PR time:
whoever lands second adds the missing line; mod-file conflicts are
trivial to resolve.
