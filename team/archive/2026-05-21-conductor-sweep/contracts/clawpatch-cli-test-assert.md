---
track: clawpatch-cli-test-assert
lane: leaf
wave: clawpatch-blockers-2026-05-21
worktree: .worktrees/clawpatch-cli-test-assert
branch: task/clawpatch-cli-test-assert
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-cli/tests/eval_export_cli.rs                    # B-5 — add one assertion
forbidden_paths:
  - crates/xvision-cli/src/**                                      # no production code change for B-5
  - crates/xvision-cli/tests/**                                    # except the one file in allowed_paths
interfaces_used:
  - assert_cmd::Command                                            # whatever the test already imports
verification:
  - cargo test -p xvision-cli eval_export_cli
acceptance:
  - **B-5 closed.** `crates/xvision-cli/tests/eval_export_cli.rs` gains a `cli_out.stdout.is_empty()` assertion before validating stderr, per clawpatch's recommendation: `assert!(cli_out.stdout.is_empty(), "stdout: {}", String::from_utf8_lossy(&cli_out.stdout));`.
  - **Worker confirms via revalidate.** Run `clawpatch revalidate --finding fnd_sig-feat-test-suite-bb1a90129a-9_8b36947666` and confirm closed.
  - **No production code change.** This is a test-coverage gap, not a behavior bug.

---

# Scope

Single-finding track (B-5 of `team/intake/2026-05-19-clawpatch-blockers.md`).
Clawpatch's autonomous loop kept failing to land this one assertion;
the manual pass is trivial.

# Out of scope

- Other CLI tests.
- Production CLI behavior.
- Other clawpatch findings (handled in sibling tracks).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/clawpatch-cli-test-assert status
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/clawpatch-cli-test-assert -b task/clawpatch-cli-test-assert origin/main
```

# Notes

The smallest unit of work in the wave. Could be picked up by a worker
needing a 15-minute task between larger contracts.
