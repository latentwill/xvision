# chat-rail-strategy-list-refresh — status

**Contract:** `team/contracts/chat-rail-strategy-list-refresh.md`
**Branch:** `task/chat-rail-strategy-list-refresh`
**Worktree:** `.worktrees/chat-rail-strategy-list-refresh`
**Claimed:** 2026-05-18
**Status:** in-progress

## Audit

Grepped `frontend/web/src/components/chat/**` and
`frontend/web/src/components/shell/ChatRail.tsx` for
`invalidateQueries` / `useQueryClient` — **zero hits.** The chat rail
mutates server state via wizard tool calls but TanStack Query is
never told the cache went stale; the operator only sees the new row
after a hard refresh.

Wizard tool registry (`crates/xvision-dashboard/src/wizard_loop.rs`
446–541) lists the tools below. The fix adds an
`invalidateForToolResult(qc, ev)` helper called inside the streaming
loop in `ChatRail::send`. The mapping it implements:

| Tool name | Mutates | Query key invalidated | Test name |
|---|---|---|---|
| `create_strategy` | strategies list | `strategyKeys.all` | `invalidates the strategies list on create_strategy` |
| `create_strategy_agent` | strategies + agents | `strategyKeys.all`, `agentKeys.all` | `invalidates BOTH strategies and agents on create_strategy_agent` |
| `attach_agent` | strategies | `strategyKeys.all` | parameterized `it.each` row `attach_agent` |
| `update_slot` | strategies | `strategyKeys.all` | parameterized row `update_slot` |
| `update_manifest` | strategies | `strategyKeys.all` | parameterized row `update_manifest` |
| `set_mechanical_param` | strategies | `strategyKeys.all` | parameterized row `set_mechanical_param` |
| `set_risk_config` | strategies | `strategyKeys.all` | parameterized row `set_risk_config` |
| `create_scenario` | scenarios list | `scenarioKeys.all` | `invalidates the scenarios list on create_scenario` |
| `run_eval` | eval-runs list | `evalKeys.all` | `invalidates the eval list on run_eval` |
| `validate_draft` | (read-only) | — | `ignores read-only validate_draft` |

Negative-path coverage:
- Non-`tool_result` events (`token`, `tool_call`, `done`) → no
  invalidation (`ignores non-tool_result events`).
- Failed tool results (`{error: "..."}`) → no invalidation
  (`ignores failed tool results (no mutation happened)`).
- Unknown tool names → no invalidation, conservative-by-default
  (`ignores unknown tools (new mutating tools must opt in
  explicitly)`).

## Path correction

Initial contract scoped `allowed_paths` to
`frontend/web/src/components/chat/**`, but the chat rail's SSE-event
consumer lives in `frontend/web/src/components/shell/ChatRail.tsx::applyEvent`.
That's where `tool_result` events are processed, so that's where
invalidation has to hook in. Contract updated in the same PR to add
`ChatRail.tsx` + its test file + `api/chat_rail.ts` to allowed_paths,
plus `api/eval.ts` (the contract listed `api/eval-runs.ts` but the
query-keys live at `api/eval.ts::evalKeys`).

## Verification

- `npm run typecheck` — clean.
- `npm test -- --run ChatRail` — 18/18 pass (5 existing ChatRail
  tests + 13 new `invalidateForToolResult` cases).
- Full repo: `npm test -- --run` — 1 pre-existing failure in
  `RunChart.test.tsx > persists layer toggles to localStorage`,
  unrelated to this PR (reproduced on `origin/main` with my diff
  stashed).

## Out-of-scope confirmations

- No optimistic updates / cache patching.
- No new server-side push channel.
- No backend changes.
- Delete tools aren't currently exposed by the chat rail wizard
  (only list-row delete actions, which already invalidate on
  their own mutation handlers).

## Checkpoints

- 2026-05-18 — worker branch created, audit complete (zero
  invalidations exist today), path correction applied to contract.
- 2026-05-18 — `invalidateForToolResult` helper landed in
  `ChatRail.tsx`; wired into the streaming loop in `send`.
- 2026-05-18 — 13 tests added (parameterized over each mutating
  tool + negative-path coverage).
- 2026-05-18 (follow-up, this PR) — operator regression resurfaced
  for tool results that ship `error: null` (Rust `Option<String>`
  serde default). The old failed-detection
  (`"error" in result`) bailed on success payloads with a `null`
  error field, so the invalidator no-op'd on a successful
  `create_strategy` whose serialized payload happened to include
  `{"id": "01...", "error": null}`.

  Fix: require a TRUTHY error value (`Boolean(result.error)`) — the
  wizard loop emits `{"error": "<msg>"}` on real failure, so
  truthiness is enough to discriminate.

  Regression test added:
  `does NOT treat success payloads with error: null / error: ""`.

## PR

#275 (follow-up): tighten failed-result detection to truthy `error`.
