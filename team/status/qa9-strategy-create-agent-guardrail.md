# qa9-strategy-create-agent-guardrail

Date: 2026-05-14
Branch: qa9-strategy-create-agent-guardrail
Claim: team/queue/qa9-strategy-create-agent-guardrail__2026-05-14T134156Z__claim.md

## Summary

- Added an explicit strategy-agent readiness checklist to the new strategy form.
- Changed draft validation so zero-agent strategy drafts report not eval-ready.
- Pinned authoring and API tests to require the missing attached-agent validation error.

## Verification

- PASS: `corepack pnpm --dir frontend/web test -- strategies-new`
- PASS: `corepack pnpm --dir frontend/web typecheck`
- PASS: `git diff --check`
- NOT RUN: Rust tests / `cargo test` because this session is on the deploy host and `CLAUDE.md` forbids Cargo on deploy hosts.
- BLOCKED BASELINE: `corepack pnpm --dir frontend/web test` still fails on `src/routes-code-splitting.test.ts` because this branch starts from `origin/main`, where `frontend/web/src/components/shell/Layout.tsx` does not lazy-load ChatRail. That code-splitting regression is addressed by `qa9-chat-rail-inflight-controls`.
