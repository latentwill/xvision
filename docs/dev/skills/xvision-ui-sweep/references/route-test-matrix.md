# xvision Route Test Matrix

Use this checklist for a full browser sweep.

## `/`

- confirm landing content loads
- verify primary nav buttons work
- check quick actions and any dashboard status cards
- inspect console for startup errors

## `/setup`

- verify the chat rail or setup wizard appears
- create a strategy draft through the guided flow
- compare the assistant response to the strategy inspector
- confirm prerequisites are surfaced when missing

## `/strategies`

- list all strategies
- open a strategy row and confirm the authoring page loads
- check attached agents and read-only metadata labels
- verify actions for edit/delete/archive if present

## `/authoring/:id`

- compare manifest fields to the setup/chat result
- verify slot prompts and attached agents
- confirm any validation banner matches the real backend state
- capture console/network output if the page disagrees with the wizard

## `/scenarios`

- confirm scenario rows match the API list
- check filters, archived toggle, and row actions
- verify duplicates do not appear

## `/scenarios/new`

- create a temporary scenario
- compare the new detail page to the submitted form
- verify all required fields are accepted and persisted

## `/scenarios/:id`

- compare detail values with the created form
- test archive/delete
- confirm the scenario disappears or updates correctly after mutation

## `/eval-runs`

- open the launcher modal
- verify strategy and scenario dropdowns are deduped
- ensure blocked state appears when prerequisites are missing
- test row delete if available

## `/eval-runs/:id`

- confirm status updates after queueing
- inspect error details on failed runs
- verify run-level delete or cleanup works

## `/settings`

- confirm provider and broker settings render correctly
- check that missing credentials and danger-zone warnings are visible
- verify save/apply actions only appear when appropriate

## Evidence Checklist

- screenshot saved
- console logs captured
- failed network responses captured
- exact route recorded
- repro steps reproducible after refresh

## Severity Guidance

- **High:** action succeeds in the UI but backend state is wrong or missing
- **Medium:** UI state is confusing, stale, duplicated, or inconsistent
- **Low:** labeling, copy, spacing, or minor navigation issue
