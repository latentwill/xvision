# QA Pass 2 — Spec

**Date:** 2026-05-11
**Surfaces:** Providers, Strategy, Eval, Chat-rail
**Status:** Spec — reviewed once with user. Ready to break into per-surface plans.

---

## Goal

Address all items the user logged in QA Pass 2. The pass cuts across four surfaces (Providers page, Strategy page, Eval page, Chat-rail) with one shared component (`ChatRail.tsx`) that touches strategy and chat items at once.

Several QA items depend on a foundational rework that is already in flight: `[intern]` → "default agent" / "default LLM". That rework is captured as **Batch 0** below; the surface items that depend on it (default-model UI, wizard model selection, per-agent model overrides) reference it explicitly.

This is a spec, not a plan — task-level decomposition happens in per-surface plans authored after this is reviewed.

---

## Batch 0 — Foundational rework: `[intern]` → "default agent" / "default LLM"

This is in flight outside QA Pass 2 but gates several items in this spec, so it's documented here for cross-reference.

### Why this is the gating change

Today `RuntimeConfig.intern` (`crates/xvision-core/src/config.rs`) is the single "always-on LLM" slot used by:

- The wizard at `/setup` (`crates/xvision-dashboard/src/routes/wizard.rs`)
- The chat-rail (`crates/xvision-dashboard/src/routes/chat_rail.rs`)
- The dashboard tool-loop dispatcher (`crates/xvision-dashboard/src/llm_dispatch.rs`)

The user has flagged two architectural problems with this slot:

1. **No "default model" concept exists.** The Settings UI today only lets users pick a default *provider* (the "demote/promote" buttons on `/settings/providers`). Within that provider, `[intern].model` is whatever was set in the TOML — not exposed in the UI. The wizard works around this by calling `streamChat({ message })` with `provider: undefined, model: undefined` and relying on the backend to fill in `[intern]`'s values. This is what produces the `[wizard] streamChat {provider: undefined, model: undefined}` log line — the wizard is intentionally agnostic, but the user wants it to use a real default-model value it has explicit knowledge of.

2. **"Intern" is awkward terminology for what is actually a "default agent" / "default LLM".** The rename is happening as part of the broader multi-agent strategy work.

### Shape of the rework

- `RuntimeConfig.intern` → `RuntimeConfig.default_agent` (or similar — final name TBD by the rework owner; this spec uses **`default_agent`** as a placeholder).
- `default_agent` carries both `provider` and `model` as user-settable fields.
- Settings UI exposes a "Default agent" panel with both pickers (provider dropdown + model dropdown, where the model list is filtered to the selected provider's `enabled_models`).
- The wizard (`setup.tsx:49`) calls `streamChat({ provider, model, message })` with the default-agent values explicitly populated from a `/api/settings/default-agent` GET.
- The chat-rail (`ChatRail.tsx:195-211`) already accepts user-selected provider/model via the `ModelPicker`; the fallback path for "no model picked" reads default-agent rather than backend-filling intern.

### Items in this spec that depend on Batch 0

- **Item 1.1 (default provider concept):** must be reframed as "default agent" with provider + model.
- **Item 3.1c (wizard uses default model):** literal output of Batch 0 — the wizard call site reads default-agent and passes both fields.
- **Item 3.2 (model selection on strategies page):** per-strategy and per-agent model selection both build on the same model-picker primitive used by default-agent UI.

If Batch 0 is not finished by the time we plan-write Surface 1 or Surface 3, those plans need to be authored in coordination with the rework owner so we don't ship a regression of the intern slot.

### Out of scope for this spec

This spec **does not** plan Batch 0 — that's owned by the rename/rework. We only state how the QA items integrate with it.

---

## Recommended sequencing

| Batch | Surface | Items | Effort | Depends on |
|---|---|---|---|---|
| 0 | Core | intern → default-agent rework | (external) | — |
| 1 | Connections / Providers | 1.1, 1.2, 1.3 | Medium | Batch 0 (for 1.1) |
| 2 | Chat-rail | 2.1, 2.2, 2.3, 2.4, 2.5 | Medium | — |
| 3 | Strategy | 3.1a, 3.1b, 3.1c, 3.2, 3.4, 3.5 | Medium-large | Batch 0 (for 3.1c, 3.2), Batch 2 (extract ModelPicker) |
| 4 | Eval | 4.1 | Medium-large | — |

Batches 2 and 4 are unblocked today and can ship in parallel. Batch 1 is mostly unblocked (Items 1.2 and 1.3 stand alone); only 1.1 waits on Batch 0. Batch 3 is the most coupled — depends on both Batch 0 (default-model) and Batch 2 (extracted `ModelPicker`).

---

## Surface 1 — Connections / Providers (`/settings/providers` + `/settings/brokers`)

**Tab structure stays as-is.** The user wants Providers (LLMs) and Brokers as separate tabs in the Settings nav. Brokers will gain more entries (other broker APIs) later, so it stays independent.

### Item 1.1 — Rework "default provider" into "default agent (provider + model)"

**User report:** "Add another provider with an API key, then you can demote this one and remove it." — redundant, remove default provider and then you don't need this.

**Where the friction lives today:**

- Frontend message: `frontend/web/src/routes/settings/providers.tsx:358-365` (renders when `locked && !canPromote`).
- `locked` derives from `row.referenced_by_intern` — a computed field on the backend (`crates/xvision-engine/src/api/settings/providers.rs:40-59`).
- Promote button + mutation: `providers.tsx:119-122, 318-324`.
- Backend endpoint: `POST /api/settings/providers/{name}/set-default` — `crates/xvision-engine/src/api/settings/providers.rs:434-454`; wired in `settings.rs:103-114`.
- The "default" is not a flag on a provider row — it's the workspace-level `[intern]` slot in `RuntimeConfig`.

**Why the user wants this changed:** the user's read is that "default provider" alone isn't a meaningful concept — it has to be paired with "default model" for the wizard to do anything useful. Today there's no UI to set the default *model*, so the "default provider" button feels half-built. The "demote then remove" copy is the symptom of a half-built concept.

**Proposed change (depends on Batch 0):**

- Drop the per-row "Make default" / "demote" / "lock" treatment from the providers list. No button, no copy, no `referenced_by_intern` lock.
- Replace with a top-of-page **"Default agent"** card that exposes:
  - Provider dropdown (one row per provider with `api_key_set && !synthetic`).
  - Model dropdown (filtered to the selected provider's `enabled_models`).
  - "Save" action posting to a new `PUT /api/settings/default-agent` endpoint (or whatever the Batch 0 rework names it).
- Remove `POST /api/settings/providers/{name}/set-default` and its frontend mutation. The default agent's provider is now changed by editing the Default-agent card, not by per-row promote.
- When the user attempts to remove a provider that is currently the default agent's provider, show a confirm dialog forcing them to re-point the default agent first.

**Scope:** Medium. Frontend re-render + backend endpoint addition/removal + the Default-agent card. Most of the lift is on the Batch 0 side.

---

### Item 1.2 — Add-key box auto-shows at 0 providers, hides after first add

**User report:** Start with 0 providers, then have "add an llm key box" visible. However once one is added, the box should disappear. Right now it stays visible.

**Where it lives today:**

- `frontend/web/src/routes/settings/providers.tsx:108` — `const [adding, setAdding] = useState(false);` controls visibility.
- `providers.tsx:168-175` — "+ Add provider" button (shown when `!adding`).
- `providers.tsx:183-191` — `AddProviderForm` renders when `adding === true`.
- `providers.tsx:193-196` — Empty-state hint "no providers yet — click + Add provider".

**Investigation finding:** The form's open/close is purely user-driven. There is no auto-open at 0 providers. The user's experience of "the box stays visible after I add a provider" is a paraphrase of: "the page never auto-presented me a form on first visit, and after I added a provider I still see the `+ Add provider` button which feels wrong when I have what I need." That's a UX shape issue, not a state bug.

**Proposed change:**

- When `rows.length === 0`: render `AddProviderForm` inline directly (no toggle, no button, no empty-state placeholder). The page IS the form.
- When `rows.length >= 1`: show the providers table; keep the `+ Add provider` button so users can add more, but do not auto-expand. (Same as today's "added" state.)
- `adding` state stays meaningful for the post-first-provider case only.

**Scope:** Small. Single-component refactor.

---

### Item 1.3 — Connection test + Alpaca card

**User report:** Add API "connected" test and alpaca.

**Where the pieces live today:**

- No LLM-provider connection-test endpoint exists. Model fetch via `/v1/models` (`fetch_models_inner` at `crates/xvision-engine/src/api/settings/providers.rs:256-311`) is a workable connectivity proxy.
- Alpaca lives on the Brokers tab today: `frontend/web/src/routes/settings/index.tsx:57-339` (`AlpacaBrokerCard`), backed by `crates/xvision-engine/src/api/settings/brokers.rs` and stored at `~/.xvn/secrets/brokers.toml`.
- Backend executor uses Alpaca at `crates/xvision-execution/src/alpaca.rs`.

**Tab structure decision (user-confirmed):** Brokers stays as a separate Settings tab. We add connection tests *to each tab independently* rather than folding them into one page.

**Proposed change — split into two parts:**

**1.3a — Connection test for LLM providers (on Providers tab):**

- New backend endpoint `POST /api/settings/providers/{name}/test-connection` that calls the provider's `/v1/models` endpoint (Anthropic/OpenAI-compat dispatch is already in `fetch_models_inner` — reuse).
- Response: `{ ok: bool, latency_ms: u32, error?: string }`.
- Frontend: a small "Test" button per provider row that calls the endpoint and shows a pill — green ✓ with `<latency_ms>ms`, or red × with the error message. Last-test timestamp shown next to the pill.

**1.3b — Connection test for Alpaca (on Brokers tab):**

- New backend endpoint `POST /api/settings/brokers/alpaca/test-connection` that calls Alpaca's `/v2/account` endpoint and returns `{ ok, account_status, equity, error? }`.
- Frontend: equivalent "Test" button on the `AlpacaBrokerCard` — green ✓ with account status + equity, or red × with error.

**1.3c — Brokers tab gets ready for expansion:**

- The Brokers tab today renders a single `AlpacaBrokerCard`. Restructure as `<BrokerList>` rendering a card per broker, with Alpaca being the only entry today. A "+ Add broker" affordance can be a stub for now ("more brokers coming") so the shape is right for later additions.

**Scope:** Medium. Two new endpoints + frontend test-button UI on both tabs + broker-list scaffolding.

---

## Surface 2 — Chat-rail (`ChatRail.tsx`)

The chat-rail is a single global component (`frontend/web/src/components/shell/ChatRail.tsx`) mounted in the layout. All chat-rail fixes land here.

### Item 2.1 — Active-process indicator (spinner / typing bubble)

**User report:** Need a spinning "active process" indicator for chat wherever it appears.

**Where it lives today:**

- `ChatRail.tsx:71` — `isStreaming` state exists.
- `ChatRail.tsx:191-211` — set true at stream start, false in the `finally` block.
- `ChatRail.tsx:302, 314` — used to disable QuickReplies and Composer.
- `ChatRail.tsx:292, 320-343` — `Thread` component does **not** receive `isStreaming` and shows no indicator.

**Proposed change:**

- Pass `isStreaming` to `Thread`.
- When `isStreaming && last bubble is empty/incomplete assistant`, render an animated three-dot indicator inside that bubble.
- When a tool-call is mid-execution (between `tool_call` event and `tool_result` event), show a per-tool spinner on the tool chip.

**Scope:** Small.

---

### Item 2.2 — Composer max-height + scroll

**User report:** Need scroll bar on chat bar, right now it keeps extending forever.

**Where it lives today:**

- `ChatRail.tsx:504-510` — `Composer` is the component to inspect; need to read lines 480-525 during plan-write to confirm whether it's `<input>` or `<textarea>`.
- The "extends forever" report wouldn't match a plain `<input>` (single-line, no growth), so it's likely an auto-grow `<textarea>` somewhere or the rail itself overflowing the viewport.

**Proposed change:**

- Read the full `Composer` body first. If `<textarea>` with autosize: cap with `max-h-[120px] overflow-y-auto resize-none`; switch to Cmd+Enter-submit so newlines don't always fire send.
- If `<input>`: the issue is elsewhere — likely the rail's vertical layout. Ensure the `Thread` container has `flex-1 min-h-0 overflow-y-auto` so messages scroll inside the rail without pushing the composer off-screen.
- Verify in-browser before claiming done.

**Scope:** Small.

---

### Item 2.3 — Inline confirmations for `xvn` tool calls

**User report:** Bot does not seem to be able to make strategies, or perhaps they aren't loading (I see an id number…). Need clear confirmations on actions taken with xvn that show up in the chat (not direct CLI commands, but more like creating strategy… setting risk… etc as each command flows).

**Where it lives today:**

- `ChatRail.tsx:537-558` — `applyEvent("tool_call")` and `tool_result` update tool chips on the latest assistant bubble.
- `ChatRail.tsx:655-699` — `summarizeArgs()` and `summarizeResult()` build the short pill text. The pasted chat log shows pills like `create_strategy · 01KRBS2FR61F91HZ57AP62RV0W` — an opaque ID with no human-readable narrative.

**Proposed change:**

Map each known `xvn` tool to a human-readable confirmation rendered as an inline event row above the next assistant token, not just a pill. Per-tool phrasing:

- `create_strategy` → "✓ Created strategy **`<display_name>`** (`<agent_id>`)"
- `set_mechanical_param` → "✓ Set `<param>` = `<value>`"
- `set_risk_config` → "✓ Risk: `<level>` (`<per_trade>` per trade, `<daily_loss>` daily kill)"
- `validate_draft` → "✓ Validation passed" or "✗ Validation failed: `<reason>`"
- `get_strategy` → render nothing (read-only; the model uses the result inline)
- `list_templates` → render nothing (same reason)

Track in a new `toolNarrative()` helper alongside the existing `summarizeArgs/Result`. The pills can stay as a secondary, dimmer "debug" row beneath the narrative for power users.

**Scope:** Medium. New helper + render path + per-tool data shaping.

---

### Item 2.4 — Markdown rendering for assistant messages

**User report:** Work on fixing formatting of agent in prompt. Maybe we give it .md display?

**Where it lives today:**

- `ChatRail.tsx:357` — assistant bubble renders `{b.text}` inside `<div className="whitespace-pre-wrap">`. Raw markdown source.
- `package.json` has no markdown library installed.

**Proposed change:**

- Add `react-markdown` + `remark-gfm` (for tables, strikethrough, task lists) to `frontend/web/package.json`.
- Replace the raw `{b.text}` with `<ReactMarkdown remarkPlugins={[remarkGfm]} components={...}>{b.text}</ReactMarkdown>`.
- Theme overrides for `<table>`, `<code>`, `<strong>` so they pick up rail's design tokens.
- Skip markdown render on user bubbles — they're plain text.

**Streaming note:** Markdown can flicker badly during token streams (a half-rendered `**bold` looks ugly). Two options:

- **A — Render-on-complete:** Stream as plain text; swap to markdown render after the bubble's stream finishes.
- **B — Debounce parse:** Re-parse every 100ms during stream.

Recommendation: B (debounce). Smoother. Confirm in plan-write.

**Scope:** Small.

---

### Item 2.5 — Duplicate-looking entries in the model picker

**User report:** the chat shows `— pick a model —deepseek-v4-prodeepseek/deepseek-v4-prodeepseek/deepseek-v4-flash` — the same model name (or a variant) appearing under multiple providers.

**Where it lives today:**

- `ChatRail.tsx:403-466` — `ModelPicker` component.
- `ChatRail.tsx:457` — `<option key={o.model} value={`${o.provider}::${o.model}`}>{o.model}</option>` — **the key uses only model, not the (provider, model) pair. When two providers both expose the same model name, React keys collide.** That's a real bug.
- Model labels themselves come from each provider's `enabled_models: string[]` — those lists are curated per provider and may legitimately overlap.

**Decision (user-confirmed):** **Don't dedupe.** Model lists per provider must remain accurate and verbatim. The fix is purely in the rendering.

**Proposed change:**

- Fix the React key: `key={`${o.provider}::${o.model}`}` at `ChatRail.tsx:457`. One-line.
- Make duplicates visually unambiguous by showing the provider as part of the option label, e.g. `<option>{o.model}  ·  {o.provider}</option>`. Combined with the existing `<optgroup label={g.provider}>` grouping, the user can always tell which provider serves which model.
- No normalization. No canonicalization. No "preferred provider" pick. If DeepSeek's API exposes `deepseek-v4-pro` and OpenRouter exposes `deepseek/deepseek-v4-pro`, both render as separate, addressable options.

**Scope:** Trivial.

---

## Surface 3 — Strategy page (`/strategies`, `/authoring/:id`, `/setup`)

### Item 3.1a — "New from template" button must work (currently disabled)

**User report:** Let's get this plan complete, seems it was missed first pass.

**Investigation finding:** The "New from template" button at `frontend/web/src/routes/strategies.tsx:79-85` has `disabled` set in code and `title="Coming in Plan 3 (Authoring)"`. Clicking it does nothing. This is **not** a bug in the chat-rail or wizard — it's a feature that was deferred and the user wants finished.

**Proposed change:**

- Add a `/strategies/new` route or a modal that opens on click, listing the 9 templates already returned by the `list_templates` tool (`crates/xvision-engine/src/api/templates/`).
- For each template, show: friendly name, one-line description (from the existing template metadata), and a "Use this template" button.
- On select, prompt the user for a strategy name (default-agent's model selection is taken from Batch 0 default — user can override later in the inspector).
- Call `POST /api/strategies` (verify endpoint exists; if not, add it — it should accept `{ template: <slug>, name: <string> }` and return the created `agent_id`).
- On success, redirect to `/authoring/:id` (existing inspector at `frontend/web/src/routes/authoring.tsx:46`).
- Remove the `disabled` prop and "Coming in Plan 3" title.

**Open question:** Does the existing wizard at `/setup` get superseded by this template-picker form path, or do they coexist? User feedback in plan-write needed. Recommendation: both coexist — `/setup` is the chat-driven authoring path for users who prefer conversational creation; `/strategies/new` is the form-driven path for users who already know the template they want.

**Scope:** Medium. New route + template-picker UI + (potentially new) `POST /api/strategies` endpoint + redirect to inspector.

---

### Item 3.1b — Root-cause the chat-rail's stale-session 404

**User report:** Figure out why error pops up, don't suppress. This is alpha. Fix right from the beginning.

**Phase 1 root-cause investigation:**

The 404 the user pasted:
```
POST https://xvn.tail2bb69.ts.net/api/chat-rail/sessions/01KRBE93TQBW0XHVS5D1V7G0DA/scope 404 (Not Found)
```

Comes from `frontend/web/src/components/shell/ChatRail.tsx:137`:
```ts
const cached = localStorage.getItem(SESSION_LS_PREFIX + key);
if (cached) {
  id = cached;
  await updateScope(id, scope).catch(() => {
    throw new Error("session-stale");
  });
}
```

The backend handler at `crates/xvision-dashboard/src/routes/chat_rail.rs:65-79` intentionally returns 404 when the session id doesn't exist:
```rust
ChatSessionStore::load_scope(&state.pool, &id)
    .await
    .map_err(|_| DashboardError::NotFound(format!("session '{id}'")))?;
```

**The bad state:** The frontend cached a session id in `localStorage[SESSION_LS_PREFIX + key]`, but the corresponding `chat_sessions` row no longer exists server-side. Causes:

1. **Dev database reset** — the local SQLite at `~/.xvn/db.sqlite` (or wherever `chat_sessions` lives) was wiped, but the localStorage cache survived.
2. **Build/deploy with a fresh DB** — same issue across deploys.
3. **Server-side GC** — none exists today, so not a real cause yet.
4. **Manual deletion** — user/operator dropped the row directly.

The frontend's current behavior is to catch the 404, throw `"session-stale"`, drop the cache, and create a fresh session. **Functionally it recovers, but the user wants the bad state not to occur in the first place** — not just papered over.

**Why "just suppress the 404 in DevTools" was the wrong recommendation:** because the underlying lifecycle is broken — the frontend is treating a localStorage UUID as authoritative when the backend has no notion of that id existing. For an alpha product, we want the contract to be sound.

**Proposed fix — change session-id ownership:**

Stop caching session ids client-side. Instead, the backend resolves a session for a given scope on demand:

- New endpoint: `POST /api/chat-rail/sessions/resolve` with body `{ scope: ContextScope }`.
- Backend looks up the most-recent session for that scope (table `chat_sessions` already exists per `crates/xvision-engine/src/chat_session.rs`). If found, return its id + history. If not found, create one and return the new id + empty history.
- Frontend's mount-effect (`ChatRail.tsx:121-170`) collapses into one call to `resolve` — no localStorage cache, no `updateScope` race, no 404 path.
- `updateScope` becomes purely the "user navigated mid-session and we need to attach a new scope to the existing session" path — and even that becomes redundant if `resolve` is called on every scope change. Keep `updateScope` only if we have a use case where the session lifetime needs to outlive scope changes (e.g., long-running tool execution). Confirm in plan-write.

**Why this is the right alpha fix:**

- Removes an entire class of stale-cache bugs.
- The 404 is impossible because the backend owns the lifecycle.
- Pages still resume the previous conversation for that scope (good UX).
- The "Start fresh" button (`ChatRail.tsx:217-231`) still works — it deletes the resolved session, and the next `resolve` call creates a new one.

**Trade-off:** A user with multiple browser tabs on the same scope will share a session. That's actually desirable for the rail's "context" model (one conversation per scope) — but worth confirming.

**Scope:** Medium. New backend resolver + frontend mount-effect simplification + remove localStorage caching keys.

---

### Item 3.1c — Wizard uses the default agent's model (not undefined)

**User report:** Wizard should use the default model! Right now we can only set default provider not default model.

**Investigation finding:** The wizard calls `streamChat({ message })` with no provider/model (`frontend/web/src/routes/setup.tsx:49`). The backend falls back to `[intern]`. The log line `[wizard] streamChat {provider: undefined, model: undefined}` is the wizard intentionally leaving these unset.

The user's complaint isn't that the fallback fails — it's that **the wizard doesn't know what model it's using** because the user has no UI to set a default model. Once Batch 0 lands (Default Agent: provider + model), the wizard should read both and pass them explicitly.

**Proposed change (post-Batch 0):**

- Add a `useDefaultAgent()` query (frontend) that fetches `GET /api/settings/default-agent` returning `{ provider, model }`.
- Update `setup.tsx:49` from `streamChat({ message: userText })` to:
  ```ts
  const defaultAgent = useDefaultAgent();
  // ...
  streamChat({
    provider: defaultAgent.data?.provider,
    model: defaultAgent.data?.model,
    message: userText,
  })
  ```
- Add a "Model: `<provider> / <model>`" inline label at the top of the wizard so the user can see what's running.
- Optionally: a "Change" link that opens a popover with the same `ModelPicker` from the chat-rail (extracted in Batch 2) for one-off override during this wizard session.
- If no default agent is configured yet (fresh install), the wizard shows an inline prompt: "Pick a default agent in Settings → Providers before continuing" with a link to that page.

**Scope:** Small (post-Batch 0). The lift is Batch 0; this is just consumption.

---

### Item 3.2 — Per-strategy AND per-agent model selection

**User report:** Need model selection in strategies page. Per strategy and PER AGENT. Need to clear that up for multi agent strategies.

**Context — xvision strategies are multi-agent:** A single strategy bundle can contain N agents, each with its own model assignment. The strategies list shows one row per strategy bundle today (`frontend/web/src/routes/strategies.tsx:97-141`), no agent-level breakdown.

**Two-level model selection:**

| Level | Where it shows | What it controls |
|---|---|---|
| Strategy default | `/strategies` (list view) | The "primary" / first / default agent's model |
| Per-agent override | `/authoring/:id` (inspector) | Each individual agent's model in a multi-agent strategy |

**Where the data lives today:**

- Strategy bundle manifest is loaded by `crates/xvision-engine/src/api/strategies/` (verify exact module structure in plan-write).
- Each agent in the manifest carries a model field (need to confirm the exact schema in `crates/xvision-core/src/manifest/` — the current eval `Algorithm` trait suggests agents are first-class).
- Inspector at `frontend/web/src/routes/authoring.tsx:46-104` already renders slot editors and manifest fields; needs new per-agent model picker rows.

**Proposed change:**

**3.2a — Strategy-list page:** Add a "Model" column to `StrategiesTable` showing the primary agent's `provider/model`. The column cell is a `<ModelPicker>` (extracted from chat-rail in Batch 2) that posts `PATCH /api/strategies/:id/default-agent-model` (new endpoint) on change. Pre-existing strategies that are pre-multi-agent treat their single agent as the primary.

**3.2b — Inspector page:** Add an "Agents" section that lists each agent in the bundle with name + per-agent `<ModelPicker>`. Changing an agent's model posts `PATCH /api/strategies/:id/agents/:agent_id/model`. For single-agent strategies, this section shows one row (matches 3.2a's value). For multi-agent strategies, all agents are editable independently.

**3.2c — Schema audit:** Plan-write needs to verify:

- That each agent in a strategy manifest actually has its own `model` field (vs. one bundle-level model).
- That `RuntimeConfig.default_agent` (Batch 0) is what supplies the model when an agent doesn't override.
- The exact path: strategy manifest → agent → model resolution order.

**Open question:** When a strategy's agents have no explicit model, do they inherit the default agent's model, or is the model required at strategy-creation time? Recommendation: inherit default; allow override. Confirm with user.

**Scope:** Medium-large. Two new endpoints, two new UI sections (list-level and inspector-level), schema audit, and `ModelPicker` extraction. Heavily depends on Batch 0.

---

### Item 3.4 — Inspector affordance from the Strategies page

**User report:** How to get inspector for strategy (maybe need strategy first).

**Investigation finding:** Affordance already exists. `strategies.tsx:115-119, 129-134` — each row has `<Link to={`/authoring/${row.agent_id}`}>` wrapping the agent ID and "Edit →". Clicking the row opens the inspector. The user's confusion is that "Edit →" isn't an obvious "open inspector" affordance.

**Proposed change:** Rename "Edit →" to "Inspector →" (or add an icon button). Cosmetic.

**Scope:** Trivial.

---

### Item 3.5 — Bundle naming UX

**User report:** Bundle naming is odd. Use something more UX friendly.

**Where it lives today:**

- `StrategiesTable` (strategies.tsx:97-141) shows `agent_id` (a ULID like `01KRBS2FR61F91HZ57AP62RV0W`) and `template`. No human-readable name.
- The manifest has a `display_name` field (used at `authoring.tsx:139`).

**Proposed change:**

- Add a "Name" column sourced from `manifest.display_name`. Make it the primary column; demote the ULID to a secondary monospace subtitle row beneath the name (or hide behind a hover).
- On create (Item 3.1a), prompt for a name as the first form field. Save as `display_name`.
- Confirm in plan-write whether `listStrategies()` (`frontend/web/src/api/strategies.ts`) returns `display_name`; if not, surface it from the backend.

**Scope:** Small.

---

## Surface 4 — Eval page (`/eval-runs`)

### Item 4.1 — No way to start an eval

**User report:** No way to start an eval.

**Where it lives today:**

- Route + component: `frontend/web/src/routes/eval-runs.tsx`.
- Empty state at lines 283-296 — directs user to `xvn ab-compare`; no button.
- CLI entry: `xvn eval run --strategy <id> --scenario <id> --mode {paper|backtest}` at `crates/xvision-cli/src/commands/eval.rs:59-75`.
- Engine API: `eval::run()` at `crates/xvision-engine/src/api/eval.rs:414-434` — **blocks synchronously on the executor** (3-10+ minutes).
- Dashboard routes: only GET endpoints (`crates/xvision-dashboard/src/server.rs:30-32`). No `POST /api/eval/runs`.

**Root issue:** The eval executor blocks the request. A `POST /api/eval/runs` calling `eval::run()` directly would tie up an HTTP connection for minutes.

**Proposed change:**

- Refactor `eval::run()` into two functions:
  - `eval::start(req) -> RunId` — insert the run row with status `Queued`, return immediately.
  - Background task spawned via `tokio::spawn` — drives the executor; writes status updates and final metrics back to the run row.
- New endpoint `POST /api/eval/runs` in `crates/xvision-dashboard/src/routes/eval_runs.rs`, calling `eval::start()` and returning `{ run_id, status: "queued" }` immediately.
- Frontend: a "Start eval" button on the eval-runs topbar opens a modal:
  - Strategy dropdown — fetched from `/api/strategies` (exists).
  - Scenario dropdown — fetched from `/api/eval/scenarios` (verify or build — see `crates/xvision-engine/src/api/eval.rs:603+`).
  - Mode radio — Paper / Backtest.
  - "Start" button → `POST /api/eval/runs` → redirect to `/eval-runs/:id` (verify detail-route).
- The existing TanStack Query for the runs list will auto-refetch and show the new row transitioning `Queued → Running → Complete`.

**Open question:** SSE live progress vs. polling? Recommendation: poll every 5s for v1; add SSE in a follow-up.

**Scope:** Medium-large. Background-task refactor + new endpoint + scenario catalog endpoint + frontend modal.

---

## Resolved questions from QA review

These were open at end of Pass 1 and have been resolved by user feedback:

| # | Question | Resolution |
|---|---|---|
| 1 | Default-provider concept: remove vs. keep | **Rework** into "default agent" with provider + model (Batch 0). Item 1.1 follows that rework. |
| 2 | Connections page restructure (fold Brokers in?) | **No.** Keep Brokers as separate tab. Will expand later. |
| 3 | Model dropdown: dedup vs. relabel | **No dedup.** Preserve provider model lists verbatim. Fix the React key, add provider to label. |
| 4 | Strategies page model picker interpretation | **Both.** Per-strategy default AND per-agent overrides for multi-agent strategies. |
| 5 | Chat-rail 404 handling | **Root-cause it.** Server-side scope-keyed session resolution, no localStorage cache. |
| 6 | Wizard provider/model fallback | **Wizard reads default agent explicitly.** No more `undefined` reliance. |

---

## Remaining open questions for plan-write

1. **Default-agent endpoint shape (Batch 0 dependency):** What exact path/method does the rework owner pick? `GET/PUT /api/settings/default-agent`? Need to coordinate before Surface 1 and Surface 3 plans.
2. **Wizard + template-picker coexistence:** `/setup` (chat) and `/strategies/new` (form) both as authoring paths, or does form supersede chat? *(Recommendation: coexist.)*
3. **Composer element identity (Item 2.2):** confirm whether `Composer` is `<input>` or `<textarea>` once we read `ChatRail.tsx:480-525`.
4. **Markdown streaming behavior (Item 2.4):** debounce 100ms vs. render-on-complete. *(Recommendation: debounce.)*
5. **Strategy agent schema (Item 3.2c):** confirm each agent has its own `model` field in the manifest and the resolution order when an agent doesn't override.
6. **Default model inheritance for agents (Item 3.2):** inherit from default agent, or require explicit per-agent on creation?
7. **Eval start streaming (Item 4.1):** poll vs. SSE.
8. **Chat-rail session-resolver scope (Item 3.1b):** keep `updateScope` for any case where session lifetime needs to outlive scope changes, or fully retire it?

---

## Out of scope

- Batch 0 itself (intern → default-agent rework) — owned by separate workstream; this spec consumes it.
- Marketplace, NFT mint, smart-contract surface — separate plans.
- Adding more LLM providers beyond what's already wired.
- Backend session GC for `chat_sessions` table — once Item 3.1b lands, stale rows can be left alone or GC'd in a follow-up.
