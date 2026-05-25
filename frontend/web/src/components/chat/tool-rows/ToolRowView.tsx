// frontend/web/src/components/chat/tool-rows/ToolRowView.tsx
//
// The dispatcher: given a reducer-projected `ToolRow`, resolve the registered
// renderer (or a side-effect fallback) and render it. This is the single entry
// point the chat rail / trace dock use to render a tool row.
//
// The reducer row does not carry the tool's `side_effect_level`, so the host
// may thread it explicitly via `sideEffect` (sourced from the originating
// `tool_requested` payload). When absent, `resolveToolRow` falls back per the
// fail-safe (unknown → write → unsupported).

import type { ToolRow } from "@/stores/message-row-reducer";

import { resolveToolRow } from "./registry";
import type { SideEffectClass } from "./types";

export function ToolRowView({
  row,
  sideEffect,
  onApprove,
}: {
  row: ToolRow;
  /** Declared side-effect class from the originating tool_requested payload. */
  sideEffect?: SideEffectClass | string | null;
  onApprove?: (spanId: string) => void;
}) {
  const Renderer = resolveToolRow(row.toolName, sideEffect);
  return <Renderer row={row} onApprove={onApprove} />;
}
