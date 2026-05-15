# XVN Mobile-First Framework - Design

> **Status:** Draft / spec. Drafted 2026-05-14.
> **Author:** xvision team.
> **Prototype source:** `docs/design/XVN-handoff.zip`, extracted to `docs/design/xvn/`.
> **Primary prototype file:** `docs/design/xvn/project/xvn mobile design.html`.
> **Companion specs:** [Chat Rail Inline Charting](./2026-05-14-chat-rail-inline-charting-design.md) | [TradingView Charts Design](./2026-05-11-tradingview-charts-design.md) | [TradingView Lightweight Eval Surface](./2026-05-14-tradingview-lightweight-eval-surface-design.md) | [QA Pass 2](./2026-05-11-qa-pass-2-spec.md)

---

## 1. Purpose

The mobile framework prototype reframes xvision around a single interaction rule:

**On mobile, chat is the primary workspace.**

The user should not have to start from the route hierarchy. They should be able to ask for current P&L, inspect an eval, compare runs, draft a variant, or pause a paper deployment from one composer. The same data surfaces still exist as real routes, but on phones they are reached from chat and keep chat available as a floating pill.

This spec turns the exported prototype into implementation guidance for the production React SPA in `frontend/web/`.

---

## 2. Source Inventory

Prototype files reviewed:

| File | Role |
|---|---|
| `docs/design/xvn/project/xvn mobile design.html` | Primary design canvas and artboard order. |
| `docs/design/xvn/project/mobile-shared.jsx` | Mobile primitives: `MiniChart`, chat message shells, rich cards, top bar, quick rail, composer. |
| `docs/design/xvn/project/mobile-chat.jsx` | Phone chat home, active eval conversation, nav drawer, all-functions sheet. |
| `docs/design/xvn/project/mobile-eval.jsx` | Phone eval runs list and run detail screens, including floating chat pill. |
| `docs/design/xvn/project/mobile-responsive.jsx` | Tablet split and desktop three-pane responsive patterns. |
| `docs/design/xvn/project/mobile-styles.css` | Mobile layout, card, drawer, sheet, dashboard, and responsive shell CSS. |
| `docs/design/xvn/project/styles.css` | Folio dark tokens, type, tables, cards, buttons, sidebar primitives. |
| `frontend/MOBILE.md` | Existing mobile strategy doc. This spec replaces its broad layout recommendation with the concrete prototype system, while keeping its web/PWA constraints. |
| `frontend/web/src/components/shell/ChatRail.tsx` | Current desktop chat rail implementation. It is text/tool oriented and hidden below `xl`. |
| `frontend/web/src/components/shell/Layout.tsx` | Current desktop shell: sidebar, main, right chat rail. |
| `frontend/web/src/api/chat_rail.ts` | Current chat rail API wrapper and `ContentBlock` union. |

---

## 3. Locked Decisions

| # | Decision |
|---|---|
| 1 | **Phone layout is chat-first.** At `<768px`, the chat thread is the default landing surface and fills the viewport. |
| 2 | **No mobile tab bar for primary IA.** Navigation lives in the hamburger drawer; actions live in the `+` all-functions sheet. |
| 3 | **Dashboards become full-screen routes on phone.** Eval list, run detail, strategy detail, settings, and authoring occupy the viewport. Chat collapses to a floating pill, not a persistent rail. |
| 4 | **Tablet uses a two-column split.** At `>=768px`, chat docks left at 360px and the active dashboard fills the right side. |
| 5 | **Desktop keeps explicit IA.** At `>=1280px`, use three columns: nav 220px, main content, chat rail 380px. This is a revised desktop target; current production uses 200px nav and 360px rail. |
| 6 | **Rich agent responses are first-class content blocks.** The chat thread renders charts, run cards, strategy cards, action confirmations, chips, and follow-up affordances inline. |
| 7 | **The all-functions sheet is the mobile command palette.** It must expose every major action the desktop command palette exposes, grouped for touch. |
| 8 | **One component family, multiple placements.** Rich cards render inline in chat, in pinned/status contexts, and in dashboard previews with density changes instead of separate designs. |
| 9 | **Use Folio dark tokens.** Keep warm black surfaces, gold accent, Cormorant display type, Inter UI type, JetBrains Mono for IDs/numerics. |
| 10 | **Do not fork the backend for mobile.** Same routes, same chat sessions, same `ContextScope`, same engine API. Mobile is a responsive layer over the SPA. |

---

## 4. Responsive Shell

### 4.1 Phone: 390 x 844 baseline

Prototype artboards target iPhone 15 Pro logical dimensions inside an iOS frame. Production should implement responsive CSS, not fixed artboards, but the 390px width is the design baseline.

Phone shell:

```
+-----------------------------+
| top app bar                 |
+-----------------------------+
|                             |
| chat thread or active route |
|                             |
+-----------------------------+
| quick rail                  |
+-----------------------------+
| composer                    |
+-----------------------------+
```

Rules:

- Chat home uses `MobileTopBar`, thread, quick rail, and composer.
- Dashboard routes use route-specific top bars and replace the full thread area.
- Dashboard routes reserve bottom space for `ChatPill`.
- Drawer and sheet dim the underlying screen with a blurred overlay.
- Thread and dashboard areas scroll independently from the composer.

### 4.2 Tablet: 768 x 1024 baseline

```
+------------------+----------------------------+
| chat rail 360px  | dashboard context          |
|                  |                            |
| composer bottom  | route content              |
+------------------+----------------------------+
```

Rules:

- Chat remains visible; no drawer/sheet overlay is needed for the core split.
- The left chat column uses the same message and card components as phone.
- The right panel uses tablet-density dashboards, not mobile stacked cards when width allows KPI rows.

### 4.3 Desktop: 1280 x 800 baseline

```
+----------+-------------------------+--------------+
| nav 220  | main dashboard          | chat 380     |
+----------+-------------------------+--------------+
```

Rules:

- Desktop remains explicit: sidebar navigation, main route, persistent chat.
- The chat rail is promoted from optional/collapsed to a permanent column in the prototype target.
- The current production collapsed 44px rail can remain as a user preference, but the design target is visible chat.

---

## 5. Mobile UI Anatomy

### 5.1 Top App Bar

Phone chat:

- Left: hamburger icon.
- Center: brand `xvn` or context chip such as `eth-mr-v3`.
- Right: pulse/status icon with unread dot.

Dashboard:

- Back or menu affordance depending on route depth.
- Route title or breadcrumb.
- Secondary actions as icon buttons, e.g. filters, branch/draft.

Implementation note: use the existing `Icon` primitive or lucide equivalents if the icon system changes. Buttons must be 36px minimum in prototype, 44px target for touch-heavy routes when space allows.

### 5.2 Chat Thread

Message structure:

- Day/time divider.
- User bubble aligned right, max width about 82%.
- Agent message aligned left with `x` avatar, name, timestamp, text, cards, and chips.
- Inline rich cards sit inside the agent message column and inherit available width.

Initial phone home content:

- Morning summary.
- Combined equity chart card.
- KPI strip: P&L, Sharpe, Trades.
- Follow-up chips: open run, compare, draft variant.

Active eval conversation:

- User asks about a run.
- Agent returns run summary, equity chart card, finding summary, and action chips.

### 5.3 Rich Cards

Cards in the prototype:

| Card | Use |
|---|---|
| `ChatChartCard` | Equity, compare, paper P&L, inline chart with KPI strip. |
| `ChatRunListCard` | Ranked run list inside a response. |
| `ChatStrategyCard` | Strategy status, mini equity, tags, open strategy CTA. |
| `ChatActionCard` | Tool/action confirmation such as run started or finding pinned. |

Production should not encode these as ad hoc markdown. They should be typed content blocks or tool-result renderers with explicit payloads.

### 5.4 Quick Rail

The quick rail is a horizontal chip strip above the composer.

Examples:

- Chat home: `Run a backtest`, `Today's P&L`, `Pause eth-mr-v3`, `New strategy`, `Journal`.
- Eval context: `Compare runs`, `Edit strategy`, `Promote to paper`, `Re-run`.
- Dashboard context: chip text should come from `ContextScope.quickReplies()` where possible.

### 5.5 Composer

Prototype composer:

- Left `+` opens all-functions sheet.
- Text field.
- Small bars/tool icon.
- Send icon button.

Production requirements:

- Supports multiline input after v1 if needed, but starts as single-line for mobile density.
- Disabled/streaming state must not resize the composer.
- The `+` action is distinct from send and always reachable.

### 5.6 Drawer

Drawer contents:

- Brand.
- Home, Strategies, Live, Eval, Journal, Data, Settings.
- Counts where useful.
- Conversation history card.
- User row.

Production route status should reflect actual availability. Deferred routes may remain visible with disabled styling only if their target exists or has a clear stub.

### 5.7 All-Functions Sheet

The bottom sheet groups commands:

- Create: new strategy, draft variant, run backtest, journal note.
- Inspect: open run, compare runs, findings library.
- Live: deploy to paper, pause/resume.

Requirements:

- Search icon opens/filter-focuses command search inside the sheet.
- Every item maps to either a route, command palette action, or chat seed.
- Sheet max height around 86% and scrolls internally.

### 5.8 Floating Chat Pill

When the user is inside a full-screen dashboard on phone:

- Pill is fixed near the bottom.
- Contains agent avatar, context-aware placeholder, and send/expand action.
- Tapping the field opens chat as a sheet or swaps back to chat context, preserving route state.

The pill is not just a shortcut; it is the continuity mechanism that keeps the chat-first system coherent when the user drills into a route.

---

## 6. Route Mapping

| Production route | Phone behavior | Tablet behavior | Desktop behavior |
|---|---|---|---|
| `/` | Chat home, not dashboard home. | Chat left + dashboard home right. | Nav + dashboard home + chat rail. |
| `/eval-runs` | Full-screen eval list with chat pill. | Chat left + eval list right. | Nav + eval list + chat rail. |
| `/eval-runs/:id` | Full-screen run detail with chart, findings, ledger, chat pill. | Chat left + run detail right. | Nav + run detail + chat rail. |
| `/eval/compare` | Full-screen compare detail, chat pill. | Chat left + compare right. | Nav + compare + chat rail. |
| `/strategies` | Full-screen strategy list or agent-filtered strategy cards. | Chat left + strategy list right. | Nav + strategy list + chat rail. |
| `/authoring/:id` | Segmented authoring surface: Outline, Editor, Validate. Persistent bottom composer. | Chat left + authoring right. | Existing multi-column authoring, with chat rail. |
| `/setup` | Agent-led setup conversation. | Chat left + setup panels right. | Current setup route with chat rail. |
| `/settings/*` | Full-screen settings forms, chat pill. | Chat left + settings right. | Nav + settings + chat rail. |

---

## 7. Data and API Contracts

### 7.1 Chat Content Blocks

Current `frontend/web/src/api/chat_rail.ts` supports:

```ts
type ContentBlock =
  | { type: "text"; text: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | { type: "tool_result"; tool_use_id: string; content: string };
```

Mobile framework needs typed display blocks in addition to tool traces:

```ts
type RichContentBlock =
  | { type: "inline_chart"; payload: InlineChartPayload }
  | { type: "run_list"; payload: ChatRunListPayload }
  | { type: "strategy_card"; payload: ChatStrategyPayload }
  | { type: "action_card"; payload: ChatActionPayload }
  | { type: "choice_chips"; payload: ChoiceChipPayload };
```

The companion charting spec defines `InlineChartPayload`.

### 7.2 Scope

The existing `ContextScope` model remains correct. Mobile adds stricter expectations:

- Phone chat home starts in `workspace`.
- Dashboard chat pill uses the active route scope.
- Run detail uses `run`.
- Authoring uses `strategy`.
- Compare needs real `run_ids` from query params, not the current empty placeholder.

### 7.3 Session Continuity

Requirements:

- Session resolves by scope as current implementation does.
- Switching from chat to a dashboard and back preserves scroll, route, session, and current composer text where feasible.
- Starting fresh is scoped, not global.

---

## 8. Implementation Direction

### 8.1 Frontend files

Proposed structure:

```
frontend/web/src/components/mobile/
  MobileShell.tsx
  MobileTopBar.tsx
  MobileDrawer.tsx
  MobileFunctionsSheet.tsx
  ChatPill.tsx
  QuickRail.tsx

frontend/web/src/components/chat/
  ChatThread.tsx
  ChatBubble.tsx
  ChatComposer.tsx
  RichContentBlock.tsx
  cards/
    ChatChartCard.tsx
    ChatRunListCard.tsx
    ChatStrategyCard.tsx
    ChatActionCard.tsx

frontend/web/src/components/responsive/
  TabletSplitShell.tsx
  DesktopThreePaneShell.tsx
```

Refactor before adding mobile:

- Split `ChatRail.tsx` into reusable thread/composer/card primitives.
- Move desktop rail width/open state into shell-level responsive logic.
- Stop hard-hiding chat below `xl`; phone needs chat as the root surface.

### 8.2 CSS / Tailwind

Tokens already exist in `frontend/web/src/styles/tokens.css`. Add component classes or Tailwind compositions that map to prototype values:

- `bg`: `#0F0E0C`
- `surface-sidebar`: `#17150F`
- `surface-card`: `#14120E`
- `surface-elev`: `#1B1810`
- `border`: `#2A2618`
- `text`: `#F1ECDD`
- `gold`: `#D4A547`
- `danger`: `#C8443A`

Keep card radii small: 6px to 10px depending on card type. The prototype's mobile chat cards use 10px, base cards use 6px.

### 8.3 Migration sequence

| Milestone | Ships |
|---|---|
| M1 | Extract shared chat primitives from `ChatRail.tsx` without visual regression on desktop. |
| M2 | Add responsive shell: phone chat home, tablet split, desktop three-pane width update behind a feature flag if needed. |
| M3 | Add drawer and all-functions sheet, wired to routes/command actions. |
| M4 | Add phone dashboard behavior: eval list and run detail with floating chat pill. |
| M5 | Add rich content blocks and inline cards from agent responses. |
| M6 | Polish authoring/settings mobile route behavior and route-specific quick replies. |

---

## 9. Acceptance Criteria

Phone:

- At 390px width, `/` renders the chat home without horizontal overflow.
- Composer stays fixed at the bottom and does not cover the last message.
- Hamburger opens a drawer with all route groups.
- `+` opens all-functions sheet, with internally scrollable content.
- `/eval-runs` and `/eval-runs/:id` render full-screen dashboards with a floating chat pill.
- Tapping chat pill returns to or expands chat without losing the current route.

Tablet:

- At 768px width, chat is a 360px left column and route content fills the remaining width.
- No drawer is required for normal navigation, though it may remain accessible.

Desktop:

- At 1280px width, layout has nav, main, and chat columns.
- Existing route functionality remains intact.

Shared:

- Rich cards render from typed payloads, not markdown conventions.
- All text fits at 390px without overlapping or clipped controls.
- Touch targets are at least 36px, with 44px target for primary controls where layout allows.
- Keyboard and screen reader users can operate drawer, sheet, composer, and cards.

---

## 10. Out of Scope

- Native iOS/Android shells.
- System widgets, lockscreen cards, CarPlay, watch surfaces.
- Voice composer.
- Offline-first multi-device sync.
- Replacing the full eval charting implementation. This spec only defines shell and card placement; chart-library decisions live in companion chart specs.

---

## 11. Open Questions

1. Should desktop chat be permanently open by default, or preserve the existing collapsed preference with the prototype as the target state?
2. Should the phone chat pill open a bottom sheet over the dashboard or navigate back to the chat home route with preserved dashboard context?
3. Should `Live` and `Journal` appear enabled in the drawer before those route capabilities are real?
4. Should the all-functions sheet be a visual wrapper over the existing command palette action registry, or should it own a separate mobile command registry?
