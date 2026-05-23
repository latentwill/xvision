# agent-firing-filter operator surface — closed 2026-05-23

Spec: `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`

Three-phase wave shipped in two days. All phases merged; janitor and TS
type-drift sync landed in a follow-up PR.

| Phase | Contract | PR |
|---|---|---|
| 1. AgentForm awareness card + docs + margin fix | `agent-firing-filter-form-and-docs.md` | #548 |
| 2. CLI verbs (`xvn agent create`, `xvn strategy add-filter`/`remove-filter`, `xvn strategy validate` soft-warn) | `agent-firing-filter-cli-verbs.md` | #555 |
| 3. StrategyForm "When does this fire?" + InlineFilterComposer + `agents.scope_strategy_id` (migration 036) | `agent-firing-filter-strategy-composer.md` | #557 |

Follow-up housekeeping (this PR):
- Synced `frontend/web/src/api/types.gen/` to engine reality
  (Phase A/B/C/D landed without `gen-types` runs).
- Added scoped-agent janitor on `strategy::delete` so the
  toggle-OFF flow's agents don't orphan in the agents table.

Migration registry: 036 (`agents_scope_strategy_id`).

This archive is read-only. Live coordination has moved on; do not
re-open any of these contracts.
