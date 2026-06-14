// frontend/web/src/components/chat/tool-rows/registry.ts
//
// The tool-row registry (Phase 2.1). Maps a tool NAME to a renderer + its
// declared side-effect class. `resolveToolRow(toolName, sideEffect)` returns
// the registered renderer, else a fallback chosen by the tool's side-effect
// class: a GENERIC read-only renderer for read tools, an explicit UNSUPPORTED
// renderer for write tools.
//
// COMPLETENESS CONTRACT (asserted by registry.test.ts):
//   Every tool name the engine knows about (KNOWN_TOOLS — mirrored from the
//   Rust classifier in crates/xvision-engine/src/chat_session/tool_policy.rs)
//   MUST have a registry entry OR be in WAIVED_TOOLS. Adding a new engine tool
//   without a registry entry (and without explicitly waiving it) fails the
//   completeness test loudly.

import {
  AgentSlotDiffRow,
  CheckpointRestoreRow,
  EvalRunRow,
  FocusChainEditRow,
  GenericReadToolRow,
  OptimizerProgressRow,
  StrategyDiffRow,
  UnsupportedWriteToolRow,
} from "./renderers";
import {
  isWriteSideEffect,
  type SideEffectClass,
  type ToolRowEntry,
  type ToolRowRenderer,
} from "./types";

/**
 * The registry, keyed by tool name. Each entry pins the tool's renderer, its
 * declared side-effect class, and a human label.
 *
 * Read tools share the generic read renderer (they are inspection verbs with
 * no bespoke diff to show); write/authoring tools get a dedicated renderer.
 */
export const TOOL_ROW_REGISTRY: Record<string, ToolRowEntry> = {
  // ── Strategy authoring (create / update diff) ──
  create_strategy: {
    render: StrategyDiffRow,
    sideEffect: "external_write",
    label: "Create strategy",
  },
  update_manifest: {
    render: StrategyDiffRow,
    sideEffect: "external_write",
    label: "Update strategy",
  },
  create_scenario: {
    render: StrategyDiffRow,
    sideEffect: "external_write",
    label: "Create scenario",
  },

  // ── Agent slot update diff ──
  update_slot: {
    render: AgentSlotDiffRow,
    sideEffect: "external_write",
    label: "Update agent slot",
  },
  create_strategy_agent: {
    render: AgentSlotDiffRow,
    sideEffect: "external_write",
    label: "Create strategy agent",
  },
  attach_agent: {
    render: AgentSlotDiffRow,
    sideEffect: "external_write",
    label: "Attach agent",
  },
  set_risk_config: {
    render: AgentSlotDiffRow,
    sideEffect: "external_write",
    label: "Set risk config",
  },
  set_filter: {
    render: AgentSlotDiffRow,
    sideEffect: "external_write",
    label: "Set filter",
  },
  clear_filter: {
    render: AgentSlotDiffRow,
    sideEffect: "external_write",
    label: "Clear filter",
  },
  validate_draft: {
    render: AgentSlotDiffRow,
    sideEffect: "external_write",
    label: "Validate draft",
  },

  // ── Backtest / eval run status ──
  run_eval: {
    render: EvalRunRow,
    sideEffect: "external_write",
    label: "Run eval",
  },
  fetch_bars: {
    render: EvalRunRow,
    sideEffect: "external_read",
    label: "Fetch bars",
  },

  // ── Optimizer progress ──
  run_optimizer: {
    render: OptimizerProgressRow,
    sideEffect: "external_write",
    label: "Run optimizer",
  },

  // ── Checkpoint restore ──
  restore_checkpoint: {
    render: CheckpointRestoreRow,
    sideEffect: "external_write",
    label: "Restore checkpoint",
  },

  // ── Focus chain edit ──
  edit_focus_chain: {
    render: FocusChainEditRow,
    sideEffect: "external_write",
    label: "Edit focus chain",
  },

  // ── Read / inspection verbs (generic read renderer) ──
  get_strategy: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "Get strategy",
  },
  get_scenario: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "Get scenario",
  },
  get_eval_run: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "Get eval run",
  },
  get_eval_review: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "Get eval review",
  },
  get_cli_job: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "Get CLI job",
  },
  get_cli_job_output: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "Get CLI job output",
  },
  list_strategies: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "List strategies",
  },
  list_scenarios: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "List scenarios",
  },
  list_eval_runs: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "List eval runs",
  },
  list_eval_reviews: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "List eval reviews",
  },
  list_strategies_folder: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "List strategies folder",
  },
  read_strategies_file: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "Read strategies file",
  },
  list_strategy_ideas: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "List strategy ideas",
  },
  resolve_strategy: {
    render: GenericReadToolRow,
    sideEffect: "read_only",
    label: "Resolve strategy",
  },
};

/**
 * Tools the engine knows about (KNOWN_TOOLS) that intentionally do NOT have a
 * bespoke registry entry — they render via the side-effect fallback. The
 * completeness test treats a waived tool as covered. Keep this set EMPTY unless
 * a tool is deliberately handled only by the generic/unsupported fallback;
 * adding a name here is the explicit "I chose not to register this" escape
 * hatch the test points at.
 */
export const WAIVED_TOOLS: ReadonlySet<string> = new Set<string>([]);

/**
 * The full set of tool names the engine can emit, mirrored from the Rust
 * classifier (`classify` in chat_session/tool_policy.rs). This is the contract
 * the completeness test asserts against: every name here must be in the
 * registry or in WAIVED_TOOLS.
 *
 * NOTE: the Rust classifier lists READ + WRITE verbs explicitly; unknown names
 * fail safe to Write there. The registry mirrors the explicit names plus the
 * net-new authoring verbs surfaced in this phase (ab_compare, run_optimizer,
 * restore_checkpoint, edit_focus_chain).
 */
export const KNOWN_TOOLS: readonly string[] = [
  // Read (from the Rust classifier Read arm)
  "get_strategy",
  "get_scenario",
  "get_eval_run",
  "get_eval_review",
  "get_cli_job",
  "get_cli_job_output",
  "list_strategies",
  "list_scenarios",
  "list_eval_runs",
  "list_eval_reviews",
  "list_strategies_folder",
  "read_strategies_file",
  "list_strategy_ideas",
  "resolve_strategy",
  // Write (from the Rust classifier Write arm)
  "create_strategy",
  "create_scenario",
  "create_strategy_agent",
  "update_slot",
  "update_manifest",
  "set_risk_config",
  "set_filter",
  "clear_filter",
  "attach_agent",
  "validate_draft",
  "run_eval",
  "fetch_bars",
  // Net-new authoring / progress verbs surfaced in this phase
  "run_optimizer",
  "restore_checkpoint",
  "edit_focus_chain",
];

/**
 * Resolve the renderer for a tool. Returns the registered renderer when one
 * exists; otherwise falls back by side-effect class:
 *   - write (external_write)  → the explicit UNSUPPORTED-write renderer
 *   - read (everything else)  → the GENERIC read-only renderer
 *
 * `sideEffect` is the tool's declared `side_effect_level` from the
 * `tool_requested` payload. A missing / unknown value is treated as a write
 * (fail safe — mirrors the Rust classifier's unknown→Write rule), so an
 * unrecognised verb surfaces as unsupported rather than silently read-only.
 */
export function resolveToolRow(
  toolName: string | null | undefined,
  sideEffect: SideEffectClass | string | null | undefined,
): ToolRowRenderer {
  if (toolName) {
    const entry = TOOL_ROW_REGISTRY[toolName];
    if (entry) return entry.render;
  }
  // Unknown tool: choose fallback by side-effect class. Unknown/missing
  // side-effect → treat as write (fail safe).
  return isWriteSideEffect(sideEffect ?? "external_write")
    ? UnsupportedWriteToolRow
    : GenericReadToolRow;
}

/** Is this tool name covered (has an entry or is explicitly waived)? */
export function isToolCovered(toolName: string): boolean {
  return toolName in TOOL_ROW_REGISTRY || WAIVED_TOOLS.has(toolName);
}
