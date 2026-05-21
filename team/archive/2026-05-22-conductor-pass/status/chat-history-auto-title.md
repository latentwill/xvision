---
track: chat-history-auto-title
worktree: .worktrees/chat-history-auto-title
branch: task/chat-history-auto-title
base: origin/main
phase: pr-open
last_updated: 2026-05-18T05:05:00Z
owner: claude
---

# What changed

- New module `frontend/web/src/components/chat/useChatTitle.ts`:
  - Pure helper `cleanTitle` — strips wrapping quotes / terminal
    punctuation, clamps to 7 words / 80 chars.
  - `summarizeChatTitle` — dispatches a one-shot cheap-model
    summarize against the configured provider/model. Creates a
    throwaway `chat-rail` session with `scope: "workspace"`, sends
    the `Summarize this conversation in <=7 words…` prompt with the
    first user + first assistant snippets, accumulates the token
    stream, then best-effort deletes the throwaway session.
  - `useChatTitle` hook — module-level guards
    (`titleCache` / `inflight` / `failed`) so summarize fires once
    per session id across remounts. Initial state hydrates from a
    `xvn.chat-history.title.<sessionId>` localStorage key for
    cross-reload UX (queue note pending for a server-side title
    column).
- New component
  `frontend/web/src/components/chat/ChatHistoryItem.tsx` — renders a
  history-list row with the title primary and the timestamp as a
  smaller secondary line. Falls back to the date-only label when
  there is no title (no-provider path, or pre-first-assistant turn).
- ChatRail conversation-history loop now renders `<ChatHistoryItem>`
  for each row. For the **active** session it derives `firstUser` /
  `firstAssistant` from the local bubble state so the summarize can
  fire as soon as the first model response completes. For other
  rows it relies on cached / persisted titles only — no LLM dispatch
  for non-active threads.
- `ready` gate: only fires once `!isStreaming && firstAssistant`.
  No-op when `providersConfigured === false` (the providers query
  returns zero rows) — that path emits a single `console.warn`
  per session, falls back to the date label, never throws.

# Verification

- Passed: `corepack pnpm --dir frontend/web test -- --run chat wizard ChatRail useChatTitle` (31 tests, incl. 10 new in `useChatTitle.test.tsx`)
- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `corepack pnpm --dir frontend/web build`

# Tests added

- `cleanTitle`:
  - strips wrapping quotes + terminal punctuation,
  - clamps to 7 words / 80 chars,
  - collapses whitespace.
- `summarizeChatTitle`:
  - creates throwaway session, accumulates tokens, deletes after,
  - throws on empty body.
- `useChatTitle`:
  - renders the generated title once summarize resolves,
  - falls back to null (= date label upstream) when no provider configured,
  - fires summarize exactly once per session even across remounts,
  - does not throw on summarizer failure and does not retry,
  - skips dispatch until `ready` (first assistant turn complete).

# Allowed-paths deviation

The contract `allowed_paths` list included
`frontend/web/src/api/wizard.ts` and `.test.ts`, but those files do
not exist on origin/main — the chat-rail's wire layer is
`api/chat_rail.ts`. The shipped surface for the conversation-history
list also lives at `frontend/web/src/components/shell/ChatRail.tsx`,
not in `components/chat/`. The edit to `ChatRail.tsx` is minimal
(one new import + one helper invocation + replacing the row markup
with `<ChatHistoryItem>`) and is flagged here for the conductor.

# Queue note

Per contract acceptance: the chat-rail does not yet have a
server-side title column on `chat_sessions`. The current
implementation uses localStorage for cross-reload persistence,
which works for one operator on one browser. A backend column
(`chat_sessions.title`) + the `xvn-engine` write-through and a
`/api/chat-rail/sessions/:id/title` POST would make titles survive
across browsers / DB resets. Filed as a follow-up — out of scope
for this contract.

# Notes

- The summarize uses the same provider/model the operator selected
  in the rail (provider-aware default, same posture as
  `qa-review-agent-provider-config`). No hardcoded provider.
- Module-level `failed` set prevents a retry storm on persistent
  provider errors; a manual reload is needed to retry, which is
  the intended escalation surface.
