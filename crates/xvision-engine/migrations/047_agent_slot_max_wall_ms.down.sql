-- 047_agent_slot_max_wall_ms.down.sql
--
-- Reverse of 047: drop the per-slot wall-clock budget column. SQLite
-- supports DROP COLUMN since 3.35 (2021-03-12); guarded with no IF
-- EXISTS because this script is only ever run as the paired down for
-- the upgrade above.

ALTER TABLE agent_slots DROP COLUMN max_wall_ms;
