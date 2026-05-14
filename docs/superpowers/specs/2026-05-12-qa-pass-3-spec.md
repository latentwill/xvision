# QA Pass 3 — Spec

**Date:** 2026-05-12  
**Surfaces:** Chat rail, setup wizard, strategy inspector, settings, strategies page  
**Status:** Draft for user review

## Goal

Fix the QA Pass 3 issues by aligning each surface to a clear product role:

- The **setup wizard** is strategy-only.
- The **chat rail** is the master control plane for `xvn`.
- The **strategy inspector** edits real agents, not deprecated fixed slots.
- The **settings** and **strategies** pages should stop showing dead or misleading UI.

The main product failure in this pass is not a missing button. It is role confusion:

- the wizard claims broader command scope than it should have,
- the chat rail hides important `xvn` actions instead of surfacing them inline,
- the strategy inspector still behaves like the old fixed-slot model,
- and the surrounding pages still show controls that are either non-functional or conceptually obsolete.

This spec defines the target state and the sequencing for implementation. It does not prescribe code details beyond the product shape needed to make the UI coherent.

## Product Rules

These are the non-negotiables for this pass:

1. The setup wizard is for **strategy authoring only**.
2. The chat rail is the **master control surface** for `xvn`.
3. Tool calls must appear **inline when they happen**, not buried in a footer or summary strip.
4. Strategy editing must use **real agent slots** with provider/model selection, not a fake `model requirement` field.
5. Anything that is visibly non-functional must either be fixed or removed.

## Recommended Sequencing

| Batch | Surface | Scope |
|---|---|---|
| 1 | Chat rail | Master control, inline tool events, history drawer, scroll behavior, markdown |
| 2 | Setup wizard | Strategy-only scope, markdown, explicit handoff boundaries |
| 3 | Strategy inspector | Replace fixed slots with renameable agents, real validation, real provider/model fields |
| 4 | Settings + strategies | Remove dead onboarding chrome, fix template flow, repair filters/status controls |

## Batch 1 — Chat Rail

### Goal

Turn the chat rail into the operational control plane for `xvn`.

### Intended behavior

- Every `xvn` tool call appears as an explicit event in the transcript at the moment it happens.
- The event should show:
  - tool name
  - arguments
  - result or error
- The assistant narrative can remain, but tool calls are the primary audit record.
- The rail should support browsing prior conversation sessions from a visible history drawer in the sidebar.
- The rail should keep its own thread area scrollable so the layout does not grow forever.
- Markdown must render correctly in the chat transcript.

### Scope clarification

“All `xvn` commands” means a curated command registry exposed through the rail, not arbitrary shell access. The rail may surface commands by category, but every exposed command should be explicit and intentional.

### Acceptance criteria

- Tool calls are visible inline and in order.
- Tool calls are not only summarized at the bottom of the assistant message.
- Session history is browsable from the rail.
- The thread area scrolls internally and does not push the page layout off-screen.
- Assistant markdown renders as markdown.

## Batch 2 — Setup Wizard

### Goal

Keep the wizard narrowly focused on building strategies.

### Intended behavior

- The wizard only offers strategy-authoring actions.
- It should not imply that it can perform general operator control.
- It should not claim eval execution unless that path is explicitly available in the product.
- Tool calls in the wizard should also be explicit inline events.
- Markdown must render correctly here as well.

### Product boundaries

The wizard should be the guided authoring assistant, not the command console. If a user wants operational control, that belongs in the chat rail.

### Acceptance criteria

- Wizard copy and behavior clearly describe strategy-building only.
- The wizard does not advertise unsupported actions.
- Markdown renders correctly.
- Tool calls are visible when they occur.

## Batch 3 — Strategy Inspector

### Goal

Replace the deprecated fixed-slot inspector with a real agent-editing surface.

### Intended behavior

- Remove the separate `Regime` and `Intern` slot sections.
- Add an `Add Agent` action that creates a renameable agent slot.
- Each agent slot should expose the real v1 fields:
  - slot name
  - provider
  - model
  - system prompt
  - skills
  - max tokens
- The model requirement field should not remain as a fake constraint string in the UI.
- Validation must visibly do something in the inspector.
- Provider and model should be editable per agent slot.

### Product intent

The inspector should read like agent composition, not like a workaround for old slot terminology. It should match the backend direction already present in the repo.

### Acceptance criteria

- Users can add and rename agent slots.
- Users can choose provider/model per agent slot.
- The old fixed `Regime` and `Intern` sections are gone.
- Validation produces a visible result and is not a no-op.
- The fake model requirement field is removed from the user-facing model.

## Batch 4 — Settings and Strategies

### Goal

Clean up onboarding and template flow so the product stops showing dead or misleading controls.

### Intended behavior

- The “Add LLM key” prompt in the sidebar should only appear in the true empty state.
- Once the workspace has usable providers, the sidebar prompt should disappear.
- Provider tests and Alpaca connectivity checks should remain functional.
- Any filter or status control that does not work should be removed or made real.
- “New from template” should actually function.
- Template browsing should be discoverable and usable.
- Unclear template dropdown behavior should be removed if it is only adding confusion.

### Strategies-page intent

The strategies page should support direct creation and template discovery without forcing the user into a template-first mental model. Templates should be available as reference material, but not as a confusing gate.

### Acceptance criteria

- Sidebar onboarding chrome disappears after the workspace is configured.
- Provider and broker connection tests still work.
- Strategies can be created from templates successfully.
- Template browsing is accessible and understandable.
- Non-functional filter/status controls are not left in place.

## Cross-cutting behavior

### Markdown

Markdown rendering must work anywhere the assistant speaks in plain prose. That includes:

- setup wizard
- chat rail

### Explicit tool logging

Any `xvn` action that happens through the wizard or chat rail must be logged visibly in the transcript. The user should not have to infer that something happened from a late summary chip.

### Tool scope

The chat rail is the broad control plane. The wizard is not.

This split is intentional:

- wizard = strategy-only guidance and authoring
- rail = operational control and all `xvn` verbs that the product exposes

## Out of scope

This pass does not redesign the underlying `xvn` CLI itself.

It also does not attempt to expose arbitrary shell execution through the chat rail. The control plane should be an explicit tool registry, not a terminal emulator hidden behind chat.

## Risks

1. The chat rail may accumulate too many controls if the tool registry is not grouped well.
2. The inspector rewrite can become confusing if the old fixed-slot vocabulary is left partially visible.
3. If the wizard and rail both retain overlapping operational copy, users will keep expecting the wrong behavior from each surface.

## Result

When this pass is complete:

- the wizard builds strategies only,
- the chat rail acts as the explicit `xvn` operator console,
- the inspector edits real agents,
- and the settings/strategies pages stop pretending dead UI is still part of the product.
