/**
 * AutoOptimizer SSE event label registry — static SPA fallback.
 *
 * Mirrors crates/xvision-dashboard/src/sse/autooptimizer_labels.rs::display_label.
 * Kept in sync via the SSE registry handoff:
 *   docs/design/2026-05-27-autooptimizer-sse-registry-handoff.md
 *
 * Terminology follows the lock at:
 *   docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md §11
 *
 * Maintenance note: when a new CycleProgressEvent variant is added, update
 * both this map and the Rust-side display_label() simultaneously. The Rust
 * unit test (display_label_covers_all_variants) is the exhaustive check.
 *
 * Usage:
 *   import { displayLabel } from './bus.js';
 *   const label = displayLabel(event);  // event is the parsed SSE payload
 *
 * The server sends `display_label` on every payload. This map is a fallback
 * for clients connecting to an older server version that doesn't yet include
 * the field.
 */

// Wire-name → operator-facing display label.
// Wire names are the snake_case type discriminants from CycleProgressEvent.
const DISPLAY_LABELS = {
  cycle_started:          "Optimizer run started",
  parent_selected:        "Parent selected",
  mutation_proposed:      "Experiment proposed",
  no_candidate:           "No experiment produced",
  mutation_gated_passed:  "Experiment kept",
  mutation_gated_dropped: "Experiment dropped",
  honesty_check_run:      "Honesty check result",
  judge_finding:          "Reviewer finished notes",
  eval_progress:          "Backtest progress",
  heartbeat:              "Working…",
  cycle_finished:         "Optimizer run finished",
};

/**
 * Returns the operator-facing display label for a parsed SSE event payload.
 *
 * Prefers `event.display_label` (server-provided), falls back to the local
 * DISPLAY_LABELS map, then falls back to `event.kind` (the raw wire name),
 * and finally returns "Unknown event".
 *
 * @param {object|null|undefined} event - Parsed SSE payload object
 * @returns {string} Display label
 */
export function displayLabel(event) {
  if (event && event.display_label) return event.display_label;
  if (event && event.kind && DISPLAY_LABELS[event.kind]) return DISPLAY_LABELS[event.kind];
  return (event && event.kind) ? event.kind : "Unknown event";
}
