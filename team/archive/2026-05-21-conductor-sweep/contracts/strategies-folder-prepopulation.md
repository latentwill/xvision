---
track: strategies-folder-prepopulation
lane: leaf
wave: v2f
worktree: .worktrees/strategies-folder-prepopulation
branch: task/strategies-folder-prepopulation
base: origin/main
status: merged
depends_on:
  - strategies-folder-surface
blocks:
  - strategy-ideas-tool-surface
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/strategies.rs            # NEW (or strategies_init.rs)
  - crates/xvision-cli/src/commands/mod.rs                   # register sub-verb
  - crates/xvision-cli/src/main.rs                           # if subcommand wiring lives there
  - crates/xvision-engine/src/strategies_folder/prepop.rs    # NEW module — depends on track 1's reader
  - crates/xvision-engine/src/strategies_folder/mod.rs       # pub mod prepop
  - crates/xvision-engine/tests/strategies_folder_prepop.rs  # NEW test file
forbidden_paths:
  - crates/xvision-engine/src/strategies_folder/reader.rs    # owned by track 1
  - crates/xvision-engine/src/strategies_folder/types.rs     # owned by track 1
  - crates/xvision-dashboard/prompts/wizard.md
  - crates/xvision-engine/src/agents/templates.rs
  - frontend/web/**
  - docs/strategies/**                                       # READ ONLY — pre-pop source
interfaces_used:
  - crates/xvision-engine/src/strategies_folder/types::FolderEntry, FileKind
  - tokio::fs (async copy)
  - sha2::Sha256
parallel_safe: false                                          # wave-2 sibling: import
parallel_conflicts:
  - strategies-folder-import                                  # both touch strategies_folder/mod.rs
verification:
  - cargo test -p xvision-engine strategies_folder
  - cargo test -p xvision-cli strategies
  - bash scripts/board-lint.sh
acceptance:
  - **New CLI verb `xvn strategies init [--force]`** — creates `$XVN_HOME/strategies/` and all five subfolders (`notes`, `docs`, `strategy-files`, `evals`, `library`) if missing. Then copies content from `docs/strategies/templates/**` and `docs/strategies/freqtrade_strategies_playlist.md` into `library/`.
  - **Provenance manifest** at `$XVN_HOME/strategies/library/.from-docs.json`. Schema: `{ "version": 1, "entries": [{ "rel_path": "library/templates/ema/...", "source": "docs/strategies/templates/ema/...", "sha256": "...", "copied_at": "<ISO8601>" }] }`.
  - **Idempotent re-runs** — `xvn strategies init` a second time without `--force` does NOT overwrite user-modified copies. For each manifest entry: rehash the current file at `rel_path`; if it matches the manifest's `sha256`, refresh from source. If it diverges from the manifest (user edited the copy), preserve the user's copy and emit a `strategies_library_drift` finding to stderr listing the divergent files. With `--force`, overwrite regardless.
  - **New source files** — when `docs/strategies/` contains a file not in the manifest, copy it in and append the manifest entry.
  - **Stale source files** — when the manifest names a file no longer in `docs/strategies/`, leave the copy in place and emit a `strategies_library_stale_source` finding. Don't auto-delete; the user may want the snapshot.
  - **Idempotency test** — `xvn strategies init` twice in a row produces identical filesystem state (modulo `copied_at` timestamps, which are stable for unchanged entries).
  - **Drift test** — modify a library file after init; rerun without `--force`; assert (a) the modified file is preserved, (b) the drift finding is emitted with the rel_path of the modified file.
  - **CLI smoke test** — `cargo test -p xvision-cli` covers the verb at minimum at the `--help` parse level + one happy-path init against a tempdir.
  - **No frontend code touched** — `frontend/web/**` is forbidden. The dashboard drop-zone surface for the import flow is track 6.
  - **No code changes** outside the listed allowed paths.

---

# Scope

Wave-2 V2F track. Ships the `xvn strategies init` CLI verb and the
prepopulation module that copies `docs/strategies/templates/**` and
the freqtrade playlist into `$XVN_HOME/strategies/library/` with a
provenance manifest. Idempotent re-runs preserve user edits and
surface drift findings.

Spec: `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`.

# Out of scope

- The folder reader / wizard tools (track 1).
- `list_strategy_ideas` tool (track 4 — depends on this track's
  prepopulation landing).
- User-driven import (track 6).
- Symlink mode (deferred per plan Decision 2).
- Regenerating `docs/strategies/templates/**` from the markdown
  backlog (that's `scripts/generate_strategy_template_files.py`'s
  job; this track copies the existing output).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/strategies-folder-prepopulation status
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/strategies-folder-prepopulation -b task/strategies-folder-prepopulation origin/main
```

# Notes

The `docs/strategies/` path is relative to the workspace root, which
the CLI can resolve via the binary's executing directory or via
`CARGO_MANIFEST_DIR` plus relative traversal. For runtime use,
embed the source files via `include_dir!` or similar so the CLI
works even when the docs tree isn't on disk (deployed image case).
Choice at author discretion; document it.

Findings are emitted to stderr in the standard xvn finding format
(`KIND: message`). The wizard / dashboard does not need to parse
them — they're for CLI users running `xvn strategies init` manually.
