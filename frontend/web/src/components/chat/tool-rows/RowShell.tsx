// frontend/web/src/components/chat/tool-rows/RowShell.tsx
//
// Shared chrome for every tool row: a bordered card with a header (tool label
// + status chip) and a body slot. Renders the policy / denial affordances that
// are common across all tools so individual renderers only own their tool's
// body content.
//
// Repo rule: NO popups/modals/sheets. The approval affordance is an inline
// button that calls the host-supplied `onApprove` handler.

import type { ReactNode } from "react";

import { Pill } from "@/components/primitives/Pill";
import type { ToolRow, ToolRowStatus } from "@/stores/message-row-reducer";

import type { ToolRowProps } from "./types";

const STATUS_TONE: Record<
  ToolRowStatus,
  "default" | "info" | "danger" | "warn"
> = {
  requested: "default",
  policy_checked: "warn",
  approved: "info",
  started: "info",
  finished: "info",
  failed: "danger",
  cancelled: "default",
  denied: "danger",
};

const STATUS_LABEL: Record<ToolRowStatus, string> = {
  requested: "requested",
  policy_checked: "checking policy",
  approved: "approved",
  started: "running",
  finished: "done",
  failed: "failed",
  cancelled: "cancelled",
  denied: "denied",
};

/**
 * The bordered tool-card shell. `header` overrides the default tool-label
 * title; `subtitle` shows a mono secondary line (e.g. span id); `children` is
 * the tool body. The shell always renders status, the denial block, and the
 * inline approval affordance when applicable.
 */
export function RowShell({
  row,
  onApprove,
  title,
  subtitle,
  badge,
  children,
}: ToolRowProps & {
  title: ReactNode;
  subtitle?: ReactNode;
  badge?: ReactNode;
  children?: ReactNode;
}) {
  const running = row.status === "started" || row.status === "requested";

  return (
    <article
      role="group"
      aria-label={`Tool ${row.toolName ?? row.spanId}`}
      data-tool-name={row.toolName ?? undefined}
      data-tool-status={row.status}
      className="rounded-md border border-border-soft bg-surface-card overflow-hidden"
    >
      <header className="px-3 py-2 border-b border-border-soft flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="m-0 text-[13px] font-semibold text-text truncate flex items-center gap-1.5">
            {title}
            {badge}
          </h3>
          {subtitle ? (
            <div className="mt-0.5 font-mono text-[11px] text-text-3 truncate">
              {subtitle}
            </div>
          ) : null}
        </div>
        <Pill tone={STATUS_TONE[row.status]} animated={running}>
          {STATUS_LABEL[row.status]}
        </Pill>
      </header>

      {children ? <div className="px-3 py-2 text-[12px]">{children}</div> : null}

      <PolicyBlock row={row} onApprove={onApprove} />
      <DenialBlock row={row} />
    </article>
  );
}

/**
 * Inline approval affordance. Shown when the latest policy check requires
 * approval and the row has not yet advanced past it. Calls `onApprove` — no
 * popup, no modal. When no handler is wired the button is disabled and labelled
 * so the operator knows approval must happen elsewhere.
 */
function PolicyBlock({ row, onApprove }: ToolRowProps) {
  // Only a NeedsApproval check that has NOT yet been approved/started/finished
  // surfaces the affordance. Once the span advances, the status chip carries
  // the state.
  const needsApproval =
    row.policyOutcome === "needs_approval" &&
    (row.status === "requested" || row.status === "policy_checked");
  if (!needsApproval) return null;

  return (
    <div className="px-3 py-2 border-t border-border-soft bg-warn/5">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[12px] text-text-2">
          Needs approval to run in{" "}
          <span className="font-mono text-text">{row.policyMode ?? "act"}</span>{" "}
          mode.
        </span>
        <button
          type="button"
          disabled={!onApprove}
          onClick={() => onApprove?.(row.spanId)}
          className="px-2 py-1 rounded border border-warn/40 text-[11px] text-warn hover:bg-warn/10 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {onApprove ? "Approve & run" : "Approval required"}
        </button>
      </div>
    </div>
  );
}

/**
 * Denial block. Shown on a `tool_denied` row: the stable machine code plus a
 * remediation hint derived from the code. Read-only — denials are not
 * approvable inline (the operator must change mode or policy elsewhere).
 */
function DenialBlock({ row }: { row: ToolRow }) {
  if (row.status !== "denied" && row.deniedCode === null) return null;

  const remediation = remediationFor(row.deniedCode);
  return (
    <div className="px-3 py-2 border-t border-border-soft bg-danger/5">
      <div className="text-[12px] text-danger flex items-start gap-1.5">
        <span className="leading-[1.4] flex-shrink-0" aria-hidden>
          !
        </span>
        <div className="min-w-0">
          <div className="leading-[1.4]">
            Denied
            {row.deniedCode ? (
              <>
                {" "}
                (<code className="font-mono">{row.deniedCode}</code>)
              </>
            ) : null}
            {row.errorMessage ? <>: {row.errorMessage}</> : null}
          </div>
          {remediation ? (
            <div className="mt-1 text-[11px] text-text-2 leading-[1.4]">
              {remediation}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

/** Map a known denial code to an operator-facing remediation hint. */
function remediationFor(code: string | null): string | null {
  switch (code) {
    case "write_tool_in_research_mode":
      return "Switch the session to Act mode to allow write tools to run.";
    case "tool_disabled":
      return "Enable this tool in Settings → Tool policy, then retry.";
    case "policy_denied":
      return "This tool is denied by the active tool policy. Update the policy to allow it.";
    default:
      return code
        ? "Adjust the session mode or tool policy, then retry the action."
        : null;
  }
}

/** Small labelled key/value line used inside tool bodies. */
export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-[11px] text-text-3 flex-shrink-0">{label}</span>
      <span className="text-[12px] text-text-2 min-w-0 truncate">
        {children}
      </span>
    </div>
  );
}

/** A read-only-tool badge for the generic fallback. */
export function ReadOnlyBadge() {
  return (
    <span className="rounded-full border border-border-soft px-1.5 py-0.5 text-[10px] text-text-3">
      read-only tool
    </span>
  );
}
