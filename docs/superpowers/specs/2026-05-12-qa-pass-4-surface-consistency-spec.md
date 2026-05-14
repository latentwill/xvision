# QA Pass 4 — Surface Consistency Spec

**Date:** 2026-05-12  
**Surfaces:** Dashboard home, strategies list, setup wizard, strategy inspector, eval runs  
**Status:** Draft for user review

## Goal

Fix the remaining QA issues where the dashboard presents stale, misleading, or internally inconsistent state.

This pass is about product truthfulness. The user should not be able to create a strategy, run an eval, or inspect risk settings and then see a different story in the list pages, labels, or summary surfaces.

## Scope

This spec covers the visible product/UI fixes and the backend consistency work needed to make those fixes real:

- remove dead home-page status chrome
- align naming and labels with the current product model
- make wizard-created strategies persist into the same strategy store as the rest of the product
- make strategy and eval list pages refresh from real state, not stale assumptions
- expose full risk editing in the Inspector

This spec does **not** cover general remote agent access, skill installation flows, or the broader CLI discoverability problem. Those belong to the companion agent-access spec.

## Product Rules

1. The dashboard must only show status surfaces that matter in v1.
2. A strategy created anywhere in the product must land in the main strategy store and appear in the public strategy list.
3. User-facing naming must describe strategies as strategies, even if some backend/API fields still use legacy `agent_id` identifiers internally.
4. Any eval the product launches must become visible on the Eval page without requiring manual cache busting or hidden navigation.
5. Risk editing in the Inspector must expose the full editable risk shape, not a partial preset facade.

## Problem Summary

The QA issues in this pass cluster around one root problem: the UI and the underlying stores are not fully aligned.

- The home page still shows obsolete or low-value cards (`LOCAL HEALTH`, `On-chain identity`) and uses the old `Control Tower` label.
- The strategies page mixes internal identifier language with user-facing strategy concepts.
- The setup wizard appears to create drafts that are visible inside the chat/wizard path but not reliably in the main strategies surface.
- Decision cadence is shown inconsistently, including raw minute counts where the product convention should read more like a strategy timeframe.
- Eval runs can complete or be launched without becoming visible on the primary Eval route.
- The Inspector still exposes an incomplete risk-editing model.

## Batch 1 — Dashboard Home Cleanup

### Goal

Make the home route read like a v1 dashboard, not an internal diagnostics page.

### Required changes

- Rename the home page title from `Control Tower` to `Dashboard`.
- Remove the `LOCAL HEALTH` panel or equivalent probe dump from the primary home surface.
- Remove the `On-chain identity` card from the page.
- Keep useful high-level operator status only: runs, strategies, providers, and other actionable summary items.

### Acceptance criteria

- The route title reads `Dashboard`.
- No `LOCAL HEALTH` section or probe-detail block is shown on the primary page.
- No `On-chain identity` card is shown on the primary page.

## Batch 2 — Strategies Store, Refresh, and Naming

### Goal

Make the strategies list reflect the real persisted strategy store, with user-facing names and labels that match product language.

### Required changes

- Fix stale or missing refresh behavior after strategy creation.
- Ensure strategies created through the wizard land in the same persisted store used by `/api/strategies`.
- Remove the current split where a strategy can be discoverable through side channels like the command palette but absent from the main list.
- Change the user-facing column label from `Agent ID` to `Strategy ID`.
- Ensure strategy creation uses the intended strategy display name, not an identity code or opaque internal token in places where the user expects the strategy name.

### Store invariants

- `POST /api/strategies` and wizard-backed strategy creation must converge on the same persistence path.
- The strategies list must render from the same source of truth that the Inspector and eval launcher use.
- Frontend cache invalidation must happen after successful creation or mutation so the new strategy appears without a manual reload.

### Acceptance criteria

- Creating a strategy from the wizard causes it to appear on `/strategies`.
- The same created strategy is returned by the public `/api/strategies` list.
- The strategies list does not require command-palette navigation to reveal a newly created strategy.
- The page uses `Strategy ID` instead of `Agent ID` in user-facing copy.

## Batch 3 — Cadence and Timeframe Convention

### Goal

Show strategy cadence in the product’s normal timeframe convention rather than leaking raw implementation detail.

### Required changes

- Decide the user-facing cadence convention for strategy lists and summaries.
- Convert raw minute counts into that convention where shown to users.
- Fix the wizard flow so the drafted strategy’s displayed cadence matches the intended timeframe, especially for the reported `60 minutes` vs `4h` mismatch.

### Decision

The product should present cadence as a strategy timeframe in user-facing surfaces, with consistent formatting derived from the stored cadence value. Raw minute counts may remain in low-level API payloads or developer diagnostics, but not as the primary presentation language.

### Acceptance criteria

- The wizard no longer reports a misleading cadence for the affected draft path.
- Strategy summary surfaces use one cadence convention consistently.
- The convention is derived from persisted strategy state, not an ephemeral wizard-only interpretation.

## Batch 4 — Eval Visibility and List Truthfulness

### Goal

Make eval launches visible on the Eval page as soon as they are real product state.

### Required changes

- Verify the eval-launch path used from the Inspector or wizard writes to the same run store as `/api/eval` and `/eval-runs`.
- Fix any missing invalidation, polling, or status propagation that allows an eval to run without appearing on the Eval page.
- Preserve the existing runs UX improvements already planned in earlier eval work; this pass is specifically about visibility and consistency.

### Acceptance criteria

- A launched eval appears on `/eval-runs` without requiring manual refresh or secondary navigation.
- If the run is queued/running/completed, the list reflects one of those real states rather than hiding the run.

## Batch 5 — Inspector Risk Editing

### Goal

Expose complete, editable risk configuration in the Inspector.

### Required changes

- Replace the current partial risk editing surface with full-field editing for the persisted risk config.
- Support editing all v1 risk fields individually, not only preset selection.
- Make validation visible when a field combination is invalid.
- Keep the UI aligned with the actual strategy risk object stored by the engine.

### Minimum editable fields

The exact field names should match the engine’s current risk schema, but the Inspector should expose the full persisted v1 risk shape, including per-trade risk, leverage, concurrency, and stop/loss or daily-kill-switch-style controls where those exist in the engine model.

### Acceptance criteria

- Users can edit the entire v1 risk config from the Inspector.
- Edits persist to the real strategy record.
- Invalid combinations produce visible validation feedback.

## Cross-cutting requirements

### User-facing terminology

- Home route: `Dashboard`
- Strategy list label: `Strategy ID`
- Strategy names should be display names where users expect names; opaque ids remain secondary/reference data

### Source-of-truth discipline

- Wizard, strategies list, Inspector, and Eval page must agree on persisted objects.
- No feature should rely on a hidden ephemeral draft that is not promoted into the public store while the UI implies it is already saved.

### Cache behavior

- Successful mutations must invalidate the relevant React Query keys or equivalent frontend cache entries.
- Backend writes must complete before the UI claims success.

## Out of scope

- General agent/CLI discoverability
- Remote execution over Tailscale
- Full redesign of the home/dashboard information architecture
- Renaming all internal backend types away from `agent_id` in one pass

## Risks

1. The strategy/agent composition refactor is already in flight, so this pass must avoid re-implementing obsolete fixed-slot behavior.
2. Some stale-list symptoms may come from a mix of persistence mismatch and cache invalidation; fixing only one side would leave a partial bug.
3. Cadence formatting can become inconsistent again if each route humanizes it separately instead of using one shared formatter.

## Result

When this pass is complete:

- the home page reads like a real dashboard,
- created strategies show up where users expect them,
- cadence is presented consistently,
- eval runs become visible from the main Eval page,
- and the Inspector exposes full editable risk control instead of a partial facade.
