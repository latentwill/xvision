// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
//
// Per-filter run aggregate computed by the engine from the `FilterEventV1`
// stream and surfaced on the run export. Spec:
// `docs/superpowers/specs/2026-05-21-filter-v1.md` §Export shape.

export type FilterSummary = {
  filter_id: string;
  bars_scanned: number;
  wakeups: number;
  suppressed_in_position: number;
  suppressed_daily_cap: number;
  suppressed_cooldown: number;
  /** bars_scanned − wakeups (in FilterGated mode). */
  llm_calls_saved: number;
  /** llm_calls_saved × `avg_briefing_token_cost` (v1 constant: 50_000). */
  estimated_tokens_saved: number;
};
