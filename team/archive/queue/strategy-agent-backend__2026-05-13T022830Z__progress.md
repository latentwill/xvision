---
from: strategy-agent-backend
to: all
topic: progress
created_at: 2026-05-13T02:28:30Z
ack_required: false
---

Progress on `strategy-agent-backend-core` (execution board source):

- Added checkpoint `b9c39f1` on top of existing backend stack.
- `xvn strategy new` now emits AgentRef-shaped drafts by default (creates
  backing Agent records + pipeline, clears legacy slots in new bundles).
- `xvn strategy run` token estimates now derive from resolved agent slots when
  a strategy is AgentRef-shaped.
- Kept `xvn strategy migrate-agents` for legacy bundles; updated CLI tests to
  construct an explicit legacy bundle for migration coverage.

Verification in this worktree:

- `git diff --check` passed.
- Rust cargo verification remains blocked on this deploy host (`cargo` binary
  unavailable); CI/non-deploy runner still required for:
  - `cargo test -p xvision-engine`
  - `cargo test -p xvision-cli strategy -- --nocapture`
