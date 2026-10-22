# Chat Rail Context Switching Implementation Plan

> **For agentic workers:** REQUIRED: Use `superpowers:subagent-driven-development` (if subagents available) or `superpowers:executing-plans` to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend xvision’s chat rail so the operator can move between whole-workspace context, a high-level category context, the current page, and a specific page/object context like an eval, strategy, scenario, or agent, with the active scope always visible and machine-readable.

**Architecture:** Reuse xvision’s existing `ContextScope` path as the persisted session scope instead of inventing a second context model in dashboard state. The server already knows how to store and resolve scope for the chat rail; this plan should extend that path with the extra UI affordances needed to switch between workspace, route/page, and object-focused views. Category selection should be treated as a UI-level grouping or suggestion layer that maps into an existing scope when possible, not as a parallel authoritative session record. Keep the design additive: existing chat sessions should continue to work with default workspace context when no richer context target is selected.

**Tech Stack:** Rust 2021 workspace (`xvision-dashboard`, `xvision-engine`), Axum routes, SQLite-backed session state, serde, current chat-rail SSE / REST surfaces, existing eval / strategy / scenario / agent route models, and dashboard UI components.

---

## Why this matters

The chat rail already has session state, mode switching, and object-specific pages, but today the operator has to carry context in their head. That makes it hard to ask, “what is this chat about right now?” or to move cleanly from a broad marketplace discussion into a specific eval, strategy, scenario, or agent. This plan makes context explicit and persistent.

The target experience should support:

- **Whole context** — the default workspace-wide chat view
- **High-level category** — e.g. `Marketplace` or `Autoresearcher`
- **Current page** — whatever page the operator is on right now
- **Individual object** — a concrete eval, strategy, scenario, or agent

The plan should also account for categories that are planned but not fully implemented yet, especially `Autoresearcher`, without pretending those surfaces already exist.

---

## Proposed context model

Reuse the existing `ContextScope` enum as the persisted session scope, and extend the chat rail’s selector UI around it instead of creating a second hidden model.

The implementation should preserve the current scope kinds already in the repo:

- `Workspace`
- `Route { route }`
- `Run { run_id }`
- `Strategy { draft_id }`
- `Deployment { deployment_id }`
- `Compare { run_ids }`
- `JournalFilter { kinds }`
- `Selection { items }`
- `Seed { seed_id }`

For this feature, the important additions are the UX and resolution rules:

- **Category selection**
  - a UI-level grouping / jump target, not necessarily a new persisted scope variant
  - should surface live categories like `Marketplace`
  - may surface planned categories like `Autoresearcher` as disabled or roadmap-backed entries if the product wants that

- **Current page resolution**
  - map the dashboard’s active page into the existing `Route { route }` scope when possible
  - promote concrete detail views into `Run`, `Strategy`, `Deployment`, or `Selection` when there is already an existing semantic match

- **Context display label**
  - derived from the resolved `ContextScope::header_label()` path, with any extra category label layered on top in the UI

- **Context metadata**
  - route, object name, object type, and any safe summary needed to orient the model
  - must be explicit and serializable so traces can show what context was active

The operator-facing default should remain `Workspace` when a page or category cannot be resolved safely.

---

## Files to touch

- Modify: `crates/xvision-dashboard/src/routes/chat_rail.rs`
- Modify: `crates/xvision-dashboard/src/state.rs`
- Modify: `crates/xvision-dashboard/src/session_bus.rs`
- Modify: `crates/xvision-dashboard/src/chat_unified.rs`
- Modify: `crates/xvision-dashboard/src/error.rs`
- Modify: `crates/xvision-dashboard/src/wizard_loop.rs`
- Modify: `crates/xvision-dashboard/src/routes/eval/mod.rs` and/or `crates/xvision-dashboard/src/routes/eval_runs.rs` for eval detail / summary surfaces
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs` for strategy detail / summary surfaces
- Modify: `crates/xvision-dashboard/src/routes/scenarios.rs` for scenario detail / summary surfaces
- Modify: `crates/xvision-dashboard/src/routes/agents.rs` for agent detail / summary surfaces
- Modify: the dashboard UI component that renders the chat rail context selector
- Modify: any shared route metadata helper used to derive current page labels
- Add tests alongside the touched route / state modules
- Update: the relevant dashboard wiki/operator docs if the new context switcher changes operator flow

> If the exact route module names differ in the current tree, use the existing page/route files that own those detail views. The plan should follow the real route ownership, not invent new modules.

---

## Task 1: Extend `ContextScope` persistence for the chat rail

**Files:**
- Modify: `crates/xvision-engine/src/chat_session/context.rs`
- Modify: `crates/xvision-dashboard/src/routes/chat_rail.rs`
- Modify: `crates/xvision-dashboard/src/error.rs`
- Modify: `crates/xvision-dashboard/src/state.rs` only if the dashboard shell caches scope labels or session snapshots
- Test: `crates/xvision-dashboard/src/routes/chat_rail.rs` tests

- [ ] **Step 1: Add a failing test for round-tripping the existing scope model**

Write a test that creates a chat session, sets its scope to `Route { route: "/strategies" }` or `Workspace`, reloads the session, and asserts the stored `ContextScope` survives round-trip.

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test -p xvision-dashboard chat_rail -- --nocapture`

Expected: fail because the session scope plumbing is still incomplete for this new workflow.

- [ ] **Step 3: Extend the existing scope serialization path**

Re-use the current `ContextScope` serde shape and session store path rather than introducing a second persisted context record. Add only the minimum new support needed for the selector workflow.

- [ ] **Step 4: Re-run tests**

Run: `cargo test -p xvision-dashboard chat_rail -- --nocapture`

Expected: PASS.

---

## Task 2: Add chat-rail APIs for switching context

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/chat_rail.rs`
- Modify: `crates/xvision-dashboard/src/session_bus.rs`
- Modify: `crates/xvision-dashboard/src/chat_unified.rs`
- Test: `crates/xvision-dashboard/src/routes/chat_rail.rs` tests

- [ ] **Step 1: Write failing tests for the context switch endpoint**

Cover these cases:

- switch to `workspace`
- switch to `category=Marketplace`
- switch to `category=Autoresearcher`
- switch to `object` for an eval/strategy/scenario/agent
- reject invalid scope kinds with a 400

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p xvision-dashboard chat_rail -- --nocapture`

Expected: the new endpoint or request body does not exist yet.

- [ ] **Step 3: Add the request/response schema**

Add a `SetContextReq` / `SetContextResp` pair or equivalent. The request should map directly onto `ContextScope` variants and only carry extra category labels when the UI needs them.

- [ ] **Step 4: Wire the new route into the chat-rail router**

Add the new endpoint next to the existing session/mode/tool-policy handlers so context switching is a first-class chat-rail action, not a hidden side channel.

- [ ] **Step 5: Broadcast context changes over the session bus**

Emit an event when the operator switches context so the UI can update without a full reload.

- [ ] **Step 6: Re-run tests**

Run: `cargo test -p xvision-dashboard chat_rail -- --nocapture`

Expected: PASS.

---

## Task 3: Resolve current page and object context from real dashboard routes

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/eval/mod.rs` and `crates/xvision-dashboard/src/routes/eval_runs.rs`
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`
- Modify: `crates/xvision-dashboard/src/routes/scenarios.rs`
- Modify: `crates/xvision-dashboard/src/routes/agents.rs`
- Modify: the helper that computes page metadata for the dashboard shell
- Modify: `crates/xvision-dashboard/src/routes/chat_rail.rs`
- Modify: `crates/xvision-dashboard/src/state.rs` only if the shell stores route labels or breadcrumbs
- Test: route metadata / context-resolution tests near the touched routes

- [ ] **Step 1: Write failing tests for page-to-context resolution**

Test that a known route resolves to a stable page key and label, and that object pages resolve to the matching existing `ContextScope` variant when the dashboard is on an eval, strategy, scenario, or agent detail view.

- [ ] **Step 2: Run the tests and confirm they fail**

Run the relevant route tests or `cargo test -p xvision-dashboard page_context -- --nocapture` if you add a focused test module.

- [ ] **Step 3: Add a page metadata helper**

Create or extend a helper that maps the current dashboard location to a page key, optional category label, and object identity when available.

- [ ] **Step 4: Use the helper to prefill chat-rail context**

When the user opens a known page, the chat rail should be able to infer a default `ContextScope` automatically while still allowing the operator to override it manually.

- [ ] **Step 5: Make unknown pages degrade cleanly**

If the current page cannot be classified, keep the session in workspace context rather than inventing a misleading target.

- [ ] **Step 6: Re-run tests**

Run: `cargo test -p xvision-dashboard page_context -- --nocapture`

Expected: PASS.

---

## Task 4: Build the chat-rail UI for context switching

**Files:**
- Modify: the dashboard chat-rail component that renders the session header / toolbar
- Modify: any component that shows session state or page breadcrumbs
- Modify: `crates/xvision-dashboard/src/routes/chat_rail.rs` if the UI needs a new fetch payload
- Test: front-end component tests or story-level checks if present

- [ ] **Step 1: Write a failing UI test for the selector**

Cover that the rail shows the active context label and offers switching between workspace, category, current page, and selected object.

- [ ] **Step 2: Run the test and confirm it fails**

Run the appropriate front-end test command used by the dashboard workspace.

- [ ] **Step 3: Add the selector UI**

Render the active context and provide explicit actions to switch scope. Include `Marketplace` and `Autoresearcher` in the category list even if `Autoresearcher` is still a planned category.

- [ ] **Step 4: Hook the selector to the API**

Submitting a new context should update the server-side session record and refresh the rail state.

- [ ] **Step 5: Re-run the UI test**

Expected: PASS.

---

## Task 5: Update prompt assembly, traces, and docs

**Files:**
- Modify: `crates/xvision-dashboard/src/wizard_loop.rs`
- Modify: `crates/xvision-dashboard/src/chat_unified.rs`
- Modify: `crates/xvision-dashboard/wiki/*` if operator docs need it
- Modify: any trace or event serialization helper used by the rail

- [ ] **Step 1: Add tests for prompt context injection**

Verify the model sees the active `ContextScope` in a structured, readable form and that object context includes the relevant object identity.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p xvision-dashboard wizard_loop -- --nocapture`

Expected: the prompt assembly does not yet reflect the updated selector workflow.

- [ ] **Step 3: Extend the existing scope-to-prompt path**

Build on the current `ContextScope::header_label()` / prompt assembly flow so the active scope, optional category label, and object/page metadata are visible in the model input without duplicating the scope model.

- [ ] **Step 4: Surface context in traces and logs**

Ensure the active context is visible in run metadata so debugging and compare surfaces can explain what the chat rail was anchored to.

- [ ] **Step 5: Update the operator docs**

Document the difference between workspace, category, page, and object context, and call out that `Autoresearcher` can be surfaced as a planned category entry even before the full feature exists.

- [ ] **Step 6: Re-run tests**

Run: `cargo test -p xvision-dashboard wizard_loop -- --nocapture`

Expected: PASS.

---

## Acceptance criteria

- The chat rail can switch between workspace, category, page, and object context.
- `Marketplace` is available as a live category context, and `Autoresearcher` can appear as a planned or roadmap-backed category entry.
- The current page can be referenced directly without manual re-entry.
- Individual eval / strategy / scenario / agent objects can be selected as context targets.
- Existing sessions still work when no context target is set.
- Context changes are visible in the UI and persisted on the server.
- Prompt assembly and traces reflect the active context explicitly.

---

## Notes / risks

- Do not let context switching become a hidden multi-agent router. It should be a visible rail feature.
- Do not invent object IDs or route classifications where the current page cannot be reliably resolved.
- `Autoresearcher` should be treated as a planned category surface, not proof that the underlying system exists today.
- Keep the default behavior conservative: if context cannot be resolved, fall back to workspace context rather than guessing.
