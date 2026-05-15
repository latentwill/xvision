# Chat Rail Inline Charting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add typed rich display blocks and custom SVG inline chart cards to the chat rail, so agent responses can render equity, compare, histogram, sparkline, strategy, run-list, and action-confirmation cards inline.

**Architecture:** Keep full eval charting separate. Chat rail inline charts are lightweight React SVG components rendered from stored typed payloads. The backend/tool layer builds and validates rich blocks from domain responses; the model should request domain actions, not hand-author chart JSON.

**Tech Stack:** React 18, TypeScript, Vite, Tailwind CSS, existing chat rail SSE/session API, Rust/serde/ts-rs for shared payload types. No charting npm dependency in v1.

**Reference spec:** `docs/superpowers/specs/2026-05-14-chat-rail-inline-charting-design.md`.

---

## Verification approach

Each frontend task verifies with:

```bash
cd frontend/web && pnpm typecheck
cd frontend/web && pnpm build
```

Backend tasks verify with targeted Rust tests for rich block builders and payload validation. Do not add a frontend test runner in this plan.

Manual browser QA covers:

- Chat history reload preserves cards.
- Cards render at 390px phone and 360px rail widths.
- Keyboard focus reaches primary card actions.
- Card tap opens the canonical route or configured action.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/chat_session/rich_blocks.rs` | Create | Rich block payload structs, builders, validation. |
| `crates/xvision-engine/src/chat_session/mod.rs` | Modify | Export rich block module. |
| `frontend/web/src/api/chat_rail.ts` | Modify | Extend `ContentBlock` with rich display block variants. |
| `frontend/web/src/components/chat/` | Modify | Preserve ordered blocks and render rich display blocks. |
| `frontend/web/src/components/chat/inline-chart/` | Create | SVG inline chart renderers and helpers. |
| `frontend/web/src/components/chat/cards/` | Create | Run list, strategy, and action confirmation cards. |
| `frontend/web/src/components/shell/ChatRail.tsx` | Modify | Use extracted block-aware chat renderer. |

---

## Tasks

### Task 1: Add shared rich block payload types

**Files:**
- Create: `crates/xvision-engine/src/chat_session/rich_blocks.rs`
- Modify: `crates/xvision-engine/src/chat_session/mod.rs`
- Modify: `frontend/web/src/api/chat_rail.ts`

- [ ] **Step 1.1: Define rich block structs in Rust**

Add serializable payloads:

- `InlineChartPayload`
- `InlineChartSeries`
- `InlinePoint`
- `InlineMetric`
- `InlineAction`
- `InlineChartSource`
- `ChatRunListPayload`
- `ChatStrategyPayload`
- `ChatActionPayload`

Use `serde` and `ts-rs` exports following existing generated-type conventions in the repo.

- [ ] **Step 1.2: Define rich content block enum shape**

Support these display block variants:

- `inline_chart`
- `run_list`
- `strategy_card`
- `action_card`
- `choice_chips` if implementation needs explicit assistant-authored chips instead of quick replies.

Keep existing text/tool block variants unchanged for compatibility.

- [ ] **Step 1.3: Add frontend TypeScript variants**

Extend `ContentBlock` in `frontend/web/src/api/chat_rail.ts` with the generated or mirrored rich block variants.

Do not remove existing `text`, `tool_use`, or `tool_result`.

- [ ] **Step 1.4: Verify**

Run:

```bash
cargo test -p xvision-engine chat_session
cd frontend/web && pnpm typecheck
```

---

### Task 2: Add payload validation and downsampling guardrails

**Files:**
- Modify: `crates/xvision-engine/src/chat_session/rich_blocks.rs`
- Add tests in the same module or nearby test module.

- [ ] **Step 2.1: Add validation**

Validation rules:

- `a11y_summary` is required for `inline_chart`.
- Total points per inline chart must be `<= 500`.
- If a payload was downsampled, mark it explicitly with a boolean field.
- `chart_id`, `kind`, `title`, and `series` are required.
- Actions may use SPA `href` values or known command ids only.

- [ ] **Step 2.2: Add builder-side normalization**

Builder helpers should round values enough to keep chat payloads small. Do not mutate source domain data.

- [ ] **Step 2.3: Tests**

Add tests for:

- Valid equity chart payload passes.
- Missing `a11y_summary` fails.
- More than 500 total points fails unless builder downsampling reduces it.
- Invalid action target fails.

Run:

```bash
cargo test -p xvision-engine rich_blocks
```

---

### Task 3: Refactor chat rendering to preserve ordered assistant blocks

**Files:**
- Modify: `frontend/web/src/components/chat/ChatThread.tsx`
- Modify: `frontend/web/src/components/chat/ChatBubble.tsx`
- Modify: `frontend/web/src/components/shell/ChatRail.tsx`

- [ ] **Step 3.1: Replace text-only assistant bubble model**

Current history conversion collapses assistant text blocks and extracts tool chips. Change the render model so assistant bubbles preserve ordered renderable blocks:

```ts
type AssistantBubble = {
  role: "assistant";
  blocks: RenderableBlock[];
  tools: Tool[];
};
```

The order `text -> chart -> text -> chips` must survive session reload.

- [ ] **Step 3.2: Keep streaming token behavior**

During SSE streaming, token events still append to the active assistant text block. Tool calls/results still update the tool chip list.

- [ ] **Step 3.3: Render unknown rich block safely**

Unknown block variants should render a compact unsupported-block notice in development and be hidden or summarized in production. They must not break the thread.

- [ ] **Step 3.4: Verify**

Manual check:

- Existing text-only chat still works.
- Markdown still renders.
- Tool chips still render.
- History reload preserves text/tool state.

---

### Task 4: Build custom SVG inline chart components

**Files:**
- Create: `frontend/web/src/components/chat/inline-chart/InlineChartCard.tsx`
- Create: `frontend/web/src/components/chat/inline-chart/InlineChartSvg.tsx`
- Create: `frontend/web/src/components/chat/inline-chart/InlineHistogram.tsx`
- Create: `frontend/web/src/components/chat/inline-chart/InlineSparkline.tsx`
- Create: `frontend/web/src/components/chat/inline-chart/shape.ts`
- Create: `frontend/web/src/components/chat/inline-chart/palette.ts`
- Create: `frontend/web/src/components/chat/inline-chart/types.ts`

- [ ] **Step 4.1: Implement shape helpers**

Implement helpers for:

- Normalizing points into a fixed viewbox.
- Building line path strings.
- Building area path strings.
- Handling flat lines.
- Handling empty or single-point series.

- [ ] **Step 4.2: Implement chart renderers**

Support:

- `equity`: one area/line primary series.
- `compare`: two to four line series with legend.
- `histogram`: bar distribution.
- `drawdown`: negative area/bar strip.
- `sparkline`: tiny line for list rows/cards.
- `trade_markers`: compact markers over an equity or event strip.

- [ ] **Step 4.3: Implement `InlineChartCard`**

Card anatomy:

- Title/subtitle.
- Primary signed value when provided.
- Chart SVG.
- KPI metric strip.
- Footer action(s).
- Screen-reader summary.

Use Folio dark tokens: gold, danger, warn, info, muted text.

- [ ] **Step 4.4: Verify**

Create local fixture payloads inside Story-like dev-only examples or a temporary route section if the repo already has a fixture pattern. Do not commit throwaway debug UI.

Manual check cards at:

- 390px phone width.
- 360px rail width.
- 320px narrow fallback width.

---

### Task 5: Add non-chart rich cards

**Files:**
- Create: `frontend/web/src/components/chat/cards/ChatRunListCard.tsx`
- Create: `frontend/web/src/components/chat/cards/ChatStrategyCard.tsx`
- Create: `frontend/web/src/components/chat/cards/ChatActionCard.tsx`
- Create: `frontend/web/src/components/chat/ContentBlockView.tsx`

- [ ] **Step 5.1: Implement run list card**

Render ranked runs with:

- Rank.
- Run id.
- Strategy/scenario subtitle.
- Return and Sharpe.
- Optional tiny sparkline.
- Footer action to view all or open list.

- [ ] **Step 5.2: Implement strategy card**

Render:

- Strategy name.
- State/source tags.
- P&L or primary metric.
- Optional sparkline.
- CTA to open strategy.

- [ ] **Step 5.3: Implement action card**

Render:

- Status icon.
- Title.
- Subtitle.
- Primary CTA.

Use this for confirmations such as run started, finding pinned, pause/resume, or draft created.

- [ ] **Step 5.4: Wire `ContentBlockView`**

Switch on block type:

- `text`: markdown renderer.
- `inline_chart`: `InlineChartCard`.
- `run_list`: `ChatRunListCard`.
- `strategy_card`: `ChatStrategyCard`.
- `action_card`: `ChatActionCard`.
- `choice_chips`: chip row if implemented.

- [ ] **Step 5.5: Verify**

Manual check:

- Mixed text/card/text assistant message renders in order.
- Cards fit at phone and rail widths.
- Footer links/actions are keyboard reachable.

---

### Task 6: Add backend rich block builders

**Files:**
- Modify: `crates/xvision-engine/src/chat_session/rich_blocks.rs`
- Modify chat/tool integration files that currently create assistant tool results, as discovered during implementation.

- [ ] **Step 6.1: Add builder helpers**

Add helpers:

- `inline_equity_chart_from_run_detail`
- `inline_compare_chart_from_report`
- `inline_returns_histogram_from_runs`
- `inline_strategy_card_from_summary`
- `action_confirmation_card`

Builders return validated rich blocks and never return unvalidated JSON.

- [ ] **Step 6.2: Keep model responsibilities narrow**

The model/tool loop may choose an action, but backend helpers construct the final rich block payload from domain data.

- [ ] **Step 6.3: Tests**

Add tests for:

- Run detail produces equity chart with metrics.
- Compare report produces compare chart with two series.
- Run summaries produce histogram/run-list cards.
- Action confirmation creates a valid card.

Run:

```bash
cargo test -p xvision-engine rich_blocks
```

---

### Task 7: Integrate rich blocks into chat tool responses

**Files:**
- Modify chat rail or wizard loop integration files that serialize assistant messages.
- Modify frontend stream/history handling if rich blocks arrive during SSE.

- [ ] **Step 7.1: Emit rich blocks for eval/run intents**

When a chat tool response resolves:

- Last run detail: emit text summary plus `inline_chart`.
- Compare runs: emit text summary plus compare `inline_chart`.
- Top/recent runs: emit `run_list`.
- Strategy status: emit `strategy_card`.
- Mutating action success: emit `action_card`.

- [ ] **Step 7.2: Preserve storage compatibility**

Store rich blocks inside `chat_messages.content_blocks_json` alongside existing text/tool blocks. Existing old messages must still load.

- [ ] **Step 7.3: SSE behavior**

If rich blocks stream after a tool result, append them to the active assistant bubble without requiring a full history reload.

- [ ] **Step 7.4: Verify**

Manual check:

- Ask about last eval run and see an equity card.
- Ask to compare two runs and see a compare card.
- Reload the page and confirm the card persists.
- Click primary card action and confirm canonical route opens.

---

### Task 8: Accessibility and performance pass

- [ ] **Step 8.1: Accessibility**

Ensure every card has:

- `role="group"` or equivalent semantic wrapper.
- `aria-label` or screen-reader text from `a11y_summary`.
- Keyboard focus for primary action.
- Visible text values that do not rely on color alone.

- [ ] **Step 8.2: Performance**

Ensure:

- One line/area path per series, not one node per point.
- Stable gradient ids derived from `chart_id`.
- No `Math.random()` ids.
- Heavy below-fold card rendering can be deferred if manual testing shows scroll hitching.

- [ ] **Step 8.3: Final verification**

Run:

```bash
cargo test -p xvision-engine rich_blocks
cd frontend/web && pnpm typecheck
cd frontend/web && pnpm build
```

Manual matrix:

| Case | Expected |
|---|---|
| Empty thread | Existing empty state works. |
| Text-only history | Renders unchanged. |
| Mixed rich history | Blocks render in stored order. |
| 10 chart cards in thread | Scroll remains smooth. |
| 390px phone | No card horizontal overflow. |
| 360px desktop rail | No card horizontal overflow. |

---

## Out of scope

- TradingView Lightweight Charts inside chat messages.
- Full eval chart replacement.
- Inline pinch/zoom, brush selection, or marker selection.
- New frontend test runner.
- Model-authored arbitrary chart JSON without backend validation.
