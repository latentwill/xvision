-- 036_agents_scope_strategy_id.sql — Phase 3 of the agent-firing-filter
-- operator surface (the spec's L4 amendment to Decision 8).
--
-- Spec: docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md
-- Contract: team/contracts/agent-firing-filter-strategy-composer.md
--
-- Adds a nullable `scope_strategy_id` column to the `agents` table.
-- This is the single schema cost of the "Save as reusable agent"
-- toggle in the strategy editor's inline Filter composer. The toggle
-- defaults ON; when an operator opts out, the inline-authored Filter
-- agent persists with `scope_strategy_id = <strategy_id>` and is
-- hidden from the workspace agent list (default `GET /api/agents`
-- returns only rows where `scope_strategy_id IS NULL`).
--
-- The column is TEXT NULL with no DEFAULT — legacy rows read back as
-- `None` on the Rust side (`Option<String>::default()`), preserving
-- today's "every agent is workspace-visible" behavior. No backfill
-- pass is required.
--
-- No foreign key to `strategies(id)`. Strategies are stored on the
-- filesystem (FilesystemStore), not in SQLite — there is no
-- `strategies` table to reference. Scoped agents become orphan rows if
-- the owning strategy is deleted; a follow-up janitor can sweep them
-- (out of scope for this PR per the contract's risk note).

ALTER TABLE agents ADD COLUMN scope_strategy_id TEXT NULL;

CREATE INDEX IF NOT EXISTS idx_agents_scope_strategy_id ON agents(scope_strategy_id);
