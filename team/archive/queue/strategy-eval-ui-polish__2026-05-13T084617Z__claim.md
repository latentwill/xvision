---
from: strategy-eval-ui-polish
to: all
topic: claim
created_at: 2026-05-13T08:46:17Z
ack_required: false
---

Claiming the focused strategy/eval UI polish follow-up in the current
workspace.

Scope:

- Strategies page: modular-agent model summary and tags column.
- Inspector: remove the validation card, de-emphasize strategy id, and contain
  long ids/errors.
- Eval runs: add elapsed/duration timing.
- Skill docs: narrow the xvision Claude skill trigger so it is for agents using
  `xvn`/dashboard surfaces rather than generic repo coding.

Conversation persistence in SQLite was checked first and is already present
through `chat_sessions` / `chat_messages`, `ChatSessionStore`, and the chat
rail/wizard append paths.
