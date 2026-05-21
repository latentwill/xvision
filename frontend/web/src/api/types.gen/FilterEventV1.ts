// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
//
// One row per cadence-gated bar in a FilterGated run. Emitted to the run's
// `events.jsonl` alongside V2D `memory_*` events. Spec:
// `docs/superpowers/specs/2026-05-21-filter-v1.md` §Export shape.
//
// `indicator_snapshot` is `BTreeMap<IndicatorRef, f64>` on the Rust side,
// but `IndicatorRef`'s custom serde impl serializes each key as the DSL
// token (`"ema_20"`, `"close"`). The wire shape is therefore a plain
// `Record<string, number>` and that's what the timeline panel reads.

import type { SuppressedReason } from "./SuppressedReason";

export type FilterEventV1 = {
  bar_timestamp: string;
  filter_id: string;
  triggered: boolean;
  suppressed_reason: SuppressedReason | null;
  conditions_passed: Array<string>;
  conditions_failed: Array<string>;
  indicator_snapshot: Record<string, number>;
};
