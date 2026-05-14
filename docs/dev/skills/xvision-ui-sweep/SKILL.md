---
name: xvision-ui-sweep
description: Use when QAing the xvision web app end-to-end through the standard UI, including navigation, lists, detail pages, forms, modals, delete/archive actions, console/network evidence, and checking that UI state stays consistent with the backend.
---

# xvision UI Sweep

## Overview

Use this for a broad, browser-driven walkthrough of xvision. The goal is to confirm the app feels coherent across routes, not just that one feature works in isolation.

## When to Use

Use when you need to:
- sweep the full UI for regressions
- test navigation and page-level affordances
- verify create/edit/delete flows from buttons and forms
- check lists, inspectors, modals, filters, and tabs
- capture screenshots, console logs, and failed network responses
- compare visible UI state against API truth

Do **not** use this for deep API-contract debugging or chat-rail-only command testing; pair it with the CLI or chat-rail QA skills when those surfaces matter.

## Route Sweep Order

1. Home / dashboard
2. Setup / onboarding flows
3. Strategies list, detail, authoring
4. Scenarios list, new, detail
5. Eval runs list, launcher, run detail
6. Settings / providers / brokers / danger zone

## What to Check on Every Page

- page title and route are correct
- primary action buttons exist and are enabled/disabled appropriately
- forms show the right defaults
- list rows match the detail pages
- modal choices are deduped and sensible
- delete/archive actions behave consistently
- any warning banner is accurate and timely
- console stays clean unless a bug is being reproduced
- network responses match the UI state

## Common QA Patterns

### Lists
- Open the list view.
- Confirm row counts and labels.
- Refresh after create/delete.
- Verify filters/search are not hiding data unexpectedly.

### Detail / Inspector Views
- Open a newly created object.
- Compare visible fields to the original form or wizard answers.
- Check read-only labels versus editable fields.
- Confirm attached resources are shown.

### Forms and Modals
- Create a record.
- Re-open the object and compare all fields.
- Exercise cancel/back/close behavior.
- Check validation messages and disabled submit states.

### Delete / Archive
- Delete or archive a temporary record.
- Verify it disappears from the list.
- Confirm the detail page 404s or shows the expected empty state.

## Evidence to Capture

For each bug, record:
- route
- exact steps
- expected vs actual
- screenshot path
- console errors
- request/response details
- whether the issue reproduces after refresh

## High-Value Bugs to Watch For

- UI says a change succeeded, but detail view still shows old data
- duplicate items appear in dropdowns or list views
- modal options include stale or deleted records
- a prerequisite is hidden until the user reaches a dead end
- empty states and error banners are inconsistent
- buttons exist but do nothing
- detail pages and list pages disagree on the same object

## Quick Reporting Format

- **Finding:**
- **Severity:**
- **Route:**
- **Repro:**
- **Expected:**
- **Actual:**
- **Evidence:**
- **Recommendation:**

## Practical Rule

If the app has both a wizard/chat flow and a normal form flow, always test both and compare the outputs. The bug may live in the handoff between them, not in either surface alone.
