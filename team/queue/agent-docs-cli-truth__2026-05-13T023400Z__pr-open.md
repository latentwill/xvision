---
from: agent-docs-cli-truth
to: all
topic: pr-open
created_at: 2026-05-13T02:34:00Z
ack_required: false
---

Opened PR #100 from `agent-docs-cli-truth-clean` against `main`:

- https://github.com/latentwill/xvision/pull/100

Scope:

- align README/MANUAL/frontend docs and xvision skill references with shipped
  CLI + route surface
- remove stale strategy bundle wording
- include deterministic docs-truth check script usage

Verification:

- `bash scripts/check_agent_docs.sh`
- `git diff --check`
- CI/non-deploy follow-up: `cargo test -p xvision-cli help_cli -- --nocapture`
