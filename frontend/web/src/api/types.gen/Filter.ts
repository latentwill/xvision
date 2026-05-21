// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
// Mirrors `xvision_filters::Filter`.

import type { ConditionTree } from "./ConditionTree";
import type { FilterStatus } from "./FilterStatus";
import type { ScanCadence } from "./ScanCadence";
import type { WakeInPosition } from "./WakeInPosition";

/**
 * Top-level Filter entity. JSON parses this directly; TOML wraps it under
 * `[filter]` (handled in the engine's parse layer). See the spec at
 * `docs/superpowers/specs/2026-05-21-filter-v1.md`.
 */
export type Filter = {
  id: string;
  strategy_id: string;
  display_name: string;
  description?: string;
  status: FilterStatus;
  asset_scope: Array<string>;
  timeframe: string;
  scan_cadence: ScanCadence;
  conditions: ConditionTree;
  cooldown_bars: number;
  max_wakeups_per_day?: number | null;
  wake_when_in_position: WakeInPosition;
  agent_context_template: string;
};
