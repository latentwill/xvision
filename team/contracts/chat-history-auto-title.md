---
track: chat-history-auto-title
lane: leaf
wave: qa-operator-2026-05-18-r3
worktree: .worktrees/chat-history-auto-title
branch: task/chat-history-auto-title
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/components/chat/**
  - frontend/web/src/components/agent-chat/**
  - frontend/web/src/stores/wizard-thread*.ts
  - frontend/web/src/api/wizard.ts
  - frontend/web/src/api/wizard.test.ts
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/src/features/eval-runs/**
  - frontend/web/src/features/agent-runs/**
interfaces_used:
  - wizard dispatch API (existing thread/message endpoints)
  - useWizardThread / chat history store
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run chat wizard
  - pnpm --dir frontend/web build
acceptance:
  - The conversation-history list (chat rail / wizard surface) shows a
    3-7 word title per thread instead of only the timestamp. The
    timestamp remains as a smaller secondary line for sort/disambig.
  - Titles are generated **once** after the first model response in a
    new thread by dispatching a cheap-model `summarize this
    conversation in <=7 words, no quotes, no terminal punctuation`
    prompt against whichever provider has a model configured
    (provider-aware default, same pattern as the review-agent fix in
    `qa-review-agent-provider-config`). If no provider is configured
    the list falls back to the date-only label and emits a single
    `tracing::warn!` — no popup, no error toast.
  - Titles persist for the lifetime of the thread. Re-summarization
    is explicitly out of scope for v1 — title-once-and-stick. A
    follow-up issue may add a manual "Rename" affordance.
  - If the conversation table does not exist server-side today
    (chat history is currently client-only), the worker files a
    queue note rather than building a backend table in this contract.
    The auto-title still works in-session; persistence across reloads
    is the queue follow-up.
  - Frontend unit tests: (a) title slot renders the auto-generated
    string when present; (b) falls back to the date when absent;
    (c) the summarize dispatcher is only fired once per thread;
    (d) failure path (no provider) renders the date and does not
    throw.
---

# Scope

Operator (2026-05-18): "Add chat title summary to conversation history
instead of simply date (agent called for chat should summarize chat,
follow standard practice for this — web search for how to do it)."

Standard practice (ChatGPT / Claude / Gemini): fire a cheap-model
summarize prompt after the first model response in a new chat thread,
clamp to ~7 words, persist on the thread record. This contract
implements that pattern.

Provider selection follows the same provider-aware-default posture as
the review-agent fix (`qa-review-agent-provider-config`): use the
first configured provider with a usable model; surface a single
`tracing::warn!` if none, fall back to the date label.

# Out of scope

- Building a server-side conversation/thread table if one does not
  exist today. Queue note instead.
- Re-summarization on pivot, manual rename, multi-language title
  i18n, title moderation. All V2.
- Changing the chat-rail surface itself (still the same rail, same
  popup-free routing — just a different label per item).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/chat-history-auto-title status
git -C .worktrees/chat-history-auto-title log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/chat-history-auto-title \
  -b task/chat-history-auto-title origin/main
```

# Notes

Worker should pick the smallest-context cheap model the operator has
configured (haiku, gpt-4o-mini, qwen-7b, ministral 8b) for the
summarizer. Avoid hardcoding a specific model — read whichever the
configured providers expose at the cheapest tier.

Append checkpoints / PR links below.
