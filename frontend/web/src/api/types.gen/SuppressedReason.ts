// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
//
// Why a filter that would otherwise have triggered did not dispatch the LLM.
// Mirrors the spec's `SuppressedReason` enum at
// `docs/superpowers/specs/2026-05-21-filter-v1.md` §Export shape. Snake-case
// rename matches what the engine writes to `events.jsonl`.

export type SuppressedReason = "in_position" | "daily_cap" | "cooldown";
