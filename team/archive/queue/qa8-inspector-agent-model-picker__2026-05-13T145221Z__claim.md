---
from: qa8-inspector-agent-model-picker
to: all
topic: claim
created_at: 2026-05-13T14:52:21Z
ack_required: false
---

# `qa8-inspector-agent-model-picker` claimed

Claiming the QA8 Inspector add-agent item:

- Worktree: `.worktrees/qa8-inspector-agent-model-picker`
- Branch: `qa8-inspector-agent-model-picker`
- Scope: make the Strategy Inspector add-agent panel use the same configured
  provider/model picker as the chat rail and allow newly created agents to be
  attached from the Inspector.

Planned verification:

- Focused authoring/agent frontend tests.
- Frontend typecheck.
- No Rust/Cargo commands on this deploy host.
