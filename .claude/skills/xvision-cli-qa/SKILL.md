---
name: xvision-cli-qa
description: Use when QAing the xvision app through direct API or CLI calls, especially to verify Strategy, Scenario, and Eval create/edit/delete behavior, detect manifest drift, duplicate records, or invalid model resolution, and collect raw HTTP evidence without the browser UI.
---

# xvision CLI QA

## Overview

This skill covers xvision QA from the raw HTTP surface. Use it when the browser is unnecessary, flaky, or you need the contract-level truth behind the UI.

## When to Use

Use this when:
- verifying list/detail endpoints mirror the UI
- testing create, edit, delete, archive, attach, detach, validate, and launch flows
- checking scenario duplication or stale data at the API layer
- diagnosing eval failures caused by provider/model resolution
- comparing raw JSON responses before investigating UI behavior

Do **not** use this for visual layout checks, control alignment, or wizard screenshot review; use the chat-rail UI skill for that.

## Core Loop

1. **Discover the routes**
   - Hit list, detail, and mutation endpoints directly.
   - Confirm HTTP methods and allowed verbs with `OPTIONS` when needed.

2. **Create a disposable resource**
   - Prefer a temp scenario or temp eval run.
   - Use real payload shapes from existing objects.

3. **Mutate the smallest stable unit**
   - Strategy: attach/detach roles, validate, inspect manifest drift.
   - Scenario: create, delete, and confirm the list view updates.
   - Eval: create, poll, inspect failure reason, then delete.

4. **Verify the contract, not the story**
   - Compare request payloads, response bodies, and status codes.
   - If the API says success but the object is inconsistent, treat it as a bug.

5. **Clean up**
   - Delete temp scenarios and eval runs.
   - Record whether cleanup endpoints exist or are missing.

## Quick Checks

### Strategy
- `GET /api/strategy/:id`
- `POST /api/strategy/:id/agents`
- `PATCH /api/strategy/:id/agents/:role`
- `DELETE /api/strategy/:id/agents/:role`
- `POST /api/strategy/:id/validate`

Watch for:
- manifest fields disagreeing with slot prompts
- validation passing despite drift
- missing delete/archive for the strategy entity itself

### Scenario
- `POST /api/scenarios`
- `GET /api/scenarios`
- `GET /api/scenarios/:id`
- `DELETE /api/scenarios/:id`

Watch for:
- duplicate records returned by the list endpoint
- required fields that differ from the UI form’s apparent defaults
- cleanup confirming `404` after delete

### Eval
- `POST /api/eval/runs`
- `GET /api/eval/runs/:id`
- `DELETE /api/eval/runs/:id`

Watch for:
- model/provider resolution mismatches
- runs queuing successfully and then failing on the first decision
- invalid JSON from the trader slot

## Common Bugs

- strategy validates while prompts and manifest still disagree
- eval uses an upstream model ID that does not exist for the configured provider
- duplicate scenarios appear in dropdowns because the API already has duplicate rows
- strategy-level deletion is absent even though agent-role mutation exists

## Evidence to Capture

Always save:
- exact request payload
- response status code
- raw JSON body
- any error message from the run detail
- before/after list output for create/delete checks

## Reporting Format

- **Finding:**
- **Severity:**
- **Repro:**
- **Expected:**
- **Actual:**
- **Evidence:**
- **Likely cause:**
- **Recommendation:**

## References

See `references/xvision-api-quirks.md` for the concrete endpoint quirks, payload shapes, and failure messages observed during QA.
