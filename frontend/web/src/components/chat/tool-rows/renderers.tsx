// frontend/web/src/components/chat/tool-rows/renderers.tsx
//
// The concrete tool-row renderers. Each consumes the reducer-projected
// `ToolRow` (status, accumulated `output` text, `outputHash`, exit code,
// denial/error fields) and renders a tool-specific body inside `RowShell`.
//
// The reducer does NOT carry parsed tool args/results (it projects lifecycle +
// accumulated delta text only — see stores/message-row-reducer.ts), so these
// renderers present the streamed `output` text and lifecycle state rather than
// re-parsing structured payloads. They own the framing/labelling per tool; the
// shared shell owns status, policy, and denial.

import { Field, ReadOnlyBadge, RowShell } from "./RowShell";
import type { ToolRowProps } from "./types";

/** Shortened span id for a subtitle line. */
function spanLabel(spanId: string): string {
  return spanId.length > 12 ? `${spanId.slice(0, 8)}…` : spanId;
}

/** The accumulated tool output, rendered as a mono block when present. */
function OutputBlock({ text }: { text: string }) {
  if (!text.trim()) return null;
  return (
    <pre className="mt-1.5 font-mono text-[11px] bg-surface-2/70 p-2 rounded overflow-x-auto whitespace-pre-wrap leading-snug text-text-2">
      {text}
    </pre>
  );
}

// ── Strategy create / update diff ────────────────────────────────────────

export function StrategyDiffRow({ row, onApprove }: ToolRowProps) {
  const verb = row.toolName === "update_manifest" ? "Update" : "Create";
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title={`${verb} strategy`}
      subtitle={`${row.toolName ?? "strategy"} · ${spanLabel(row.spanId)}`}
    >
      <div className="text-text-2">
        Strategy {verb.toLowerCase()} diff
        {row.status === "finished" && row.outputHash ? (
          <Field label="hash">
            <code className="font-mono">{row.outputHash.slice(0, 12)}</code>
          </Field>
        ) : null}
      </div>
      <OutputBlock text={row.output} />
    </RowShell>
  );
}

// ── Agent slot update diff ───────────────────────────────────────────────

export function AgentSlotDiffRow({ row, onApprove }: ToolRowProps) {
  const verb =
    row.toolName === "create_strategy_agent" || row.toolName === "attach_agent"
      ? "Attach agent"
      : "Update agent slot";
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title={verb}
      subtitle={`${row.toolName ?? "slot"} · ${spanLabel(row.spanId)}`}
    >
      <div className="text-text-2">Agent slot diff</div>
      <OutputBlock text={row.output} />
    </RowShell>
  );
}

// ── A/B compare result ───────────────────────────────────────────────────

export function AbCompareRow({ row, onApprove }: ToolRowProps) {
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title="A/B compare"
      subtitle={spanLabel(row.spanId)}
    >
      <div className="text-text-2">
        {row.status === "finished" ? "Comparison result" : "Comparing arms…"}
      </div>
      <OutputBlock text={row.output} />
    </RowShell>
  );
}

// ── Backtest / eval run status ───────────────────────────────────────────

export function EvalRunRow({ row, onApprove }: ToolRowProps) {
  const finished = row.status === "finished";
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title="Eval run"
      subtitle={`${row.toolName ?? "run_eval"} · ${spanLabel(row.spanId)}`}
    >
      <div className="text-text-2">
        {finished
          ? "Eval run complete"
          : row.status === "failed"
            ? "Eval run failed"
            : "Eval run in progress…"}
      </div>
      {finished && row.exitCode !== null ? (
        <Field label="exit">
          <code className="font-mono">{row.exitCode}</code>
        </Field>
      ) : null}
      <OutputBlock text={row.output} />
    </RowShell>
  );
}

// ── Optimizer progress ───────────────────────────────────────────────────

export function OptimizerProgressRow({ row, onApprove }: ToolRowProps) {
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title="Optimizer"
      subtitle={spanLabel(row.spanId)}
    >
      <div className="text-text-2">
        {row.status === "finished"
          ? "Optimization complete"
          : "Optimizer running…"}
      </div>
      <OutputBlock text={row.output} />
    </RowShell>
  );
}

// ── Checkpoint restore ───────────────────────────────────────────────────

export function CheckpointRestoreRow({ row, onApprove }: ToolRowProps) {
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title="Checkpoint restore"
      subtitle={spanLabel(row.spanId)}
    >
      <div className="text-text-2">
        {row.status === "finished"
          ? "Checkpoint restored"
          : row.status === "failed"
            ? "Restore failed"
            : "Restoring checkpoint…"}
      </div>
      <OutputBlock text={row.output} />
    </RowShell>
  );
}

// ── Focus chain edit ─────────────────────────────────────────────────────

export function FocusChainEditRow({ row, onApprove }: ToolRowProps) {
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title="Focus chain edit"
      subtitle={spanLabel(row.spanId)}
    >
      <div className="text-text-2">Focus chain updated</div>
      <OutputBlock text={row.output} />
    </RowShell>
  );
}

// ── Generic read-only fallback ───────────────────────────────────────────

export function GenericReadToolRow({ row, onApprove }: ToolRowProps) {
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title={row.toolName ?? "tool"}
      subtitle={spanLabel(row.spanId)}
      badge={<ReadOnlyBadge />}
    >
      <div className="text-text-2">
        Read-only tool — inspection only, no state change.
      </div>
      <OutputBlock text={row.output} />
    </RowShell>
  );
}

// ── Unsupported write fallback ───────────────────────────────────────────

export function UnsupportedWriteToolRow({ row, onApprove }: ToolRowProps) {
  return (
    <RowShell
      row={row}
      onApprove={onApprove}
      title={row.toolName ?? "tool"}
      subtitle={spanLabel(row.spanId)}
      badge={
        <span className="rounded-full border border-danger/40 px-1.5 py-0.5 text-[10px] text-danger">
          unsupported write
        </span>
      }
    >
      <div className="text-[12px] text-danger leading-[1.4]">
        Unregistered write tool. It cannot execute in Act mode until it is
        registered with a tool policy.
      </div>
      {row.output.trim() ? <OutputBlock text={row.output} /> : null}
    </RowShell>
  );
}
