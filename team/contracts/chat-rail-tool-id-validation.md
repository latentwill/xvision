---
track: chat-rail-tool-id-validation
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/chat-rail-tool-id-validation
branch: task/chat-rail-tool-id-validation
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/wizard_loop.rs
  - crates/xvision-dashboard/tests/wizard_loop.rs
  - crates/xvision-dashboard/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
  - crates/xvision-engine/**
interfaces_used:
  - xvision-dashboard::wizard_loop::dispatch_tool (or the existing tool dispatch site)
  - the retry-budget guard added by `chat-rail-validate-retry-budget` (qa-round-5 F-3, merged 2026-05-19 in PR #316) — reuse, do not duplicate
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-dashboard -- -D warnings
  - cargo test -p xvision-dashboard wizard_loop
acceptance:
  - Before invoking `get_cli_job` or `get_cli_job_output`, the dispatch path validates the shape of the `job_id` argument against the known id grammars. Accepted shapes:
    * A bare ULID (26 chars, Crockford base32).
    * Any string the existing CLI job store treats as a valid prefix (look up the canonical validator if one exists; otherwise the bare-ULID rule is sufficient).
  - If the `job_id` doesn't match, the tool call short-circuits with a typed `InvalidJobId { provided, reason }` result returned via the existing tool_result path. The chat-rail UI surfaces this as the verbatim error message (same surfacing as `validate_draft` errors, per qa-round-5 F-2).
  - The retry-budget guard added in PR #316 (`chat-rail-validate-retry-budget`) is **extended** to also cover `get_cli_job` and `get_cli_job_output`. After 2 same-error retries for any of those tools the wizard loop force-ends with a stuck card, same UX as `validate_draft`. Reuse the existing data structure; do not duplicate it. If extending requires a small refactor to the guard's keying (per-tool error class), do that refactor.
  - Tests:
    * Unit test: dispatching `get_cli_job(job_id="eval_run_XKI6IWGw5aFZXsqkW3a3")` returns `InvalidJobId` without hitting the store. (This is the exact bad id pattern observed in chat_session `01KRXXHPRBKYKVEM2Q1VBS2YJ4`.)
    * Unit test: dispatching `get_cli_job` with a valid ULID still reaches the store.
    * Integration test: simulate two same-error retries against `get_cli_job_output` with a bad id; assert the loop force-ends after the 2nd and surfaces the stuck card.
  - No migration; no frontend changes — the chat-rail UI surface for tool errors is already in place from qa-round-5 F-2.
---

# Scope

Intake F-10 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.
A chat session (`01KRXXHPRBKYKVEM2Q1VBS2YJ4`) repeatedly invoked
`get_cli_job(job_id="eval_run_XKI6IWGw5aFZXsqkW3a3")` and got `cli job
'eval_run_…' not found`, then retried with the same bad id. Same
anti-pattern as `validate_draft` (qa-round-5 F-3, merged) — just a
different tool. Reuse the retry-budget guard; add id-shape validation
before dispatch.

# Out of scope

- Anything outside `wizard_loop.rs` and its tests.
- Engine-side changes to the CLI job store or its id grammar.
- Re-implementing the retry-budget guard — reuse the merged one.
- Frontend changes — the surfacing path was added by qa-round-5 F-2.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/chat-rail-tool-id-validation status
git -C .worktrees/chat-rail-tool-id-validation log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/chat-rail-tool-id-validation -b task/chat-rail-tool-id-validation origin/main
```

# Notes

Append checkpoints below. Do not edit the frontmatter above the line
without a contract-update PR.
