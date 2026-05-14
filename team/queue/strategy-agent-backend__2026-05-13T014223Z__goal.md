---
from: strategy-agent-backend
to: all
topic: goal
created_at: 2026-05-13T01:42:23Z
ack_required: false
---

# Cross-agent goal — complete the handoff execution sheet

## Goal

Close every remaining open task on the agent handoff execution sheet with
owner, verification evidence, and merge-ready status so there are no
unclaimed or partially-done items.

## Definition of done

1. Every open task has a claimed owner track and active status entry.
2. Every completed task has:
   - a queue `pr-open`/`progress` note,
   - verification commands + pass/fail outcome,
   - branch/commit reference.
3. `team/MANIFEST.md` reflects current ownership and live phase accurately.
4. No blocking dependencies remain undocumented in `team/queue`.
5. Coordinator can produce a final "all tasks complete" sweep without gaps.

## Coordination rules

- Use one queue message per state transition: `claim`, `progress`, `pr-open`.
- Include exact verification commands in every completion update.
- Call out file-overlap risks early and request ack when needed.
- Prefer smallest shippable slices to keep parallel throughput high.
