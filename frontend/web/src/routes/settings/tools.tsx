import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError } from "@/api/client";
import {
  dataToolsKeys,
  getDataTools,
  setDataTools,
} from "@/api/dataTools";
import type { DataToolEntry } from "@/api/dataTools";

// ── helpers ──────────────────────────────────────────────────────────────────

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}

const KIND_LABELS: Record<DataToolEntry["kind"], string> = {
  nansen: "Nansen",
  elfa: "Elfa",
};

// ── Main route ───────────────────────────────────────────────────────────────

export function SettingsToolsRoute() {
  const qc = useQueryClient();

  const query = useQuery({
    queryKey: dataToolsKeys.list(),
    queryFn: getDataTools,
  });

  const save = useMutation({
    mutationFn: (entries: DataToolEntry[]) =>
      setDataTools({ data_tools: entries }),
    onSuccess: () => qc.invalidateQueries({ queryKey: dataToolsKeys.list() }),
  });

  // Local working copy: mirrors the server list; edits are batched and
  // sent on "Save changes" to match the PUT-the-whole-list API shape.
  const [draft, setDraft] = useState<DataToolEntry[] | null>(null);

  // If the server list changes (e.g. after a save) and we have no pending
  // draft, reset the draft to the new server data.
  const serverList = query.data?.data_tools ?? [];
  const workingList: DataToolEntry[] = draft ?? serverList;

  function patchEntry(index: number, patch: Partial<DataToolEntry>) {
    const next = workingList.map((e, i) =>
      i === index ? { ...e, ...patch } : e,
    );
    setDraft(next);
  }

  const isDirty =
    draft !== null &&
    JSON.stringify(draft) !== JSON.stringify(serverList);

  function handleSave() {
    save.mutate(workingList, {
      onSuccess: () => setDraft(null),
    });
  }

  function handleReset() {
    setDraft(null);
  }

  // ── Loading / error guards ─────────────────────────────────────────────────

  if (query.isPending) {
    return (
      <div className="space-y-5">
        <PageHeader />
        <div className="animate-pulse rounded-md border border-border-soft bg-surface-elev/30 p-4 h-20" />
      </div>
    );
  }

  if (query.isError) {
    return (
      <div className="space-y-5">
        <PageHeader />
        <div className="rounded-md border border-danger/40 bg-danger/[0.06] p-4 text-[13px] text-danger">
          {errorMessage(query.error)}
          <button
            onClick={() => query.refetch()}
            className="ml-3 px-2 py-0.5 rounded border border-danger/40 text-[12px] hover:border-danger"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // ── Empty state ───────────────────────────────────────────────────────────

  if (serverList.length === 0) {
    return (
      <div className="space-y-5">
        <PageHeader />
        <p className="text-[13px] text-text-3">
          No data tools configured. Add a{" "}
          <code className="font-mono">[[data_tools]]</code> block to{" "}
          <code className="font-mono">config/default.toml</code> to enable
          Nansen or Elfa.
        </p>
      </div>
    );
  }

  // ── Populated list ────────────────────────────────────────────────────────

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between mb-2">
        <PageHeader />
        <div className="flex items-center gap-2">
          {isDirty ? (
            <button
              type="button"
              onClick={handleReset}
              disabled={save.isPending}
              className="text-[12px] text-text-3 hover:text-text disabled:opacity-40"
            >
              Reset
            </button>
          ) : null}
          <button
            type="button"
            onClick={handleSave}
            disabled={!isDirty || save.isPending}
            className="px-3 py-1.5 rounded text-[12px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 motion-safe:active:scale-[0.96]"
          >
            {save.isPending ? "Saving…" : "Save changes"}
          </button>
        </div>
      </div>

      {save.isError ? (
        <div className="rounded-md border border-danger/40 bg-danger/[0.06] p-3 text-[12px] text-danger">
          {errorMessage(save.error)}
        </div>
      ) : null}

      {save.isSuccess && !isDirty ? (
        <div className="rounded-md border border-info/40 bg-info/[0.06] p-3 text-[12px] text-info">
          Saved.
        </div>
      ) : null}

      <div className="rounded-md border border-border-soft overflow-hidden">
        <table className="w-full">
          <thead>
            <tr className="bg-surface-elev/40 border-b border-border-soft">
              <th className="py-2 pl-5 pr-3 text-left text-[11px] text-text-3 font-medium uppercase tracking-wider">
                Kind
              </th>
              <th className="py-2 pr-3 text-left text-[11px] text-text-3 font-medium uppercase tracking-wider">
                Base URL
              </th>
              <th className="py-2 pr-3 text-left text-[11px] text-text-3 font-medium uppercase tracking-wider">
                Env var
              </th>
              <th className="py-2 pr-3 text-center text-[11px] text-text-3 font-medium uppercase tracking-wider">
                Enabled
              </th>
              <th className="py-2 pr-3 text-right text-[11px] text-text-3 font-medium uppercase tracking-wider">
                Budget
              </th>
              <th className="py-2 pr-5 text-right text-[11px] text-text-3 font-medium uppercase tracking-wider">
                Lag days
              </th>
            </tr>
          </thead>
          <tbody>
            {workingList.map((entry, idx) => (
              <DataToolRow
                key={`${entry.kind}-${idx}`}
                entry={entry}
                onChange={(patch) => patchEntry(idx, patch)}
                disabled={save.isPending}
              />
            ))}
          </tbody>
        </table>
      </div>

      <p className="m-0 text-text-3 text-[11px] leading-snug">
        <code className="font-mono">api_key_env</code> is the environment
        variable NAME — the secret is read from the environment at runtime and
        never stored in the config or surfaced here.
      </p>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function PageHeader() {
  return (
    <div>
      <h3 className="m-0 font-sans font-medium text-[20px] tracking-tight">
        Data tools
      </h3>
      <p className="m-0 mt-1 text-text-3 text-[12px]">
        External data providers xvn can call during strategy evaluation
        (Nansen on-chain analytics, Elfa social signals).
      </p>
    </div>
  );
}

function DataToolRow({
  entry,
  onChange,
  disabled,
}: {
  entry: DataToolEntry;
  onChange: (patch: Partial<DataToolEntry>) => void;
  disabled: boolean;
}) {
  return (
    <tr className="border-t border-border-soft align-middle hover:bg-surface-elev/20">
      <td className="py-3 pl-5 pr-3">
        <span className="inline-flex items-center gap-1.5">
          <span className="text-[13px] font-medium text-text">
            {KIND_LABELS[entry.kind] ?? entry.kind}
          </span>
          <code className="font-mono text-[10px] text-text-4">
            {entry.kind}
          </code>
        </span>
      </td>
      <td className="py-3 pr-3">
        <code className="font-mono text-[12px] text-text-2">
          {entry.base_url}
        </code>
      </td>
      <td className="py-3 pr-3">
        <code className="font-mono text-[12px] text-text-2">
          {entry.api_key_env}
        </code>
      </td>
      <td className="py-3 pr-3 text-center">
        <label className="inline-flex items-center gap-1.5 cursor-pointer">
          <input
            type="checkbox"
            aria-label="enabled"
            checked={entry.enabled}
            disabled={disabled}
            onChange={(e) => onChange({ enabled: e.target.checked })}
            className="w-4 h-4 accent-gold disabled:opacity-50"
          />
          <span className="text-[12px] text-text-2 sr-only">enabled</span>
        </label>
      </td>
      <td className="py-3 pr-3 text-right">
        {entry.budget_credits_per_run !== null ? (
          <span className="text-[12px] text-text-2">
            {entry.budget_credits_per_run}
          </span>
        ) : (
          <span className="text-[12px] text-text-3">—</span>
        )}
      </td>
      <td className="py-3 pr-5 text-right">
        {entry.nansen_lookahead_lag_days !== null ? (
          <span className="text-[12px] text-text-2">
            {entry.nansen_lookahead_lag_days}d
          </span>
        ) : (
          <span className="text-[12px] text-text-3">—</span>
        )}
      </td>
    </tr>
  );
}
