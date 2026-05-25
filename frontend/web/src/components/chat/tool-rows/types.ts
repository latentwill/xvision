// frontend/web/src/components/chat/tool-rows/types.ts
//
// Shared types for the tool-row registry (Phase 2.1).
//
// A tool row is the rendered projection of a reducer `ToolRow` (one per
// span_id; see stores/message-row-reducer.ts). The registry maps a tool name
// to a renderer component; `resolveToolRow` falls back to a generic read-only
// renderer or an explicit unsupported-write renderer for unknown tools, keyed
// on the tool's side-effect class.

import type { ComponentType } from "react";

import type { ToolRow } from "@/stores/message-row-reducer";

/**
 * Side-effect class of a tool, mirroring the Rust `SideEffectLevel`
 * (`crates/xvision-observability/src/types.rs`, snake_case wire values) which
 * rides on `ToolCallStartedEvent.side_effect_level` (the `tool_requested`
 * payload). `pure` / `read_only` / `external_read` are read-only; only
 * `external_write` mutates.
 */
export type SideEffectClass =
  | "pure"
  | "read_only"
  | "external_read"
  | "external_write";

/** Read-only side-effect classes — anything that does not mutate state. */
export const READ_ONLY_SIDE_EFFECTS: readonly SideEffectClass[] = [
  "pure",
  "read_only",
  "external_read",
];

/** Is this side-effect class a write (mutating) class? */
export function isWriteSideEffect(
  sideEffect: SideEffectClass | string | null | undefined,
): boolean {
  return sideEffect === "external_write";
}

/**
 * Props every tool-row renderer receives.
 *
 * `row` is the reducer-projected tool row (status, output, denial code,
 * policy outcome, …) — the registry RENDERS this, it never mutates the row
 * shape. `onApprove` is the inline approval handler wired by the host (the
 * rail); a renderer surfacing a NeedsApproval policy check calls it instead of
 * opening any popup. It is optional so rows render read-only in contexts (the
 * trace dock, tests) that do not supply one.
 */
export type ToolRowProps = {
  row: ToolRow;
  /** Invoked by the inline approval affordance. Receives the span id. */
  onApprove?: (spanId: string) => void;
};

/** A tool-row renderer component. */
export type ToolRowRenderer = ComponentType<ToolRowProps>;

/** A registry entry: the renderer plus the tool's declared side-effect class. */
export type ToolRowEntry = {
  /** Renderer for the tool's body (the diff / status / result content). */
  render: ToolRowRenderer;
  /** Declared side-effect class — drives the read/write fallback selection
   *  when an entry is absent and informs the header label. */
  sideEffect: SideEffectClass;
  /** Human label for the tool action, used in the row header. */
  label: string;
};
