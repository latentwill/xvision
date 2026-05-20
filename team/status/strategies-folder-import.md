# status — strategies-folder-import

V2F wave-2 leaf. CLI verb `xvn strategies import` + dashboard
`/strategies-folder` drop-zone + `POST /api/strategies-folder/import`.

## Summary of work

- **Engine module** `crates/xvision-engine/src/strategies_folder/`
  gains two new files alongside the track-1 reader:
  - `summary.rs` — `pdftotext`-based PDF extractor (missing-binary
    branch surfaces a soft finding) + CSV header+50-row markdown
    extractor.
  - `import.rs` — pure import logic for both the CLI and dashboard
    routes. Resolves the destination subfolder (extension defaults
    or `--to` override), enforces the type allowlist (`md`/`txt`/
    `csv`/`pdf`/`json`), the 25 MB size cap, and path safety
    (filename can't contain separators or `..`).
- **CLI** new `commands/strategies.rs` module wires the
  `xvn strategies import <path> [--to <subfolder>] [--no-clobber] [--json]`
  verb. Sits alongside the existing `xvn strategy` verb (which
  manages strategy-bundle authoring — different surface).
- **Dashboard** new `routes/strategies_folder.rs` module exposes:
  - `GET /api/strategies-folder/list?subfolder=<name>` — thin
    wrapper over `strategies_folder::list` so the SPA can render
    the folder grouped by subfolder.
  - `POST /api/strategies-folder/import` — `multipart/form-data`
    upload. Accepts `file`, optional `to`, optional `no_clobber`.
    Reuses `strategies_folder::import_bytes` so CLI + HTTP share
    one validator + extractor stack.
- **Frontend** new `routes/strategies-folder.tsx` route renders a
  flat grouped list + native `<input type="file" multiple>`
  picker. Inline status entries replace any modal — the no-popups
  rule is preserved (`role="dialog"` is explicitly negatively
  asserted in the route test). Added to `Sidebar` as "Folder".

## Multi-owner files

Per the contract, this track shares `strategies_folder/mod.rs` +
`commands/strategies.rs` with the sibling
`strategies-folder-prepopulation` track. This PR adds the
`pub mod import; pub mod summary;` lines and the `Import` action
on the CLI; the sibling will append `pub mod prepop;` and an
`Init` action. Conflict resolution at land time is a trivial
enum-merge.

## Path-safety review

Filenames flow through `sanitize_filename`, which rejects
directory separators, drive prefixes, parent traversal, and
non-utf8. The target directory is canonicalized after
`create_dir_all` and the `starts_with(canonical_root)` check
guards against any symlink that might have been pre-placed under
the strategies folder. Tests cover the traversal and separator
cases (`import_bytes_rejects_traversal_in_filename`,
`import_bytes_rejects_separator_in_filename`).

## Dependency bump

`crates/xvision-dashboard/Cargo.toml`: added `multipart` to the
`axum` feature set (axum 0.7 ships multipart but only under that
feature). No new top-level crate.

## Verification

- `cargo test -p xvision-engine --test strategies_folder --test
  strategies_folder_import` → 23 pass (track-1: 13, this track: 10).
- `cargo test -p xvision-engine --lib strategies_folder` → 6 pass
  (unit tests inside `import.rs` + `summary.rs`).
- `cargo test -p xvision-dashboard --test strategies_folder_routes`
  → 7 pass.
- `cargo test -p xvision-cli --lib strategies` → 2 pass.
- `pnpm --dir frontend/web typecheck` → clean.
- `pnpm --dir frontend/web test -- routes/strategies-folder --run`
  → 5 pass.
- `pnpm --dir frontend/web test -- routes.test
  routes-code-splitting --run` → 5 pass (no regression on the
  router registration tests).

board-lint failures present at HEAD on main are unrelated to this
track (overlap claims on `agents/**`, `eval/mod.rs`, etc.).

## PR

To be filed against `task/strategies-folder-import`.
