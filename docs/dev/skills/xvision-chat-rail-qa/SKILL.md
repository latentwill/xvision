---
name: xvision-chat-rail-qa
description: Use when QAing the xvision app's chat rail and verifying Strategy, Scenario, and Eval flows stay in sync with the UI, especially when commands create/edit/delete resources or the chat output disagrees with the inspector.
---

# xvision Chat Rail QA

## Overview

This skill is a fast QA loop for xvision's chat rail. The goal is to verify that chat-driven actions actually persist, the UI reflects them, and the app surfaces the right prerequisites and errors.

## Use When

Use this for:
- chat rail commands that create, edit, delete, or launch Strategy / Scenario / Eval objects
- checking that wizard answers match the inspector/detail view
- finding UI vs backend mismatches
- capturing evidence for code agents

Do **not** use this for full app redesign or broad unrelated feature sweeps; start with the chat rail surface and expand only after the core path is green.

## Core QA Loop

1. **Map the surface**
   - Open `/`, `/setup`, `/strategies`, `/scenarios`, `/eval-runs`, and the relevant detail pages.
   - Note visible actions, read-only labels, and missing affordances.

2. **Drive one chat-rail mutation at a time**
   - Strategy: create via wizard, then verify list + authoring page.
   - Scenario: create via UI or chat, then verify list + detail page.
   - Eval: launch only after prerequisites are satisfied.

3. **Verify the UI matches the chat result**
   - Compare chat transcript, list row, detail/inspector fields, and any status badges.
   - If the chat claims a field changed but the inspector does not, treat as a bug.

4. **Capture evidence**
   - Record console errors, failed responses, screenshots, and any visible warnings.
   - Save the exact user-facing quote that proves the mismatch.

5. **Investigate root cause only after repro is stable**
   - Check network/API responses first.
   - Then inspect code or bundle behavior for the likely source of divergence.

## Recommended Test Order

### Strategy
- Create a draft from chat rail.
- Confirm the strategy appears in `/strategies`.
- Open the authoring page and verify:
  - name
  - asset universe
  - cadence
  - risk basis
  - attached agents / provider-model readiness
- Try edit and delete if exposed.

### Scenario
- Create a scenario.
- Confirm it appears in `/scenarios`.
- Open the detail page and verify the asset, window, granularity, and notes/tags.
- Test clone/edit, archive, and delete.

### Eval
- Open the eval launcher.
- Verify the correct strategy and scenario are selectable.
- Confirm the UI blocks runs when prerequisites are missing.
- Start the eval only when the strategy has an agent/model attached.
- Verify the run appears in `/eval-runs` and that status/details update.
- Test delete if available.

## What to Record

For each issue, capture:
- page and route
- exact steps
- chat rail prompt and assistant response
- expected vs actual
- severity
- screenshot or console/network evidence
- probable root cause if known

## Common Bugs to Look For

- chat rail says a field changed, but the inspector still shows old values
- duplicate rows or duplicate dropdown options
- create succeeds, but list view does not refresh
- detail page reflects a new object, but launcher selectors do not
- eval blocked too late, after the user already invested time
- read-only metadata is presented as if it were editable
- assistant output implies capability the toolset does not actually have

## Quick Checklist

- [ ] chat rail command sent
- [ ] assistant response captured
- [ ] UI updated in the right place
- [ ] backend response checked
- [ ] console/network errors checked
- [ ] screenshot saved
- [ ] repro is repeatable
- [ ] severity assigned
- [ ] likely root cause noted

## CLI / API-only QA

Use this mode when you need to verify the app without any browser UI at all.

### What to check
- list/detail endpoints reflect the same entities the UI shows
- create/edit/delete routes return the correct HTTP semantics (`201`, `200`, `204`, `404`, `405`, `422`)
- strategy validation catches manifest-vs-slot drift instead of only checking syntax
- eval scheduling uses the resolved provider/model actually configured for the strategy
- duplicate records are not present in list endpoints

### Especially important xvision pitfalls
- A strategy can validate successfully while still being internally inconsistent.
- A strategy may attach an agent successfully, but eval can still fail if model resolution points at an invalid upstream ID.
- Duplicate scenario rows in selectors often originate from duplicate records already present in the API list response.
- If the API exposes agent mutation but not strategy entity deletion, document that as a lifecycle gap rather than assuming a hidden route exists.

### CLI evidence style
- Capture raw JSON responses for create, mutate, and delete calls.
- Note exact status codes and error payloads.
- Prefer direct endpoint checks over UI inference when diagnosing contract bugs.

## Working Note

If the browser automation stack is flaky, prefer the most reliable available browser path. If browser access is unnecessary, the CLI/API surface is often the cleaner source of truth for xvision QA.

## References

- `references/xvision-api-quirks.md` — CLI/API findings, contract quirks, and repro snippets from the May 14, 2026 session.

## Reporting Format

Use concise bullets:
- **Finding:**
- **Severity:**
- **Repro:**
- **Expected:**
- **Actual:**
- **Evidence:**
- **Likely cause:**
- **Recommendation:**

## Keep It Tight

This skill is intentionally lightweight. Add new subsections only when the xvision chat rail grows new resource types or the QA surface expands beyond Strategy, Scenario, and Eval.
