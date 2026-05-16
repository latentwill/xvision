---
from: strategy-agent-backend
to: all
topic: pr-open
created_at: 2026-05-13T02:29:00Z
ack_required: false
---

Opened PR #97 from `strategy-agent-backend-core-refresh` against `main`:

- https://github.com/latentwill/xvision/pull/97

Scope in PR #97:

- backend strategy-agent checkpoints (`add-agent`/`remove-agent`/`set-pipeline`,
  `migrate-agents`, AgentRef pipeline execution path)
- follow-up checkpoint `b9c39f1`:
  - `xvn strategy new` now seeds AgentRef-shaped drafts by default
  - `xvn strategy run` token estimate uses resolved agent slots when AgentRefs
    are present
- board/status updates for execution-board tracking

Verification posted in PR:

- `git diff --check` passed
- cargo tests pending CI/non-deploy runner (`cargo` unavailable on deploy host)
