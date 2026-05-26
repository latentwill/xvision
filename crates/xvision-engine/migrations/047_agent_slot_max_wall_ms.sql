-- 047_agent_slot_max_wall_ms.sql
--
-- QA30 follow-on (2026-05-26): surface a per-slot wall-clock budget so
-- operators can pin a hard ceiling on cycle time from the agent form.
--
-- Background: the Cline runtime's wall budget used to be a hardcoded
-- 120s default, which clipped slow-but-healthy model completions
-- (Gemini Flash 3.1-lite under load, Sonnet with extended thinking)
-- and surfaced them as `budget_wall_ms_exceeded` failures. The QA30
-- round set `DEFAULT_MAX_WALL_MS = u32::MAX` (no enforcement) and
-- plumbed `max_wall_ms: Option<u32>` through `DispatchInput` ->
-- `ClineSlotInput`; this migration completes the chain by giving
-- `agent_slots` a persisted column the agent form can write to.
--
-- Sentinel convention mirrors `max_tokens`: stored as a non-null
-- INTEGER with `0` meaning "unset" (use the runtime's no-enforcement
-- default). Non-zero positive integers are honoured verbatim.
-- Idempotent via the `migrate_agent_slot_max_wall_ms` helper's column
-- probe.

ALTER TABLE agent_slots
    ADD COLUMN max_wall_ms INTEGER NOT NULL DEFAULT 0;
