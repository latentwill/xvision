---
track: strategies-folder-surface
lane: foundation
wave: v2f
worktree: .worktrees/strategies-folder-surface
branch: task/strategies-folder-surface
base: origin/main
status: merged
depends_on: []
blocks:
  - strategies-folder-prepopulation
  - strategy-ideas-tool-surface
  - strategies-folder-import
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies_folder/**          # NEW module
  - crates/xvision-engine/src/lib.rs                         # one-line pub mod add
  - crates/xvision-engine/tests/strategies_folder.rs         # NEW test file
  - crates/xvision-dashboard/src/wizard_loop.rs              # register 2 new tools on wizard dispatch
  - crates/xvision-dashboard/src/wizard_tools.rs             # if it exists; else create alongside
  - crates/xvision-engine/Cargo.toml                         # ts-rs feature wiring if needed
forbidden_paths:
  - crates/xvision-dashboard/prompts/wizard.md               # owned by wizard-prompt-strategy-folder-and-templates
  - crates/xvision-engine/src/agents/templates.rs            # owned by agent-pipeline-template-library-expansion
  - frontend/web/**                                          # no frontend in this track
  - docs/strategies/**                                       # read-only source
interfaces_used:
  - crates/xvision-engine/src/api/mod.rs::ApiContext.xvn_home
  - tokio::fs (async read)
parallel_safe: true
parallel_conflicts:
  - agent-pipeline-template-library-expansion                # both wave-1, but disjoint files
  - wizard-prompt-strategy-folder-and-templates              # both wave-1, but disjoint files
verification:
  - cargo test -p xvision-engine strategies_folder
  - cargo test -p xvision-dashboard
  - bash scripts/board-lint.sh
acceptance:
  - **New module `crates/xvision-engine/src/strategies_folder/`** — at minimum: `mod.rs`, `reader.rs`, `types.rs`. Public surface: `pub fn folder_root(xvn_home: &Path) -> PathBuf` (returns `<xvn_home>/strategies`), `pub async fn list(ctx: &ApiContext, subfolder: Option<&str>) -> ApiResult<Vec<FolderEntry>>`, `pub async fn read(ctx: &ApiContext, rel_path: &str) -> ApiResult<FileContent>`.
  - **`FolderEntry` struct** — `{ rel_path: String, kind: FileKind, size_bytes: u64, modified_at: String }` where `FileKind = Markdown | Json | Csv | Pdf | Text | Other`. Derives `Serialize`, `Deserialize`, ts-rs `TS` when `feature = "ts-export"` is on.
  - **`FileContent` struct** — `{ rel_path: String, kind: FileKind, content: String, truncated: bool }`. Body truncated at 256 KB with `truncated: true` flag set; agents asking for more must read in chunks (future contract; not v1).
  - **Path safety** — every `rel_path` resolved via `folder_root(...).join(rel_path).canonicalize()` then checked to ensure the canonical path is still under `folder_root`. Reject with `ApiError::Forbidden` if it escapes. No symlink traversal, no `..` escape. Unit tests cover both attacks.
  - **Missing folder = empty result, not error** — `list(ctx, None)` on a host that's never run `xvn strategies init` returns `Ok(vec![])`. No panic. `read` of a missing file returns `ApiError::NotFound`.
  - **Subfolder filtering** — `list(ctx, Some("notes"))` enumerates only `<root>/notes/**`. Allowed subfolder names locked to: `notes`, `docs`, `strategy-files`, `evals`, `library`. Anything else → `ApiError::BadRequest`.
  - **File-kind detection** — by extension (`.md` → Markdown, `.json` → Json, `.csv` → Csv, `.pdf` → Pdf, `.txt` → Text, else Other). Documented in the module header.
  - **Two new wizard tools** registered in `wizard_loop.rs` tool dispatch: `list_strategies_folder({ subfolder?: string }) → entries[]` and `read_strategies_file({ rel_path: string }) → FileContent`. Tool descriptions clarify: "reads from the user's strategies folder (read-only); use to consult their notes, library, or imported reference material when authoring strategies."
  - **Tool authorization** — both tools available to wizard runs only (not eval-runtime agent calls). The runtime agent surface stays narrow until V3 autooptimizer.
  - **Unit tests** — one per file kind (md / json / csv / pdf / txt), one missing-folder, one path-escape attempt, one symlink-escape attempt (skip on platforms without symlink support), one subfolder-allowlist rejection.
  - **Wizard integration test** — `crates/xvision-dashboard/tests/wizard_loop.rs` (or sibling) gets a new test that mounts a temp `xvn_home`, drops a markdown file under `notes/`, runs a wizard turn that asks "what notes do I have", asserts the wizard called `list_strategies_folder` and read the file.
  - **No code changes outside the listed allowed paths.**

---

# Scope

V2F foundation. Stand up the read-only strategies-folder surface
under `$XVN_HOME/strategies/`, register two wizard tools
(`list_strategies_folder` / `read_strategies_file`) on the wizard
dispatch, and ship the path-safety + missing-folder semantics every
downstream V2F track depends on.

Spec: `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`.

# Out of scope

- `xvn strategies init` (pre-pop) — track 2.
- `list_strategy_ideas` (idea query tool) — track 4.
- `xvn strategies import` — track 6.
- Wizard prompt edits — track 5.
- Agent-pipeline templates — track 3.
- Agent-runtime tool surface (eval-time tools). Wizard-only in v1.
- Writing to the folder. Read-only in v1.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/strategies-folder-surface status
git -C .worktrees/strategies-folder-surface log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/strategies-folder-surface -b task/strategies-folder-surface origin/main
```

# Notes

`ApiContext.xvn_home` is the only env input. Tests construct an
`ApiContext::new` with a `tempdir` (see existing patterns in
`crates/xvision-engine/tests/retention_janitor_spawn.rs`).

The wizard tool registration site is `wizard_loop.rs`; the existing
template tools (`create_blank_strategy`, etc.) are the pattern to
mirror.
