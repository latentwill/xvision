import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import {
  deleteToolPolicy,
  listToolPolicies,
  setToolPolicy,
  toolPolicyKeys,
  type EffectiveToolPolicy,
} from "@/api/chat_rail";

const SCOPE = "global";

export function SettingsToolPolicyRoute() {
  const q = useQuery({
    queryKey: toolPolicyKeys.list(SCOPE),
    queryFn: () => listToolPolicies(SCOPE),
  });

  if (q.isPending) {
    return (
      <Card className="p-6 animate-pulse">
        <div className="h-4 w-48 bg-surface-elev rounded mb-3" />
        <div className="h-4 w-72 bg-surface-elev rounded" />
      </Card>
    );
  }

  if (q.isError || !q.data) {
    const detail =
      q.error instanceof ApiError
        ? `${q.error.code}: ${q.error.message}`
        : q.error instanceof Error
          ? q.error.message
          : "unknown error";
    return (
      <Card className="p-6">
        <p className="m-0 text-danger text-[13px] font-mono">{detail}</p>
        <button
          onClick={() => q.refetch()}
          className="mt-3 px-3 py-1.5 rounded text-[12px] border border-border text-text hover:border-text-3"
        >
          Retry
        </button>
      </Card>
    );
  }

  const read = q.data.filter((r) => r.class === "read");
  const write = q.data.filter((r) => r.class === "write");

  return (
    <div className="space-y-5">
      <Card className="p-5">
        <h3 className="m-0 mb-1 font-sans font-medium text-[16px] tracking-tight">
          Tool policy
        </h3>
        <p className="m-0 mb-4 text-text-3 text-[12px] leading-snug">
          Controls which tools the chat-rail agent can invoke and whether Write
          tools auto-run in Act mode without an approval round-trip. Overrides
          apply workspace-wide (global scope). Reset reverts a tool to its class
          default.
        </p>
        <PolicySection title="Read tools" rows={read} scope={SCOPE} />
      </Card>
      <Card className="p-5">
        <PolicySection title="Write tools" rows={write} scope={SCOPE} />
      </Card>
    </div>
  );
}

function PolicySection({
  title,
  rows,
  scope,
}: {
  title: string;
  rows: EffectiveToolPolicy[];
  scope: string;
}) {
  return (
    <div>
      <div className="text-[11px] font-medium text-text-3 uppercase tracking-widest mb-2">
        {title}
      </div>
      <div className="divide-y divide-border-soft">
        {rows.map((row) => (
          <PolicyRow key={row.tool_name} row={row} scope={scope} />
        ))}
      </div>
    </div>
  );
}

function PolicyRow({
  row,
  scope,
}: {
  row: EffectiveToolPolicy;
  scope: string;
}) {
  const qc = useQueryClient();
  const invalidate = () =>
    qc.invalidateQueries({ queryKey: toolPolicyKeys.list(scope) });

  const setMut = useMutation({
    mutationFn: (patch: { enabled: boolean; auto_approve: boolean }) =>
      setToolPolicy(row.tool_name, patch, scope),
    onSuccess: invalidate,
  });

  const resetMut = useMutation({
    mutationFn: () => deleteToolPolicy(row.tool_name, scope),
    onSuccess: invalidate,
  });

  const isPending = setMut.isPending || resetMut.isPending;

  return (
    <div className="flex items-center gap-3 py-2.5 min-w-0">
      <code className="font-mono text-[12px] text-text flex-1 truncate min-w-0">
        {row.tool_name}
      </code>
      <div className="flex items-center gap-4 shrink-0">
        <Toggle
          label="enabled"
          checked={row.enabled}
          disabled={isPending}
          onChange={(v) =>
            setMut.mutate({ enabled: v, auto_approve: row.auto_approve })
          }
        />
        {row.class === "write" && (
          <Toggle
            label="auto-approve"
            checked={row.auto_approve}
            disabled={isPending}
            onChange={(v) =>
              setMut.mutate({ enabled: row.enabled, auto_approve: v })
            }
          />
        )}
        {row.is_override && (
          <button
            onClick={() => resetMut.mutate()}
            disabled={isPending}
            className="text-[11px] text-text-3 hover:text-text underline-offset-2 hover:underline disabled:opacity-40"
          >
            reset
          </button>
        )}
        {!row.is_override && (
          <span className="text-[11px] text-transparent select-none w-[30px]" />
        )}
      </div>
    </div>
  );
}

function Toggle({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="flex items-center gap-1.5 cursor-pointer select-none">
      <button
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={[
          "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold/60",
          "disabled:opacity-40 disabled:cursor-not-allowed",
          checked ? "bg-gold" : "bg-surface-elev border border-border",
        ].join(" ")}
      >
        <span
          className={[
            "inline-block h-3 w-3 rounded-full bg-white transition-transform shadow-sm",
            checked ? "translate-x-3.5" : "translate-x-0.5",
          ].join(" ")}
        />
      </button>
      <span className="text-[11px] text-text-3">{label}</span>
    </label>
  );
}
