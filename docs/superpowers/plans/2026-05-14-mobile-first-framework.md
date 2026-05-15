# XVN Mobile-First Framework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the mobile-first framework from `docs/superpowers/specs/2026-05-14-mobile-first-framework-design.md`: phone chat-as-home, hamburger drawer, all-functions sheet, floating chat pill on dashboard routes, tablet split layout, and desktop three-pane target.

**Architecture:** Refactor the current desktop-only `ChatRail` into reusable chat primitives, then add a responsive shell that chooses phone, tablet, or desktop composition from viewport width. Keep the same React SPA, same engine API, same chat session endpoints, and same `ContextScope` model. Mobile adds layout and component placement; it does not fork backend behavior.

**Tech Stack:** React 18, TypeScript, Vite, Tailwind CSS, TanStack Query, existing `zustand` UI store, existing `Icon` primitive and Folio dark tokens.

**Reference spec:** `docs/superpowers/specs/2026-05-14-mobile-first-framework-design.md`.

---

## Verification approach

Each task verifies with:

1. `cd frontend/web && pnpm typecheck`
2. `cd frontend/web && pnpm build`
3. Manual browser verification at:
   - 390 x 844 phone viewport.
   - 768 x 1024 tablet viewport.
   - 1280 x 800 desktop viewport.

Do not add a frontend test runner in this plan. Use manual responsive QA plus typecheck/build.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `frontend/web/src/components/shell/ChatRail.tsx` | Refactor | Keep desktop orchestration, extract reusable thread/composer pieces. |
| `frontend/web/src/components/shell/Layout.tsx` | Modify | Route through responsive shell composition. |
| `frontend/web/src/components/chat/` | Create | Shared chat thread, bubble, composer, quick rail, block renderers. |
| `frontend/web/src/components/mobile/` | Create | Mobile shell, top bar, drawer, functions sheet, chat pill. |
| `frontend/web/src/components/responsive/` | Create | Tablet split and desktop three-pane layout wrappers if keeping shell code separate. |
| `frontend/web/src/api/chat_rail.ts` | Modify | Fix route-derived scopes, especially compare run ids. |
| `frontend/web/src/stores/ui.ts` | Modify | Drawer, functions sheet, and mobile chat-pill state. |
| `frontend/web/src/styles/globals.css` | Modify | Shared mobile utility classes only where Tailwind classes become repetitive. |

---

## Tasks

### Task 1: Extract reusable chat primitives without changing desktop behavior

**Files:**
- Modify: `frontend/web/src/components/shell/ChatRail.tsx`
- Create: `frontend/web/src/components/chat/ChatThread.tsx`
- Create: `frontend/web/src/components/chat/ChatBubble.tsx`
- Create: `frontend/web/src/components/chat/ChatComposer.tsx`
- Create: `frontend/web/src/components/chat/QuickRail.tsx`

- [ ] **Step 1.1: Move pure render code out of `ChatRail.tsx`**

Extract `Thread`, `BubbleView`, `MarkdownView`, `TypingDots`, `QuickReplies`, and `Composer` into the new `components/chat/` files. Keep the existing props and visual classes.

`ChatRail.tsx` remains responsible for:

- Deriving `ContextScope`.
- Resolving session id/history.
- Streaming chat events.
- Provider/model selection.
- Start-fresh behavior.
- Open/collapse state on desktop.

- [ ] **Step 1.2: Preserve desktop output**

After extraction, the expanded and collapsed desktop rail should render exactly as before:

- Collapsed rail remains `44px`.
- Expanded rail remains `360px`.
- Model picker remains inside desktop rail.
- Thread scroll and composer behavior stay unchanged.

- [ ] **Step 1.3: Verify**

Run:

```bash
cd frontend/web && pnpm typecheck
cd frontend/web && pnpm build
```

Manual check:

- At desktop width, open/collapse the rail.
- Send a chat message.
- Confirm markdown, tool chips, typing indicator, quick replies, and composer still work.

---

### Task 2: Add shell-level responsive layout

**Files:**
- Modify: `frontend/web/src/components/shell/Layout.tsx`
- Create: `frontend/web/src/components/mobile/MobileShell.tsx`
- Create: `frontend/web/src/components/responsive/TabletSplitShell.tsx`
- Create: `frontend/web/src/components/responsive/DesktopThreePaneShell.tsx`

- [ ] **Step 2.1: Add viewport-based shell selection**

Implement CSS breakpoint composition:

- `<768px`: `MobileShell`.
- `>=768px` and `<1280px`: `TabletSplitShell`.
- `>=1280px`: `DesktopThreePaneShell`.

Prefer CSS/Tailwind responsive classes over JavaScript resize listeners unless route state requires JS.

- [ ] **Step 2.2: Desktop shell**

Desktop target:

- Sidebar column: 220px.
- Main content: flexible.
- Chat column: 380px target.

Keep current collapsed rail behavior as a user preference: if the rail is collapsed, the desktop shell may show the 44px strip instead of the full 380px column.

- [ ] **Step 2.3: Tablet shell**

Tablet target:

- Left chat column: 360px.
- Right route column: flexible.
- No sidebar column.

The chat column reuses `ChatThread`, `QuickRail`, and `ChatComposer`. It should include a compact top bar and context label.

- [ ] **Step 2.4: Phone shell**

Phone target:

- `/` renders chat home as the primary surface.
- Non-home routes render full-screen route content with a floating chat pill.
- No desktop sidebar or right rail is mounted at phone widths.

- [ ] **Step 2.5: Verify**

Run typecheck/build. Manually inspect `/`, `/eval-runs`, and `/eval-runs/:id` at 390, 768, and 1280 widths.

---

### Task 3: Build mobile top bar, drawer, and all-functions sheet

**Files:**
- Create: `frontend/web/src/components/mobile/MobileTopBar.tsx`
- Create: `frontend/web/src/components/mobile/MobileDrawer.tsx`
- Create: `frontend/web/src/components/mobile/MobileFunctionsSheet.tsx`
- Modify: `frontend/web/src/stores/ui.ts`

- [ ] **Step 3.1: Add UI store state**

Add state/actions for:

- `mobileDrawerOpen`
- `setMobileDrawerOpen`
- `mobileFunctionsOpen`
- `setMobileFunctionsOpen`

Do not persist this state to localStorage.

- [ ] **Step 3.2: Implement `MobileTopBar`**

Phone chat top bar:

- Left hamburger opens `MobileDrawer`.
- Center shows `xvn` or a context chip.
- Right pulse/status icon uses the existing `Icon` primitive and can show the unread dot visually.

Dashboard top bar:

- Back/menu affordance based on route depth.
- Route title or run id.
- Optional icon actions such as filters/branch.

- [ ] **Step 3.3: Implement `MobileDrawer`**

Drawer contents:

- Brand.
- Home, Strategies, Eval, Data, Settings.
- Live and Journal may appear disabled unless their routes are real in the current build.
- Conversation history card.
- User row.

Clicking a route closes the drawer.

- [ ] **Step 3.4: Implement `MobileFunctionsSheet`**

Bottom sheet groups:

- Create: new strategy, draft variant, run backtest, journal note.
- Inspect: open a run, compare runs, findings library.
- Live: deploy to paper, pause/resume.

Map implemented actions to existing routes or command palette actions. Disabled/deferred actions stay visible only if they have clear disabled styling.

- [ ] **Step 3.5: Verify**

At 390px:

- Hamburger opens/closes drawer.
- Route selection closes drawer.
- Composer `+` opens/closes sheet.
- Sheet scrolls internally and never pushes composer off-screen.
- Escape key closes drawer/sheet on desktop browser.

---

### Task 4: Add mobile chat home and quick rail behavior

**Files:**
- Modify: `frontend/web/src/components/mobile/MobileShell.tsx`
- Modify: `frontend/web/src/components/chat/QuickRail.tsx`
- Modify: `frontend/web/src/api/chat_rail.ts`

- [ ] **Step 4.1: Render phone chat home on `/`**

At phone width, `/` should show:

- `MobileTopBar`.
- Chat thread for workspace scope.
- Quick rail above composer.
- Composer at bottom.

Use real chat history when available. If empty, show a compact empty state that invites the user to ask xvn about the workspace; do not hardcode prototype sample data in production.

- [ ] **Step 4.2: Reuse route quick replies**

Use `quickReplies(scope)` from `chat_rail.ts` for quick rail chips. Add mobile-specific defaults only when the scope returns no replies.

- [ ] **Step 4.3: Fix compare scope derivation**

Update `scopeFromPath` or the caller so compare route scope carries selected run ids from the URL query when available. The resulting scope should be:

```ts
{ scope: "compare", run_ids: ["..."] }
```

If no ids are present, use an empty compare scope and let the UI show generic compare quick replies.

- [ ] **Step 4.4: Verify**

Manual checks:

- Phone `/` opens in chat mode.
- Quick rail chips submit messages.
- Compare route shows compare-specific context label and quick replies.
- Desktop rail behavior is unchanged.

---

### Task 5: Add dashboard chat pill on phone routes

**Files:**
- Create: `frontend/web/src/components/mobile/ChatPill.tsx`
- Modify: `frontend/web/src/components/mobile/MobileShell.tsx`
- Modify: route wrappers as needed for bottom padding.

- [ ] **Step 5.1: Implement `ChatPill`**

The pill contains:

- Agent avatar.
- Context-aware placeholder.
- Send/expand icon.

On tap, open chat in a mobile overlay/sheet over the current route. Preserve the route and route scroll position.

- [ ] **Step 5.2: Mount pill on phone dashboard routes**

Show the pill on phone widths for:

- `/eval-runs`
- `/eval-runs/:id`
- `/eval/compare`
- `/strategies`
- `/authoring/:id`
- `/settings/*`

Do not show it on phone `/` because chat is already the primary surface.

- [ ] **Step 5.3: Add bottom-safe padding**

Dashboard content must reserve enough bottom padding so the pill does not cover final cards, table rows, or action buttons.

- [ ] **Step 5.4: Verify**

At 390px:

- `/eval-runs` and `/eval-runs/:id` have visible route content and a pill.
- Last content remains reachable above the pill.
- Tapping the pill opens chat and closing chat returns to the same route.

---

### Task 6: Tablet split polish

**Files:**
- Modify: `frontend/web/src/components/responsive/TabletSplitShell.tsx`
- Modify: route container classes only where needed.

- [ ] **Step 6.1: Dock chat left at tablet widths**

At 768px:

- Left chat column is 360px.
- Right route column is `minmax(0, 1fr)`.
- Both columns scroll independently.

- [ ] **Step 6.2: Hide desktop sidebar and right rail**

Tablet layout should not mount the desktop sidebar or the desktop right rail.

- [ ] **Step 6.3: Verify**

Manual checks at 768 x 1024:

- `/` shows chat plus dashboard context.
- `/eval-runs/:id` shows chat plus run detail.
- Composer remains visible in the left column.
- Right column has no horizontal overflow.

---

### Task 7: Final responsive QA pass

- [ ] **Step 7.1: Run verification commands**

```bash
cd frontend/web && pnpm typecheck
cd frontend/web && pnpm build
```

- [ ] **Step 7.2: Manual acceptance matrix**

Check these combinations:

| Viewport | Routes |
|---|---|
| 390 x 844 | `/`, `/eval-runs`, `/eval-runs/:id`, `/settings/providers` |
| 768 x 1024 | `/`, `/eval-runs/:id`, `/strategies` |
| 1280 x 800 | `/`, `/eval-runs`, `/eval-runs/:id` |

Acceptance:

- No horizontal overflow.
- Composer is always reachable where chat is visible.
- Drawer and sheet do not trap the app after close.
- Chat pill does not cover final route content.
- Desktop route functionality remains intact.

---

## Out of scope

- Native mobile app shell.
- Web Push or notification channels.
- Inline chart/rich-block payload implementation; that is owned by `2026-05-14-chat-rail-inline-charting.md`.
- TradingView full eval chart implementation; owned by TradingView chart plans.
