---
track: strategies-folder-surface
contract: team/contracts/strategies-folder-surface.md
status: ready-for-review
owner: claude-opus-4-7
claimed_at: 2026-05-21
worktree: .worktrees/strategies-folder-surface
branch: task/strategies-folder-surface
---

# Status

## 2026-05-21 — implementation complete

V2F foundation track. Stood up the read-only `strategies_folder` engine
surface and registered the two wizard tools (`list_strategies_folder`,
`read_strategies_file`) on the wizard dispatch.

## Plan summary

1. New module `crates/xvision-engine/src/strategies_folder/` (mod.rs +
   types.rs + reader.rs) re-exporting `folder_root`, `list`, `read`,
   `FolderEntry`, `FileContent`, `FileKind`, `SUBFOLDER_ALLOWLIST`,
   and `MAX_FILE_BYTES`.
2. `pub mod strategies_folder;` added to `crates/xvision-engine/src/lib.rs`.
3. Two new `ToolDefinition`s appended to `strategy_tool_defs()` in
   `crates/xvision-dashboard/src/wizard_loop.rs`, with matching match
   arms in `WizardLoop::run_tool` that delegate to
   `strategies_folder::{list, read}`. The tools are available on both
   `AgentProfile::StrategySetup` (wizard) and `Workspace` (workspace
   chat) — both surfaces are wizard runtimes; eval-time agents have a
   separate tool surface and are not affected.

## Implementation notes

- **Path safety** lives in `reader::resolve_under_root`. It rejects
  empty / absolute / `..`-traversal paths before canonicalize, and
  re-checks the canonical resolved path against the canonical root so
  a symlink pointing outside the folder is rejected. `list` also
  canonicalizes every entry during enumeration and skips entries whose
  canonical form escapes the root, so a planted symlink is not
  surfaced at all.
- **Missing folder** routes through `tokio::fs::try_exists` →
  `Ok(vec![])`. Subfolder filter on a missing root also returns empty
  (the allowlist check runs first, so an invalid subfolder is still
  rejected even when the root is missing).
- **File kind** is purely extension-driven (`FileKind::from_extension`).
  `.md` / `.json` / `.csv` / `.pdf` / `.txt` map to typed variants;
  everything else falls to `Other`. PDF/CSV summarization lives in
  wave-2 (`strategies-folder-import`).
- **Truncation** at 256 KB (`MAX_FILE_BYTES`). `FileContent` sets
  `truncated: true` so the wizard can detect partial reads. Bodies
  decode as UTF-8 with `from_utf8_lossy` so binary blobs (PDFs) still
  return — the agent can choose to ignore them or defer to a future
  summary extractor.
- **ts-rs derives** on `FileKind`, `FolderEntry`, `FileContent` are
  gated on `feature = "ts-export"` so the frontend types are generated
  automatically by the existing pipeline.

## Deviation from contract (single item)

The contract specifies `ApiError::BadRequest` and `ApiError::Forbidden`
variants for invalid subfolder names and path escapes. Neither variant
exists on `ApiError` today — the closest existing variant is
`ApiError::Validation` (which maps to HTTP 422 via
`DashboardError::Validation` in `crates/xvision-dashboard/src/error.rs`),
and that is the established convention throughout the engine for
caller-input errors. Rather than expand the ApiError enum (which would
ripple through every downstream test and route mapping), this track
emits `ApiError::Validation` with structured message prefixes:

- `subfolder_not_allowed: '<name>' is not in the strategies-folder allowlist (...)`
- `path_escape: '<rel_path>' contains '..'`
- `path_escape: '<rel_path>' is absolute or rooted`
- `path_escape: '<rel_path>' resolves outside the strategies folder`

This preserves the contract's semantic ("reject with classified
caller-error") and keeps the error surface uniform with the rest of
the engine. If a future track adds dedicated `BadRequest`/`Forbidden`
variants the message prefixes can be lifted into structured codes
without changing call-sites.

## Verification

Ran (with `CARGO_TARGET_DIR=$HOME/.cargo-target/xvision`):

- `cargo test -p xvision-engine --test strategies_folder` →
  **13 passed; 0 failed**
- `cargo test -p xvision-dashboard` →
  **20 test suites passed; 0 failed** (includes the two new wizard
  tests `wizard_lists_strategies_folder_notes` and
  `wizard_reads_strategies_file`)
- `cargo build -p xvision-engine -p xvision-dashboard` → clean

The full `cargo test -p xvision-engine` invocation exposes two
pre-existing test-file compile errors
(`tests/eval_runs_agents_agent_id.rs` missing `bar_history_limit`
field, `tests/eval_prompt_cache_and_rolling_window.rs` ambiguous
numeric type). Those failures are present on `origin/main` and are
outside this track's `allowed_paths`; surfaced for visibility but
not addressed here.

## Acceptance checklist

- [x] New module `strategies_folder/{mod.rs, reader.rs, types.rs}` with
      the public surface `folder_root`, `list`, `read`.
- [x] `FolderEntry { rel_path, kind, size_bytes, modified_at }`,
      `FileKind` enum, `FileContent { rel_path, kind, content, truncated }`
      with ts-rs derives gated on `feature = "ts-export"`.
- [x] Path safety: canonicalize + prefix check + reject symlinks /
      `..` / absolute paths.
- [x] Missing folder = empty list. Missing file = `ApiError::NotFound`.
- [x] Subfolder allowlist (`notes`, `docs`, `strategy-files`, `evals`,
      `library`); other names → `ApiError::Validation` (see deviation
      note above).
- [x] File-kind detection by extension (`.md`, `.json`, `.csv`, `.pdf`,
      `.txt`, else `Other`).
- [x] Body truncated at 256 KB with `truncated: true`.
- [x] Two wizard tools registered (`list_strategies_folder`,
      `read_strategies_file`) on the wizard-runtime dispatch.
- [x] Tests cover all file kinds, missing folder, path escape, symlink
      escape, allowlist rejection, truncation, nested recursion.
- [x] Wizard integration test asserts a `tool_use` for
      `list_strategies_folder` returns a `ToolResult` containing the
      dropped note.
- [x] No code changes outside the listed allowed paths.

## Allowed-paths audit

Files touched:

- `crates/xvision-engine/src/lib.rs` — single `pub mod strategies_folder;` line
- `crates/xvision-engine/src/strategies_folder/mod.rs` (new)
- `crates/xvision-engine/src/strategies_folder/reader.rs` (new)
- `crates/xvision-engine/src/strategies_folder/types.rs` (new)
- `crates/xvision-engine/tests/strategies_folder.rs` (new)
- `crates/xvision-dashboard/src/wizard_loop.rs` — added 2 tool defs +
  2 dispatch arms + 2 inline tests in the existing `mod tests {}` block

None of the forbidden paths were touched:

- `crates/xvision-dashboard/prompts/wizard.md` (track 5)
- `crates/xvision-engine/src/agents/templates.rs` (track 3)
- `frontend/web/**`
- `docs/strategies/**`
