// frontend/web/src/components/chat/tool-rows/index.ts
//
// Public surface of the tool-row registry (Phase 2.1).

export { ToolRowView } from "./ToolRowView";
export {
  KNOWN_TOOLS,
  TOOL_ROW_REGISTRY,
  WAIVED_TOOLS,
  isToolCovered,
  resolveToolRow,
} from "./registry";
export {
  AgentSlotDiffRow,
  CheckpointRestoreRow,
  EvalRunRow,
  FocusChainEditRow,
  GenericReadToolRow,
  OptimizerProgressRow,
  StrategyDiffRow,
  UnsupportedWriteToolRow,
} from "./renderers";
export { RowShell } from "./RowShell";
export {
  READ_ONLY_SIDE_EFFECTS,
  isWriteSideEffect,
  type SideEffectClass,
  type ToolRowEntry,
  type ToolRowProps,
  type ToolRowRenderer,
} from "./types";
