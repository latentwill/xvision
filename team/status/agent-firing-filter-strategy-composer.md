---
track: agent-firing-filter-strategy-composer
worker: claude-opus
phase: in-progress
last_update: 2026-05-23
---

# Status

Claimed 2026-05-23 after confirming all upstream deps have merged:

- agent-graph Phase A schema (#527) ✅
- agent-graph Phase B dispatch (#546) + caveat fix (#550) ✅
- agent-graph Phase C filter (#551) ✅
- agent-graph Phase D recorder (#552) ✅
- agent-graph Phase E templates (#549) ✅
- agent-graph runtime restore (#553) + test repair (#554) ✅
- agent-firing-filter Phase 1 awareness (#548) ✅

Phase 2 (CLI verbs) remains deferred but is not a prerequisite for Phase 3.

## Migration

Reserving **036** for `agents_scope_strategy_id` (the L4 schema amendment).
Registry note added in this PR.

## Scope adaptation

Contract's `allowed_paths` lists `frontend/web/src/components/strategy/**`,
which doesn't exist on disk. Strategy authoring lives in
`frontend/web/src/routes/authoring.tsx`. Inline-composer components are
created under the contract-scoped folder; the existing route is
extended to mount them. PR description calls the deviation out.

The contract's verification mentions `pnpm --filter web e2e` and
`pnpm --filter web lint`. Neither script exists in `frontend/web/package.json`
today. Verification runs `typecheck` + `test`.

## Approach

1. Engine: migration 036 + `Agent.scope_strategy_id` field + `AgentStore`
   persistence + `ListAgentsRequest.scope` filter + integration test.
2. Regenerate TS types so the Phase A `AgentRef.activates`,
   `PipelineEdge.condition`, `EdgePredicate`, and `Capability` shapes
   reach the SPA (currently stale).
3. SPA: `components/strategy/FiringSection.tsx` +
   `components/strategy/InlineFilterComposer.tsx`; mount inside the
   AgentRef cards in `routes/authoring.tsx`.
4. Verify, open PR.
