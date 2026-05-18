---
track: sample-rich-acceptance
lane: leaf
wave: sample-wave-edge
worktree: .worktrees/sample-rich-acceptance
branch: task/sample-rich-acceptance
base: origin/main
status: ready
depends_on: []
blocks: []
allowed_paths:
  - sample/path.rs
forbidden_paths: []
verification:
  - cargo test -p some-crate -- -D warnings
acceptance:
  - The `create_strategy_draft` tool no longer lists
    `template` in `required`. Passing `template: null` or omitting
    it produces a blank draft.
  - Multi-paragraph entry with: colons, "quoted strings", and
    backticked `field.path` tokens that would otherwise confuse
    strict YAML parsers.
  - Bullet entries can include — em dashes — and  "double quotes".
---

# Scope

This fixture mimics the real-world acceptance shape that broke js-yaml.
The targeted scalar extractor must still surface track/lane/wave/etc.
