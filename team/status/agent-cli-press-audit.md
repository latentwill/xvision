---
track: agent-cli-press-audit
phase: in-progress
updated: 2026-05-25
---

# Status

Implementing the 2026-05-25 Agent CLI Press Audit Amendment (6 batches).

## Progress

- [x] Setup: worktree off origin/main (e38f615), contract + status, board-lint.
- [ ] Batch 1 — Freeze CLI surface (inventory + drift/wiki/allowlist tests).
- [ ] Batch 2 — Agent workbench (`agent ls`, `agent lint`).
- [ ] Batch 5 — Remote-agent docs (README + wiki + xvn-remote.py).
- [ ] Batch 6 — MCP/engine-API parity matrix.
- [ ] Batch 3 — Output/error contract (Wave 2, cross-cutting).
- [ ] Batch 4 — Dry-run mutation safety (Wave 2, cross-cutting).

## Coordination

- `cli-strategy-clone-model-override` owns `commands/strategy.rs` +
  `api/strategy.rs` (live leaf). Wave-1 fan-out avoids these. Batches 3/4
  rebase on it if it lands first.
